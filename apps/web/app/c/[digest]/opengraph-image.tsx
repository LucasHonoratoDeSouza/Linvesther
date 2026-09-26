import { ImageResponse } from "next/og";
import { fetchPublicClaim } from "../../../lib/api";
import { describeClaim } from "../../../lib/claimFormat";
import { shortAddress } from "../../../lib/profileMeta";

export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

const bg = "#0b0d0c";
const fg = "#f4f6f4";
const muted = "#8a978f";

export default async function Image({ params }: { params: Promise<{ digest: string }> }) {
  const { digest } = await params;
  const result = await fetchPublicClaim(digest);
  const title = result.ok
    ? result.data.claimSet.claims.map(describeClaim).join(" · ")
    : result.status === 410
      ? "This claim has expired"
      : "This claim doesn’t exist";

  return new ImageResponse(
    (
      <div
        style={{
          width: "100%",
          height: "100%",
          display: "flex",
          flexDirection: "column",
          justifyContent: "space-between",
          padding: 80,
          background: bg,
          color: fg,
          fontFamily: "sans-serif",
        }}
      >
        <div style={{ display: "flex", fontSize: 30, color: muted }}>Linvesther · Public claim</div>
        <div style={{ display: "flex", flexDirection: "column" }}>
          <div style={{ display: "flex", fontSize: 56, fontWeight: 700, lineHeight: 1.2 }}>{title}</div>
          {result.ok && (
            <div style={{ display: "flex", fontSize: 30, color: muted, marginTop: 32 }}>
              {`Claimed by ${shortAddress(result.data.owner)}`}
            </div>
          )}
        </div>
      </div>
    ),
    size,
  );
}
