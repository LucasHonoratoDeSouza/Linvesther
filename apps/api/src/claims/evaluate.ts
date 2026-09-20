import type { ClaimPredicate } from "./types.js";

/** The owner's current combined figures, as decimal fractions ("0.042" = 4.2%). */
export type ClaimMetrics = Record<string, number | undefined>;

export type ClaimCheck = { ok: true } | { ok: false; error: "claim_not_satisfied" | "metric_unavailable" | "unsupported_claim" };

const compare = (type: string, value: number, threshold: number): boolean | null => {
  switch (type) {
    case "GE":
      return value >= threshold;
    case "GT":
      return value > threshold;
    case "LE":
      return value <= threshold;
    case "LT":
      return value < threshold;
    default:
      return null;
  }
};

/** A claim is only worth signing if it is true of the record right now:
 * every predicate must hold against the owner's combined metrics. */
export function checkClaims(claims: ClaimPredicate[], metrics: ClaimMetrics): ClaimCheck {
  if (claims.length === 0) return { ok: false, error: "unsupported_claim" };
  for (const claim of claims) {
    if (claim.type === "RANGE" || claim.type === "VALUE") return { ok: false, error: "unsupported_claim" };
    const value = metrics[claim.metric];
    if (value === undefined || !Number.isFinite(value)) return { ok: false, error: "metric_unavailable" };
    const threshold = Number(claim.threshold);
    if (!Number.isFinite(threshold)) return { ok: false, error: "unsupported_claim" };
    if (!compare(claim.type, value, threshold)) return { ok: false, error: "claim_not_satisfied" };
  }
  return { ok: true };
}
