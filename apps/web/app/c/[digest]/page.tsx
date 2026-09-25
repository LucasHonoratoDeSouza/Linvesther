import type { Metadata } from "next";
import { fetchPublicClaim } from "../../../lib/api";
import { describeClaim } from "../../../lib/claimFormat";
import { shortAddress } from "../../../lib/profileMeta";
import { ClaimPageClient } from "./ClaimPageClient";

export async function generateMetadata({
  params,
}: {
  params: Promise<{ digest: string }>;
}): Promise<Metadata> {
  const { digest } = await params;
  const result = await fetchPublicClaim(digest);
  if (!result.ok) {
    return { title: result.status === 410 ? "This claim has expired" : "This claim doesn’t exist" };
  }

  const { claimSet, owner } = result.data;
  const title = claimSet.claims.map(describeClaim).join(" · ");
  const description = `Claimed by ${shortAddress(owner)} about their combined track record, counted since ${new Date(claimSet.periodStart).toLocaleDateString()}. It was true when signed — the figure behind it stays private.`;
  return {
    title,
    description,
    openGraph: { title, description },
    twitter: { card: "summary_large_image", title, description },
  };
}

export default function Page() {
  return <ClaimPageClient />;
}
