"use client";

import { useParams } from "next/navigation";
import { useEffect, useState } from "react";
import { AppShell } from "../../../components/AppShell";
import {
  fetchPublicProfile,
  type CollectorOrigin,
  type OnChainProfile,
  type PublicTrack,
} from "../../../lib/api";
import { OriginBadge } from "../../../components/OriginBadge";
import styles from "../profile.module.css";
import { TrackCard } from "../TrackCard";

export function ProfilePageClient() {
  const { address } = useParams<{ address: string }>();
  const [state, setState] = useState<
    | { status: "loading" }
    | { status: "missing" }
    | { status: "error"; message: string }
    | { status: "private" }
    | { status: "ready"; tracks: PublicTrack[] }
  >({ status: "loading" });
  const [profile, setProfile] = useState<OnChainProfile | null>(null);
  const [origin, setOrigin] = useState<CollectorOrigin | undefined>(undefined);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    fetchPublicProfile(address).then((result) => {
      if (result.ok) setProfile(result.data.profile);
      if (result.ok) setOrigin(result.data.origin);
      if (result.ok)
        setState(
          result.data.privacyMode
            ? { status: "private" }
            : { status: "ready", tracks: result.data.tracks },
        );
      else if (result.status === 404) setState({ status: "missing" });
      else setState({ status: "error", message: result.error });
    });
  }, [address]);

  const short = `${address.slice(0, 6)}…${address.slice(-4)}`;

  return (
    <AppShell>
      <main className="shell">
        <div className={styles.profileHead}>
          <div className={styles.avatar}>
            {address.slice(2, 4).toUpperCase()}
          </div>
          <div>
            <h1 className={styles.profileTitle} data-testid="profile-title">
              {profile?.name || "Public track record"}
            </h1>
            <div className={styles.address}>
              <span data-testid="profile-address" title={address}>
                {short}
              </span>
              <button
                type="button"
                className={styles.copy}
                onClick={() => {
                  navigator.clipboard?.writeText(window.location.href);
                  setCopied(true);
                  setTimeout(() => setCopied(false), 1500);
                }}
              >
                {copied ? "Link copied" : "Copy link"}
              </button>
            </div>
          </div>
        </div>

        {profile?.bio && (
          <p
            className={styles.explain}
            data-testid="profile-bio"
            style={{ marginTop: 0 }}
          >
            {profile.bio}
            <br />
            <small>
              Written by the owner of this address on a public blockchain — not
              verified by Linvesther. Last changed{" "}
              {new Date(profile.updatedAt * 1000).toLocaleString()}.
            </small>
          </p>
        )}

        {state.status === "loading" && <p className="note">Loading…</p>}

        {state.status === "missing" && (
          <div className={styles.explain} data-testid="profile-missing">
            <strong>Nothing to show here.</strong> No account is connected at
            this address.
          </div>
        )}

        {state.status === "private" && (
          <div className={styles.explain} data-testid="profile-private">
            <strong>Full privacy mode.</strong> The owner of this address has
            chosen to show nothing at all.
          </div>
        )}

        {state.status === "error" && (
          <div className="error-banner" role="alert">
            Could not load this profile: {state.message}
          </div>
        )}

        {state.status === "ready" && (
          <p style={{ margin: "0 0 16px" }}>
            <OriginBadge origin={origin} />
          </p>
        )}

        {state.status === "ready" &&
          state.tracks.map((track) => (
            <TrackCard
              key={track.statement.accountId}
              track={track}
              chart={{ address }}
            />
          ))}

        <div className={styles.explain}>
          <strong>How to read this.</strong> The owner connected an account with
          a read-only key — Linvesther can look but never trade or move money.
          All connected accounts are added together at this address, as
          percentages only: never balances, positions or trades — and the owner
          can’t pick which ones to show. Figures count from the moment of
          connection, so a good stretch can’t be picked out after the fact.{" "}
          <a href="/docs/concepts#verification" className="text-link">
            How verification works
          </a>
        </div>
      </main>
    </AppShell>
  );
}
