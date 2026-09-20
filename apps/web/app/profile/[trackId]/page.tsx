"use client";

import { useParams } from "next/navigation";
import { useEffect, useState } from "react";
import { AppShell } from "../../../components/AppShell";
import Link from "next/link";
import { Icon } from "../../../components/Icon";
import { fetchPublicTrack, type TrackProjection } from "../../../lib/api";

const A0_LIMITATION_NOTICE =
  "Origin: A0 — signed by an identified collector. This is not the exchange's own signature and is not resistance to a malicious collector.";

export default function ProfilePage() {
  const params = useParams<{ trackId: string }>();
  const trackId = params.trackId;
  const [projection, setProjection] = useState<TrackProjection | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setProjection(null);
    setError(null);
    fetchPublicTrack(trackId).then((result) => {
      if (cancelled) return;
      if (!result.ok) {
        setError(`Could not load this profile: ${result.error}`);
        return;
      }
      setProjection(result.data);
    });
    return () => {
      cancelled = true;
    };
  }, [trackId]);

  if (error) {
    return (
      <AppShell>
        <main className="shell">
          <div className="hero">
            <div className="hero-label">Public explorer</div>
            <h1 className="hero-title">Track unavailable.</h1>
            <p className="hero-subtitle">Check the Track ID and try again.</p>
          </div>
          <div className="error-banner" role="alert" data-testid="error-banner">
            {error}
          </div>
          <Link className="button button-outline" href="/explorer">
            Back to explorer <Icon name="arrow" width={14} />
          </Link>
        </main>
      </AppShell>
    );
  }

  if (!projection) {
    return (
      <AppShell>
        <main className="shell">
          <p className="chart-empty" role="status" data-testid="loading">
            Loading the public record…
          </p>
        </main>
      </AppShell>
    );
  }

  return (
    <AppShell>
      <main className="shell">
        <div className="hero" style={{ paddingTop: 0 }}>
          <div className="hero-label">Public track</div>
          <h1
            className="hero-title"
            data-testid="track-id"
            style={{ fontSize: 24, wordBreak: "break-all" }}
          >
            {projection.profile.trackId}
          </h1>
        </div>

        {projection.dimensions.origin === "A0" && (
          <div
            className="error-banner"
            style={{ background: "var(--accent-soft)", color: "var(--accent)" }}
            data-testid="a0-notice"
          >
            {A0_LIMITATION_NOTICE}
          </div>
        )}

        <div className="card">
          <div className="card-title-row">
            <div className="card-title">Profile</div>
          </div>
          <dl className="kv-list" style={{ margin: 0 }}>
            <div className="kv-row">
              <dt>Denomination</dt>
              <dd data-testid="currency">{projection.profile.currency}</dd>
            </div>
            <div className="kv-row">
              <dt>Verified since</dt>
              <dd data-testid="verified-since">
                {projection.profile.verifiedSince ?? "not yet verified"}
              </dd>
            </div>
          </dl>
        </div>

        <div className="card">
          <div className="card-title-row">
            <div className="card-title">Dimensions</div>
          </div>
          <dl
            className="kv-list"
            data-testid="dimensions"
            style={{ margin: 0 }}
          >
            <div className="kv-row">
              <dt>Origin</dt>
              <dd data-testid="dimension-origin">
                {projection.dimensions.origin}
              </dd>
            </div>
            <div className="kv-row">
              <dt>Coverage</dt>
              <dd data-testid="dimension-coverage">
                {projection.dimensions.coverage}
              </dd>
            </div>
            <div className="kv-row">
              <dt>Calculation</dt>
              <dd data-testid="dimension-calculation">
                {projection.dimensions.calculation}
              </dd>
            </div>
            <div className="kv-row">
              <dt>Registry</dt>
              <dd data-testid="dimension-registry">
                {projection.dimensions.registry}
              </dd>
            </div>
            <div className="kv-row">
              <dt>Availability</dt>
              <dd data-testid="dimension-availability">
                {projection.dimensions.availability}
              </dd>
            </div>
          </dl>
        </div>

        <div className="card">
          <div className="card-title-row">
            <div className="card-title">Gaps</div>
          </div>
          <div data-testid="gaps">
            {projection.gaps.length === 0 ? (
              <p
                style={{
                  fontSize: 13,
                  color: "var(--text-tertiary)",
                  margin: 0,
                }}
              >
                None
              </p>
            ) : (
              <ul style={{ margin: 0, paddingLeft: 18, fontSize: 13 }}>
                {projection.gaps.map((gap, i) => (
                  <li key={i}>{`${gap.startMs}–${gap.endMs}`}</li>
                ))}
              </ul>
            )}
          </div>
        </div>

        <div className="card">
          <div className="card-title-row">
            <div className="card-title">Corrections</div>
          </div>
          <div data-testid="corrections">
            {projection.corrections.length === 0 ? (
              <p
                style={{
                  fontSize: 13,
                  color: "var(--text-tertiary)",
                  margin: 0,
                }}
              >
                None
              </p>
            ) : (
              <ul style={{ margin: 0, paddingLeft: 18, fontSize: 13 }}>
                {projection.corrections.map((correction) => (
                  <li key={correction.targetDigest}>{correction.reasonCode}</li>
                ))}
              </ul>
            )}
          </div>
        </div>
      </main>
    </AppShell>
  );
}
