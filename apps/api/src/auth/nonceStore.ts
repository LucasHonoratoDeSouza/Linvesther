import { randomBytes } from "node:crypto";

export interface NonceStore {
  issue(): string;
  /** Consumes `nonce` if it was issued, is still fresh and was not already
   * used. Returns whether the consume succeeded — `false` means the nonce
   * never existed, expired, or was already used (a replay). */
  consume(nonce: string): boolean;
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
