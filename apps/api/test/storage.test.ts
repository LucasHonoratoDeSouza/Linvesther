import { createHash, randomBytes } from "node:crypto";
import pg from "pg";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import {
  MemoryCredentialStore,
  MemoryWebAuthnCredentialStore,
  PostgresCredentialStore,
  PostgresWebAuthnCredentialStore,
  type CredentialStore,
  type WebAuthnCredentialStore,
} from "../src/auth/credentialStore.js";
import { createPool } from "../src/storage/pool.js";
import { MemorySessionStore, PostgresSessionStore, SESSION_TTL_MS, type SessionStore } from "../src/auth/sessionStore.js";
import { MemoryDisclosureStore, PostgresDisclosureStore, type DisclosureRecord, type DisclosureStore } from "../src/claims/store.js";

// The same behaviour is asserted for the in-memory and the Postgres store, so
// they cannot drift apart. Postgres runs in a schema of its own that is
// dropped afterwards, never touching other tables in the database.
const DATABASE_URL = process.env.DATABASE_URL ?? "postgresql://linvestherzk:linvestherzk-local-dev-only@localhost:5433/linvestherzk";
const schema = `api_test_${randomBytes(6).toString("hex")}`;
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
const QX = "0x2222222222222222222222222222222222222222222222222222222222222222" as const;
const QY = "0x3333333333333333333333333333333333333333333333333333333333333333" as const;

describe.each<[string, (clock: () => Date) => Promise<SessionStore>]>([
  ["memory", async (clock) => new MemorySessionStore(clock)],
  [
    "postgres",
    async (clock) => {
      const store = new PostgresSessionStore(pool, clock);
      await store.ensureSchema();
      return store;
    },
  ],
])("sessions (%s)", (_name, make) => {
  it("finds a session it created, and forgets it once deleted", async () => {
    const store = await make(() => new Date());
    const created = await store.create(ME);
    expect(await store.get(created.id)).toEqual(created);
    await store.delete(created.id);
    expect(await store.get(created.id)).toBeUndefined();
  });

  it("gives every session its own id and CSRF token", async () => {
    const store = await make(() => new Date());
    const [a, b] = [await store.create(ME), await store.create(ME)];
    expect(a.id).not.toBe(b.id);
    expect(a.csrfToken).not.toBe(b.csrfToken);
  });

  it("does not return a session past its expiry", async () => {
    let now = new Date("2026-09-01T00:00:00Z");
    const store = await make(() => now);
    const created = await store.create(ME);
    now = new Date(now.getTime() + SESSION_TTL_MS - 1);
    expect(await store.get(created.id)).toBeDefined();
    now = new Date(now.getTime() + 2);
    expect(await store.get(created.id)).toBeUndefined();
  });

  it("does not find an id it never issued", async () => {
    expect(await (await make(() => new Date())).get("nope")).toBeUndefined();
  });
});

describe.each<[string, () => Promise<CredentialStore>]>([
  ["memory", async () => new MemoryCredentialStore()],
  [
    "postgres",
    async () => {
      const store = new PostgresCredentialStore(pool);
      await store.ensureSchema();
      return store;
    },
  ],
])("identities (%s)", (_name, make) => {
  const key = () => `0x${randomBytes(20).toString("hex")}` as `0x${string}`;

  it("returns a registered identity's key and method", async () => {
    const store = await make();
    const subject = key();
    await store.register(subject, QX, QY, "webauthn");
    expect(await store.get(subject)).toEqual({ subjectKey: subject, method: "webauthn", qx: QX, qy: QY, counter: 0 });
  });

  it("knows nothing about an identity that never registered", async () => {
    expect(await (await make()).get(key())).toBeUndefined();
  });
});

describe("identities keep the first registration (postgres)", () => {
  it("a later request cannot swap the key or method behind an identity", async () => {
    const store = new PostgresCredentialStore(pool);
    await store.ensureSchema();
    const subject = `0x${randomBytes(20).toString("hex")}` as `0x${string}`;
    await store.register(subject, QX, QY, "vault");
    await store.register(subject, QY, QX, "webauthn");
    expect(await store.get(subject)).toMatchObject({ method: "vault", qx: QX, qy: QY });
  });
});

describe.each<[string, () => Promise<WebAuthnCredentialStore>]>([
  ["memory", async () => new MemoryWebAuthnCredentialStore()],
  [
    "postgres",
    async () => {
      const store = new PostgresWebAuthnCredentialStore(pool);
      await store.ensureSchema();
      return store;
    },
  ],
])("passkeys (%s)", (_name, make) => {
  const id = () => randomBytes(16).toString("base64url");
  const cose = () => {
    const bytes = new Uint8Array(new ArrayBuffer(77));
    bytes.set(randomBytes(77));
    return bytes;
  };

  it("returns the exact public key bytes and counter it was given", async () => {
    const store = await make();
    const [credentialId, publicKeyCose] = [id(), cose()];
    await store.save(credentialId, { publicKeyCose, counter: 3 });
    const stored = await store.get(credentialId);
    expect(stored?.counter).toBe(3);
    expect(Array.from(stored!.publicKeyCose)).toEqual(Array.from(publicKeyCose));
  });

  it("advances the signature counter", async () => {
    const store = await make();
    const credentialId = id();
    await store.save(credentialId, { publicKeyCose: cose(), counter: 1 });
    await store.updateCounter(credentialId, 9);
    expect((await store.get(credentialId))?.counter).toBe(9);
  });

  it("knows nothing about a credential it never saw", async () => {
    expect(await (await make()).get(id())).toBeUndefined();
  });
});

const record = (owner: `0x${string}`): DisclosureRecord => ({
  claimSet: {
    identityId: owner,
    trackId: "all",
    checkpointId: "cp",
    periodStart: "2026-09-01T00:00:00.000Z",
    periodEnd: "2026-09-10T00:00:00.000Z",
    claims: [{ type: "GE", metric: "return", threshold: "0.1" }],
    audience: "public",
    nonce: "n",
    expiresAt: "2026-10-01T00:00:00.000Z",
  },
  credential: { method: "vault", signature: "0xabc" },
  owner,
  publishedAt: "2026-09-10T00:00:00.000Z",
});

describe.each<[string, () => Promise<DisclosureStore>]>([
  ["memory", async () => new MemoryDisclosureStore()],
  [
    "postgres",
    async () => {
      const store = new PostgresDisclosureStore(pool);
      await store.ensureSchema();
      return store;
    },
  ],
])("published claims (%s)", (_name, make) => {
  const digest = () => randomBytes(32).toString("hex");

  it("reads back exactly what was saved", async () => {
    const store = await make();
    const [d, saved] = [digest(), record("0xAbCdEf1111111111111111111111111111111111")];
    await store.save(d, saved);
    expect(await store.get(d)).toEqual(saved);
  });

  it("lists an owner's claims whatever the case of their address, and no one else's", async () => {
    const store = await make();
    const owner = `0x${randomBytes(20).toString("hex")}` as `0x${string}`;
    const other = `0x${randomBytes(20).toString("hex")}` as `0x${string}`;
    const mine = digest();
    await store.save(mine, record(owner));
    await store.save(digest(), record(other));
    const listed = await store.listByOwner(owner.toUpperCase().replace("0X", "0x"));
    expect(listed.map((entry) => entry.digest)).toEqual([mine]);
  });

  it("finds nothing for a digest that was never published", async () => {
    expect(await (await make()).get(digest())).toBeUndefined();
  });
});

describe("state survives a restart (postgres)", () => {
  it("a new store on the same database sees sessions, identities, passkeys and claims", async () => {
    const [sessions, identities, passkeys, claims] = [new PostgresSessionStore(pool), new PostgresCredentialStore(pool), new PostgresWebAuthnCredentialStore(pool), new PostgresDisclosureStore(pool)];
    await Promise.all([sessions.ensureSchema(), identities.ensureSchema(), passkeys.ensureSchema(), claims.ensureSchema()]);

    const session = await sessions.create(ME);
    const subject = `0x${randomBytes(20).toString("hex")}` as `0x${string}`;
    await identities.register(subject, QX, QY, "webauthn");
    await passkeys.save("cred-restart", { publicKeyCose: new Uint8Array(new ArrayBuffer(4)), counter: 2 });
    await claims.save("digest-restart", record(ME));

    const [sessions2, identities2, passkeys2, claims2] = [new PostgresSessionStore(pool), new PostgresCredentialStore(pool), new PostgresWebAuthnCredentialStore(pool), new PostgresDisclosureStore(pool)];
    expect(await sessions2.get(session.id)).toEqual(session);
    expect((await identities2.get(subject))?.method).toBe("webauthn");
    expect((await passkeys2.get("cred-restart"))?.counter).toBe(2);
    expect(await claims2.get("digest-restart")).toBeDefined();
  });
});

describe("what the database holds for a session (postgres)", () => {
  it("is a hash, so reading the table gives nothing that works as a cookie", async () => {
    const store = new PostgresSessionStore(pool);
    await store.ensureSchema();
    const session = await store.create(ME);
    const rows = await pool.query<{ id: string }>("SELECT id FROM api_sessions");
    const ids = rows.rows.map((row) => row.id);
    expect(ids).not.toContain(session.id);
    expect(ids).toContain(createHash("sha256").update(session.id).digest("hex"));
    // The stored value is not itself accepted as a session id.
    expect(await store.get(createHash("sha256").update(session.id).digest("hex"))).toBeUndefined();
    expect(await store.get(session.id)).toBeDefined();
  });
});

describe("the database going away", () => {
  it("does not take the process down: the pool logs it and reconnects", async () => {
    const survivor = createPool(DATABASE_URL, { max: 2 });
    const errors: unknown[] = [];
    const seen = (error: unknown) => errors.push(error);
    process.on("uncaughtException", seen);
    try {
      await survivor.query("SELECT 1"); // leaves one idle connection open
      const backend = (await survivor.query<{ pid: number }>("SELECT pg_backend_pid() AS pid")).rows[0]!.pid;
      await admin.query("SELECT pg_terminate_backend($1)", [backend]);
      await new Promise((resolve) => setTimeout(resolve, 300));
      expect(errors).toEqual([]);
      expect((await survivor.query("SELECT 1 AS ok")).rows[0]).toEqual({ ok: 1 });
    } finally {
      process.off("uncaughtException", seen);
      await survivor.end();
    }
  });
});
