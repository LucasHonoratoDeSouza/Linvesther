/** Fixed-window rate limiter, checked before any expensive work runs: input
 * that exceeds a frequency limit is rejected before the costly part. */
export class RateLimiter {
  private readonly windows = new Map<string, { count: number; windowStart: number }>();

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
