import type { Pool } from "pg";

/** Fixed-window rate limiter, checked before any expensive work runs: input
 * that exceeds a frequency limit is rejected before the costly part. */
/** Counts requests per key in a window. `allow` may answer synchronously (in
 * memory) or not (a shared store). */
export interface RateLimit {
  allow(key: string, now: number): boolean | Promise<boolean>;
}

export class RateLimiter implements RateLimit {
  private readonly windows = new Map<
    string,
    { count: number; windowStart: number }
  >();

  constructor(
    private readonly maxPerWindow: number,
    private readonly windowMs: number,
  ) {}

  allow(key: string, now: number): boolean {
    if (this.windows.size >= SWEEP_THRESHOLD) {
      this.sweep(now);
    }
    const entry = this.windows.get(key);
    if (!entry || now - entry.windowStart >= this.windowMs) {
      this.windows.set(key, { count: 1, windowStart: now });
      return true;
    }
    if (entry.count >= this.maxPerWindow) {
      return false;
    }
    entry.count += 1;
    return true;
  }

  /** Forgets windows that have ended, so a stream of new keys cannot grow the table forever. */
  private sweep(now: number): void {
    for (const [key, entry] of this.windows) {
      if (now - entry.windowStart >= this.windowMs) this.windows.delete(key);
    }
  }
}

const SWEEP_THRESHOLD = 10_000;

/** The same limits, shared by every API instance and kept across restarts: a
 * client cannot double its allowance by reaching a second instance, and the
 * operator's gas budget survives a restart. One row per bucket and key, updated
 * atomically. */
export class PostgresRateLimiter implements RateLimit {
  constructor(
    private readonly pool: Pool,
    private readonly bucket: string,
    private readonly maxPerWindow: number,
    private readonly windowMs: number,
  ) {}

  async ensureSchema(): Promise<void> {
    await this.pool.query(`
      CREATE TABLE IF NOT EXISTS api_rate_limits (
        bucket TEXT NOT NULL,
        key TEXT NOT NULL,
        count INTEGER NOT NULL,
        window_start BIGINT NOT NULL,
        PRIMARY KEY (bucket, key)
      )`);
  }

  async allow(key: string, now: number): Promise<boolean> {
    // One statement: start a new window when the last one has ended, otherwise count.
    const result = await this.pool.query<{ count: number }>(
      `INSERT INTO api_rate_limits (bucket, key, count, window_start) VALUES ($1, $2, 1, $3::bigint)
       ON CONFLICT (bucket, key) DO UPDATE SET
         count = CASE WHEN api_rate_limits.window_start <= ($3::bigint - $4::bigint) THEN 1 ELSE api_rate_limits.count + 1 END,
         window_start = CASE WHEN api_rate_limits.window_start <= ($3::bigint - $4::bigint) THEN $3::bigint ELSE api_rate_limits.window_start END
       RETURNING count`,
      [this.bucket, key, now, this.windowMs],
    );
    // Ended windows are only dead weight; clearing a few now and then keeps the table bounded.
    if (Math.random() < 0.01) {
      await this.pool.query(
        "DELETE FROM api_rate_limits WHERE bucket = $1 AND window_start <= $2::bigint - $3::bigint",
        [this.bucket, now, this.windowMs],
      );
    }
    return (result.rows[0]?.count ?? 1) <= this.maxPerWindow;
  }
}
