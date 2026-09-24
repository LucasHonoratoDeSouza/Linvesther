import { createHash, randomBytes } from "node:crypto";
import pg from "pg";
import { afterAll, beforeAll, beforeEach, describe, expect, it } from "vitest";
import {
  MAX_ACTIVE_TOKENS,
  MAX_TOKEN_LENGTH,
  MemoryReadOnlyTokenStore,
  PostgresReadOnlyTokenStore,
  READ_ONLY_SCOPE,
  TooManyReadOnlyTokensError,
  generateReadOnlyToken,
  looksLikeReadOnlyToken,
  readOnlyTokenDigest,
  type ReadOnlyTokenStore,
} from "../src/auth/readOnlyTokenStore.js";
import { requireReadOnlyToken } from "../src/auth/requireReadOnlyToken.js";
import type { FastifyRequest } from "fastify";

// The same behaviour is asserted for the in-memory and the Postgres
// store, so they cannot drift apart. Postgres runs in a schema of its
// own that is dropped afterwards, never touching other tables.
const DATABASE_URL = process.env.DATABASE_URL ?? "postgresql://linvestherzk:linvestherzk-local-dev-only@localhost:5433/linvestherzk";
const schema = `api_token_test_${randomBytes(6).toString("hex")}`;
let admin: pg.Pool;
let pool: pg.Pool;

beforeAll(async () => {
  admin = new pg.Pool({ connectionString: DATABASE_URL });
  await admin.query(`CREATE SCHEMA ${schema}`);
  pool = new pg.Pool({ connectionString: DATABASE_URL, options: `-c search_path=${schema}` });
});

afterAll(async () => {
  await pool.end();
  await admin.query(`DROP SCHEMA ${schema} CASCADE`);
  await admin.end();
});

const ME = "0x1111111111111111111111111111111111111111" as const;
const NOW = new Date("2026-09-01T12:00:00Z");

/** A fresh identity per test: the Postgres store keeps its rows for the
 * whole suite, so two tests sharing an address would see each other's
 * tokens while the in-memory store, rebuilt each time, would not. */
const anAddress = () => `0x${randomBytes(20).toString("hex")}` as `0x${string}`;

/** Only the two fields the middleware reads, so a test cannot
 * accidentally rely on anything else about a request. */
const asRequest = (headers: Record<string, string>, ip = "203.0.113.7") => ({ headers, ip }) as unknown as FastifyRequest;

const bearer = (token: string) => asRequest({ authorization: `Bearer ${token}` });

describe.each<[string, () => Promise<ReadOnlyTokenStore>]>([
  ["memory", async () => new MemoryReadOnlyTokenStore(() => NOW)],
  [
    "postgres",
    async () => {
      const store = new PostgresReadOnlyTokenStore(pool, () => NOW);
      await store.ensureSchema();
      return store;
    },
  ],
])("read-only tokens (%s)", (_name, make) => {
  let me: `0x${string}`;
  let other: `0x${string}`;
  beforeEach(() => {
    me = anAddress();
    other = anAddress();
  });

  it("authenticates the token it just issued, and records the use", async () => {
    const store = await make();
    const { token, record } = await store.issue(me, "Hermes");
    expect(record.lastUsedAt).toBeNull();
    expect(record.scope).toBe(READ_ONLY_SCOPE);

    const used = new Date("2026-09-02T09:30:00Z");
    const authenticated = await store.authenticate(token, used);
    expect(authenticated).toMatchObject({ id: record.id, address: me, scope: READ_ONLY_SCOPE });

    const [listed] = await store.list(me);
    expect(listed?.lastUsedAt).toBe(used.toISOString());
  });

  it("refuses a token it never issued", async () => {
    const store = await make();
    await store.issue(me, "Hermes");
    expect(await store.authenticate(generateReadOnlyToken(), NOW)).toBe("unknown");
  });

  it("refuses anything that is not shaped like a token, without looking it up", async () => {
    const store = await make();
    const { token } = await store.issue(me, "Hermes");
    for (const wrong of ["", "x", token.toUpperCase(), token.slice(0, -1), `${token}0`, token.replace("lvz_ro_", "lvz_rw_"), `lvz_ro_${"z".repeat(64)}`]) {
      expect(await store.authenticate(wrong, NOW)).toBe("unknown");
    }
    expect(await store.authenticate(token, NOW)).not.toBe("unknown");
  });

  it("refuses a revoked token, and says so", async () => {
    const store = await make();
    const { token, record } = await store.issue(me, "Hermes");
    expect(await store.revoke(me, record.id, NOW)).toBe(true);
    expect(await store.authenticate(token, NOW)).toBe("revoked");
  });

  it("refuses an expired token the moment it expires", async () => {
    const store = await make();
    const expiresAt = new Date("2026-09-10T00:00:00Z");
    const { token } = await store.issue(me, "Hermes", { expiresAt });
    expect(await store.authenticate(token, new Date(expiresAt.getTime() - 1))).not.toBe("expired");
    expect(await store.authenticate(token, expiresAt)).toBe("expired");
    expect(await store.authenticate(token, new Date(expiresAt.getTime() + 1))).toBe("expired");
  });

  it("does not let one identity revoke another's token", async () => {
    const store = await make();
    const { token, record } = await store.issue(me, "Hermes");
    expect(await store.revoke(other, record.id, NOW)).toBe(false);
    expect(await store.authenticate(token, NOW)).not.toBe("revoked");
  });

  it("revoking twice is not a second revocation", async () => {
    const store = await make();
    const { record } = await store.issue(me, "Hermes");
    expect(await store.revoke(me, record.id, NOW)).toBe(true);
    expect(await store.revoke(me, record.id, NOW)).toBe(false);
  });

  it("lists only the caller's own tokens, newest first, revoked ones included", async () => {
    const store = await make();
    const mine = await store.issue(me, "first", { now: new Date("2026-09-01T00:00:00Z") });
    const later = await store.issue(me, "second", { now: new Date("2026-09-02T00:00:00Z") });
    await store.issue(other, "theirs");
    await store.revoke(me, mine.record.id, NOW);

    const listed = await store.list(me);
    expect(listed.map((t) => t.label)).toEqual(["second", "first"]);
    expect(listed.find((t) => t.id === mine.record.id)?.revokedAt).toBe(NOW.toISOString());
    expect(listed.find((t) => t.id === later.record.id)?.revokedAt).toBeNull();
  });

  it("never hands the token back in a listing", async () => {
    const store = await make();
    const { token } = await store.issue(me, "Hermes");
    expect(JSON.stringify(await store.list(me))).not.toContain(token);
  });

  it("gives every token its own secret and its own id", async () => {
    const store = await make();
    const issued = await Promise.all([store.issue(me, "a"), store.issue(me, "b"), store.issue(me, "c")]);
    expect(new Set(issued.map((i) => i.token)).size).toBe(3);
    expect(new Set(issued.map((i) => i.record.id)).size).toBe(3);
  });

  it("refuses to issue past the cap, and lets a revoke make room again", async () => {
    const store = await make();
    const issued = [];
    for (let i = 0; i < MAX_ACTIVE_TOKENS; i += 1) issued.push(await store.issue(me, `agent-${i}`));
    await expect(store.issue(me, "one too many")).rejects.toBeInstanceOf(TooManyReadOnlyTokensError);
    await store.revoke(me, issued[0]!.record.id, NOW);
    await expect(store.issue(me, "back under the cap")).resolves.toBeDefined();
  });
});

describe("what a token looks like", () => {
  it("is prefixed so it cannot be confused with a session id, and is 32 random bytes", () => {
    const token = generateReadOnlyToken();
    expect(token).toMatch(/^lvz_ro_[0-9a-f]{64}$/);
    expect(token).toHaveLength(MAX_TOKEN_LENGTH);
    expect(looksLikeReadOnlyToken(token)).toBe(true);
  });

  it("is stored as its SHA-256, the same treatment a session id gets", () => {
    const token = generateReadOnlyToken();
    expect(readOnlyTokenDigest(token)).toBe(createHash("sha256").update(token).digest("hex"));
    expect(readOnlyTokenDigest(token)).not.toBe(token);
  });
});

describe("what the database holds for a token (postgres)", () => {
  it("is a hash, so reading the table gives nothing that authenticates", async () => {
    const store = new PostgresReadOnlyTokenStore(pool);
    await store.ensureSchema();
    const { token } = await store.issue(ME, "Hermes");
    const rows = await pool.query<{ token_digest: string }>("SELECT token_digest FROM api_read_only_tokens");
    const digests = rows.rows.map((row) => row.token_digest);
    expect(digests).not.toContain(token);
    expect(digests).toContain(readOnlyTokenDigest(token));
    // The stored value is not itself accepted as a token.
    expect(await store.authenticate(readOnlyTokenDigest(token), NOW)).toBe("unknown");
  });

  it("survives a restart: a new store on the same database still knows the token", async () => {
    const store = new PostgresReadOnlyTokenStore(pool);
    await store.ensureSchema();
    const { token } = await store.issue(ME, "Hermes");
    expect(await new PostgresReadOnlyTokenStore(pool).authenticate(token, NOW)).toMatchObject({ address: ME });
  });
});

describe("requireReadOnlyToken", () => {
  const store = new MemoryReadOnlyTokenStore(() => NOW);

  it("reads the bearer token and answers with the identity behind it", async () => {
    const { token, record } = await store.issue(ME, "Hermes");
    expect(await requireReadOnlyToken(bearer(token), store, NOW)).toEqual({
      tokenId: record.id,
      address: ME,
      scope: READ_ONLY_SCOPE,
    });
  });

  it("accepts the scheme in any case, since RFC 7235 makes it case-insensitive", async () => {
    const { token } = await store.issue(ME, "Hermes");
    for (const scheme of ["Bearer", "bearer", "BEARER", "BeArEr"]) {
      expect(await requireReadOnlyToken(asRequest({ authorization: `${scheme} ${token}` }), store, NOW)).toMatchObject({ address: ME });
    }
  });

  it("refuses a request with no Authorization header at all", async () => {
    expect(await requireReadOnlyToken(asRequest({}), store, NOW)).toBe("unknown");
  });

  it("refuses a header that is not a bearer token", async () => {
    const { token } = await store.issue(ME, "Hermes");
    for (const header of [token, `Basic ${token}`, `Bearer`, `Bearer `, `Token ${token}`, `Bearer${token}`]) {
      expect(await requireReadOnlyToken(asRequest({ authorization: header }), store, NOW)).toBe("unknown");
    }
  });

  it("refuses an oversized header without looking it up", async () => {
    const huge = new MemoryReadOnlyTokenStore(() => NOW);
    let lookups = 0;
    const counting = {
      ...huge,
      authenticate: async (...args: Parameters<typeof huge.authenticate>) => {
        lookups += 1;
        return huge.authenticate(...args);
      },
    } as unknown as MemoryReadOnlyTokenStore;
    expect(await requireReadOnlyToken(asRequest({ authorization: `Bearer ${"a".repeat(100_000)}` }), counting, NOW)).toBe("unknown");
    expect(lookups).toBe(0);
  });

  // A browser sends its cookie on every request to this origin. That
  // must never be what gets an agent's route answered.
  it("ignores a session cookie entirely", async () => {
    const withCookie = asRequest({ cookie: "sid=a-real-looking-session-id" });
    expect(await requireReadOnlyToken(withCookie, store, NOW)).toBe("unknown");
  });

  it("distinguishes a revoked and an expired token from an unknown one", async () => {
    const revoked = await store.issue(ME, "revoked");
    await store.revoke(ME, revoked.record.id, NOW);
    expect(await requireReadOnlyToken(bearer(revoked.token), store, NOW)).toBe("revoked");

    const expired = await store.issue(ME, "expired", { expiresAt: new Date(NOW.getTime() - 1) });
    expect(await requireReadOnlyToken(bearer(expired.token), store, NOW)).toBe("expired");
  });
});
