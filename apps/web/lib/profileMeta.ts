import type { OnChainProfile, PublicTrack } from "./api";

/** The address, shortened for display when there's no name to show instead. */
export function shortAddress(address: string): string {
  return `${address.slice(0, 6)}…${address.slice(-4)}`;
}

export function profileTitle(address: string, profile: OnChainProfile | null): string {
  return `${profile?.name || shortAddress(address)} — Public track record`;
}

/** One line summarizing the combined track record, for a meta description or an OG card. */
export function profileSummary(tracks: PublicTrack[], bio?: string): string {
  if (bio) return bio;
  const lines = tracks
    .map(({ statement }) => {
      const { return: ret, maxDrawdown } = statement.metrics;
      if (ret === undefined) return null;
      const signed = `${Number(ret) >= 0 ? "+" : ""}${(Number(ret) * 100).toFixed(1)}%`;
      return maxDrawdown === undefined
        ? `${signed} return`
        : `${signed} return, ${(Number(maxDrawdown) * 100).toFixed(1)}% max drawdown`;
    })
    .filter((line): line is string => line !== null);
  return lines.length > 0
    ? lines.join(" · ")
    : "A verifiable, privacy-preserving investment track record on Linvesther.";
}
