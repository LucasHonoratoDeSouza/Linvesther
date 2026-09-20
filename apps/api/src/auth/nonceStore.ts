import { randomBytes } from "node:crypto";
import type { Pool } from "pg";

export interface NonceStore {
  issue(): string | Promise<string>;
  /** Consumes `nonce` if it was issued, is still fresh and was not already
   * used. Returns whether the consume succeeded — `false` means the nonce
   * never existed, expired, or was already used (a replay). */
  consume(nonce: string): boolean | Promise<boolean>;
}

/** A sign-in challenge is only worth answering for a few minutes. */
export const NONCE_TTL_MS = 5 * 60 * 1000;
/** Upper bound on challenges waiting to be answered, so asking for them
 * cannot grow memory without limit. */
export const NONCE_CAPACITY = 10_000;

/** In-memory nonce store: every `issue()` returns an unguessable value that
 * can be `consume()`d exactly once, within `NONCE_TTL_MS`. */
export class MemoryNonceStore implements NonceStore {
  // Insertion order is issue order, so the oldest challenge is always first.
  private readonly issued = new Map<string, number>();

  constructor(private readonly now: () => number = Date.now) {}

  issue(): string {
    const nonce = randomBytes(32).toString("base64url");
    const now = this.now();
    if (this.issued.size >= NONCE_CAPACITY) {
      this.evictExpired(now);
    }
    // Still full of live challenges: drop the oldest rather than refuse a new person.
    while (this.issued.size >= NONCE_CAPACITY) {
      const oldest = this.issued.keys().next().value;
      if (oldest === undefined) break;
      this.issued.delete(oldest);
    }
    this.issued.set(nonce, now + NONCE_TTL_MS);
    return nonce;
  }

  consume(nonce: string): boolean {
    const expiresAt = this.issued.get(nonce);
    if (expiresAt === undefined) {
      return false;
    }
    this.issued.delete(nonce);
    return expiresAt > this.now();
  }

  private evictExpired(now: number): void {
    for (const [nonce, expiresAt] of this.issued) {
      if (expiresAt <= now) this.issued.delete(nonce);
    }
  }
}

/** Challenges kept in the database, so one issued by an API instance can be
 * answered at another (or after a restart), and still only once. */
export class PostgresNonceStore implements NonceStore {
  constructor(
    private readonly pool: Pool,
    private readonly purpose: string,
    private readonly now: () => number = Date.now,
  ) {}

  async ensureSchema(): Promise<void> {
    // Several stores share this table and are set up at the same moment.
    // Postgres can fail two concurrent CREATE TABLE IF NOT EXISTS on the same
    // name, so the setup takes a lock and runs one at a time.
    const client = await this.pool.connect();
    try {
      await client.query("BEGIN");
      await client.query(
        "SELECT pg_advisory_xact_lock(hashtext('api_nonces_schema'))",
      );
      await client.query(`
        CREATE TABLE IF NOT EXISTS api_nonces (
          nonce TEXT PRIMARY KEY,
          purpose TEXT NOT NULL,
          expires_at BIGINT NOT NULL
        )`);
      await client.query(
        "CREATE INDEX IF NOT EXISTS api_nonces_expiry ON api_nonces (purpose, expires_at)",
      );
      await client.query("COMMIT");
    } catch (error) {
      await client.query("ROLLBACK").catch(() => undefined);
      throw error;
    } finally {
      client.release();
    }
  }

  async issue(): Promise<string> {
    const nonce = randomBytes(32).toString("base64url");
    const now = this.now();
    await this.pool.query(
      "INSERT INTO api_nonces (nonce, purpose, expires_at) VALUES ($1, $2, $3)",
      [nonce, this.purpose, now + NONCE_TTL_MS],
    );
    // Expired challenges are dead weight; and the table has the same bound as
    // the in-memory store, dropping the oldest when asked for too many.
    await this.pool.query(
      "DELETE FROM api_nonces WHERE purpose = $1 AND expires_at <= $2",
      [this.purpose, now],
    );
    await this.pool.query(
      `DELETE FROM api_nonces WHERE purpose = $1 AND nonce IN (
         SELECT nonce FROM api_nonces WHERE purpose = $1 ORDER BY expires_at DESC OFFSET $2)`,
      [this.purpose, NONCE_CAPACITY],
    );
    return nonce;
  }

  async consume(nonce: string): Promise<boolean> {
    // Deleting is what makes it single-use, even with two instances asking at once.
    const result = await this.pool.query<{ expires_at: string }>(
      "DELETE FROM api_nonces WHERE nonce = $1 AND purpose = $2 RETURNING expires_at",
      [nonce, this.purpose],
    );
    const row = result.rows[0];
    return row !== undefined && Number(row.expires_at) > this.now();
  }
}
