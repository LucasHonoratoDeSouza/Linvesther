import type { Metadata } from "next";
import { fetchPublicProfile } from "../../../lib/api";
import { profileSummary, profileTitle, shortAddress } from "../../../lib/profileMeta";
import { ProfilePageClient } from "./ProfilePageClient";

export async function generateMetadata({
  params,
}: {
  params: Promise<{ address: string }>;
}): Promise<Metadata> {
  const { address } = await params;
  const result = await fetchPublicProfile(address);
  if (!result.ok) return { title: `${shortAddress(address)} — Public track record` };

  const { profile, tracks, privacyMode } = result.data;
  const title = profileTitle(address, profile);
  const description = privacyMode
    ? "This owner has chosen full privacy mode and shares nothing here."
    : profileSummary(tracks, profile?.bio);
  return {
    title,
    description,
    openGraph: { title, description },
    twitter: { card: "summary_large_image", title, description },
  };
}

export default function Page() {
  return <ProfilePageClient />;
}
