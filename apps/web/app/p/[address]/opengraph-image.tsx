import { ImageResponse } from "next/og";
import { fetchPublicProfile } from "../../../lib/api";
import { shortAddress } from "../../../lib/profileMeta";

export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

const bg = "#0b0d0c";
const fg = "#f4f6f4";
const muted = "#8a978f";
const up = "#5fd08a";

export default async function Image({ params }: { params: Promise<{ address: string }> }) {
  const { address } = await params;
  const result = await fetchPublicProfile(address);
  const name = result.ok ? result.data.profile?.name : null;
  const track = result.ok && !result.data.privacyMode ? result.data.tracks[0] : undefined;
  const ret = track?.statement.metrics.return;
  const drawdown = track?.statement.metrics.maxDrawdown;
  const caption =
    result.ok && result.data.privacyMode
      ? "Full privacy mode"
      : "Verifiable, privacy-preserving track record";

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
        <div style={{ display: "flex", fontSize: 30, color: muted }}>Linvesther · Public track record</div>
        <div style={{ display: "flex", flexDirection: "column" }}>
          <div style={{ display: "flex", fontSize: 64, fontWeight: 700 }}>{name || shortAddress(address)}</div>
          {ret !== undefined ? (
            <div style={{ display: "flex", gap: 64, marginTop: 40 }}>
              <div style={{ display: "flex", flexDirection: "column" }}>
                <div style={{ display: "flex", fontSize: 26, color: muted }}>Return</div>
                <div style={{ display: "flex", fontSize: 56, fontWeight: 700, color: up }}>
                  {`${Number(ret) >= 0 ? "+" : ""}${(Number(ret) * 100).toFixed(1)}%`}
                </div>
              </div>
              {drawdown !== undefined && (
                <div style={{ display: "flex", flexDirection: "column" }}>
                  <div style={{ display: "flex", fontSize: 26, color: muted }}>Max drawdown</div>
                  <div style={{ display: "flex", fontSize: 56, fontWeight: 700 }}>
                    {`${(Number(drawdown) * 100).toFixed(1)}%`}
                  </div>
                </div>
              )}
            </div>
          ) : (
            <div style={{ display: "flex", fontSize: 30, color: muted, marginTop: 24 }}>{caption}</div>
          )}
        </div>
      </div>
    ),
    size,
  );
}
