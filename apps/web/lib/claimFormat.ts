import type { ClaimSet } from "./api";

export const CLAIM_METRICS = {
  return: { label: "Return", type: "GE" as const, phrase: "at least" },
  maxDrawdown: {
    label: "Max drawdown",
    type: "LE" as const,
    phrase: "at most",
  },
};

export type ClaimMetricKey = keyof typeof CLAIM_METRICS;

/** "Return at least 10%" — the claim in words, without the exact figure behind it. */
export function describeClaim(claim: ClaimSet["claims"][number]): string {
  const meta = CLAIM_METRICS[claim.metric as ClaimMetricKey];
  const percent = `${(Number(claim.threshold) * 100).toFixed(2).replace(/\.?0+$/, "")}%`;
  return meta
    ? `${meta.label} ${meta.phrase} ${percent}`
    : `${claim.metric} ${claim.type} ${percent}`;
}
