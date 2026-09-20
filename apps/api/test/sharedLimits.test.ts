import { randomBytes } from "node:crypto";
import pg from "pg";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { MemoryNonceStore, NONCE_TTL_MS, PostgresNonceStore, type NonceStore } from "../src/auth/nonceStore.js";
import { PostgresRateLimiter, RateLimiter, type RateLimit } from "../src/auth/rateLimiter.js";

// The same behaviour for the in-memory and the shared (Postgres) implementations,
// plus what only a shared store can promise: two instances acting as one.
const DATABASE_URL = process.env.DATABASE_URL ?? "postgresql://linvestherzk:linvestherzk-local-dev-only@localhost:5433/postgres";
const schema = `api_shared_${randomBytes(6).toString("hex")}`;
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

const bucket = () => `b${randomBytes(4).toString("hex")}`;

describe.each<[string, (max: number, windowMs: number) => Promise<RateLimit>]>([
  ["memory", async (max, windowMs) => new RateLimiter(max, windowMs)],
  [
    "postgres",
    async (max, windowMs) => {
      const limiter = new PostgresRateLimiter(pool, bucket(), max, windowMs);
      await limiter.ensureSchema();
      return limiter;
    },
  ],
])("rate limits (%s)", (_name, make) => {
  it("lets through up to the limit in a window, then refuses", async () => {
    const limiter = await make(3, 1_000);
    const results = [];
    for (let i = 0; i < 5; i++) results.push(await limiter.allow("client", 10_000));
    expect(results).toEqual([true, true, true, false, false]);
  });

  it("counts each client on its own", async () => {
    const limiter = await make(1, 1_000);
    expect(await limiter.allow("a", 0)).toBe(true);
    expect(await limiter.allow("b", 0)).toBe(true);
    expect(await limiter.allow("a", 1)).toBe(false);
  });

  it("starts a new window once the last one has ended", async () => {
    const limiter = await make(1, 1_000);
    expect(await limiter.allow("client", 0)).toBe(true);
    expect(await limiter.allow("client", 999)).toBe(false);
    expect(await limiter.allow("client", 1_000)).toBe(true);
  });
});

describe("rate limits shared between instances (postgres)", () => {
  it("count together: a second instance cannot double the allowance", async () => {
    const name = bucket();
    const [first, second] = [new PostgresRateLimiter(pool, name, 2, 60_000), new PostgresRateLimiter(pool, name, 2, 60_000)];
    await first.ensureSchema();
    expect(await first.allow("client", 5)).toBe(true);
    expect(await second.allow("client", 6)).toBe(true);
    expect(await first.allow("client", 7)).toBe(false);
  });

  it("stay counted across a restart", async () => {
    const name = bucket();
    const before = new PostgresRateLimiter(pool, name, 1, 60_000);
    await before.ensureSchema();
    expect(await before.allow("client", 5)).toBe(true);
    const afterRestart = new PostgresRateLimiter(pool, name, 1, 60_000);
    expect(await afterRestart.allow("client", 6)).toBe(false);
  });

  it("stay exact when many requests arrive at once", async () => {
    const limiter = new PostgresRateLimiter(pool, bucket(), 5, 60_000);
    await limiter.ensureSchema();
    const results = await Promise.all(Array.from({ length: 20 }, () => limiter.allow("client", 100)));
    expect(results.filter(Boolean)).toHaveLength(5);
  });
});

describe.each<[string, (clock: () => number) => Promise<NonceStore>]>([
  ["memory", async (clock) => new MemoryNonceStore(clock)],
  [
    "postgres",
    async (clock) => {
      const store = new PostgresNonceStore(pool, bucket(), clock);
      await store.ensureSchema();
      return store;
    },
  ],
])("sign-in challenges (%s)", (_name, make) => {
  it("are unguessable and never repeat", async () => {
    const store = await make(() => 0);
    const nonces = await Promise.all(Array.from({ length: 50 }, () => store.issue()));
    expect(new Set(nonces).size).toBe(50);
    for (const nonce of nonces) expect(nonce).toMatch(/^[A-Za-z0-9_-]{43}$/);
  });

  it("can be used once, and only if they were issued", async () => {
    const store = await make(() => 0);
    const nonce = await store.issue();
    expect(await store.consume("never-issued")).toBe(false);
    expect(await store.consume(nonce)).toBe(true);
    expect(await store.consume(nonce)).toBe(false);
  });

  it("stop working after a few minutes", async () => {
    let now = 1_000;
    const store = await make(() => now);
    const nonce = await store.issue();
    now += NONCE_TTL_MS + 1;
    expect(await store.consume(nonce)).toBe(false);
  });
});

describe("sign-in challenges shared between instances (postgres)", () => {
  it("issued by one instance, answered at another, once", async () => {
    const purpose = bucket();
    const [a, b] = [new PostgresNonceStore(pool, purpose), new PostgresNonceStore(pool, purpose)];
    await a.ensureSchema();
    const nonce = await a.issue();
    expect(await b.consume(nonce)).toBe(true);
    expect(await a.consume(nonce)).toBe(false);
  });

  it("are not interchangeable between purposes", async () => {
    const [registration, authentication] = [new PostgresNonceStore(pool, bucket()), new PostgresNonceStore(pool, bucket())];
    await registration.ensureSchema();
    const nonce = await registration.issue();
    expect(await authentication.consume(nonce)).toBe(false);
    expect(await registration.consume(nonce)).toBe(true);
  });

  it("survive a restart", async () => {
    const purpose = bucket();
    const before = new PostgresNonceStore(pool, purpose);
    await before.ensureSchema();
    const nonce = await before.issue();
    expect(await new PostgresNonceStore(pool, purpose).consume(nonce)).toBe(true);
  });

  it("are consumed exactly once even when two instances answer at the same moment", async () => {
    const purpose = bucket();
    const store = new PostgresNonceStore(pool, purpose);
    await store.ensureSchema();
    const nonce = await store.issue();
    const answers = await Promise.all(Array.from({ length: 8 }, () => new PostgresNonceStore(pool, purpose).consume(nonce)));
    expect(answers.filter(Boolean)).toHaveLength(1);
  });
});

describe("setting up the challenge table", () => {
  it("survives several stores doing it at the same moment, which is how the API starts", async () => {
    // A fresh schema each time: the race only exists while the table does not yet exist.
    for (let round = 0; round < 5; round++) {
      const fresh = `api_race_${randomBytes(6).toString("hex")}`;
      await admin.query(`CREATE SCHEMA ${fresh}`);
      const racing = new pg.Pool({ connectionString: DATABASE_URL, options: `-c search_path=${fresh}` });
      try {
        const stores = ["vault", "webauthn-registration", "webauthn-authentication"].map((purpose) => new PostgresNonceStore(racing, purpose));
        await expect(Promise.all(stores.map((store) => store.ensureSchema()))).resolves.toBeDefined();
      } finally {
        await racing.end();
        await admin.query(`DROP SCHEMA ${fresh} CASCADE`);
      }
    }
  });
});
