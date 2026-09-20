export const SHAREABLE_METRICS = ["return", "maxDrawdown", "sharpe", "winRate"] as const;
export type ShareableMetric = (typeof SHAREABLE_METRICS)[number];

/** The one choice the owner has: full privacy mode. Otherwise every
 * connected account is public at the owner's address, all of it added
 * together and with every number shown — no picking the flattering ones. */
export interface ProfileSettings {
  privacyMode: boolean;
}

/** What anyone can read about an account: only ratios about its track
 * record — never a balance, a position, a trade, an exchange identifier
 * or any absolute amount. */
export interface ProfileStatement {
  address: `0x${string}`;
  accountId: string;
  trackName: string;
  /** ISO time the exchange account was connected — everything below is measured from then. */
  since: string;
  trackedDays: number;
  /** Where the numbers come from: read straight from the exchange with a
   * read-only key by Linvesther's collector — not (yet) a zero-knowledge proof. */
  verification: "collector_attested_read_only";
  metrics: {
    /** Compounded return since `since`, as a decimal fraction ("0.0421" = 4.21%). */
    return?: string;
    /** Biggest fall from a peak since `since`, as a fraction. */
    maxDrawdown?: string;
    sharpe?: string;
    winRate?: { value: string; closedTrades: number };
  };
  /** ISO time these numbers were computed. */
  computedAt: string;
}

/** Where the figures come from. Every profile and claim is read from the
 * exchange by a collector that signs what it saw; `collector` is the
 * fingerprint of that collector's key, or `null` when the instance has no
 * signing key configured. Compare it against a list of collectors you
 * trust: a key that is not on your list is self-attested. */
export interface CollectorOrigin {
  mechanism: "A0";
  collector: string | null;
}
