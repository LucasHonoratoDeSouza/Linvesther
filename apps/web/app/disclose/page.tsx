"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { AppShell } from "../../components/AppShell";
import { checkSession, fetchProfileSettings, fetchPublicProfile, updateProfileSettings, type PublicTrack } from "../../lib/api";
import { TrackCard } from "../p/TrackCard";
import { PublicLinkCard } from "../portfolio/PublicLinkCard";
import styles from "../portfolio/portfolio.module.css";

/** The public profile: every connected account added together, percentages
 * only, with every number shown — nothing to pick or hide, so a track record
 * can't be curated. The one choice is full privacy mode. */
export default function PublicProfilePage() {
  const [signedIn, setSignedIn] = useState<boolean | null>(null);
  const [address, setAddress] = useState<`0x${string}` | null>(null);
  const [privacyMode, setPrivacyMode] = useState(false);
  const [visible, setVisible] = useState<PublicTrack[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function loadPublicView(forAddress: string) {
    const result = await fetchPublicProfile(forAddress);
    setVisible(result.ok ? result.data.tracks : []);
  }

  useEffect(() => {
    checkSession().then(async (session) => {
      setSignedIn(session.ok);
      if (!session.ok) return;
      setAddress(session.data.address);
      const settings = await fetchProfileSettings();
      if (!settings.ok) return setError(`Could not load your settings: ${settings.error}`);
      setPrivacyMode(settings.data.privacyMode);
      await loadPublicView(session.data.address);
    });
  }, []);

  async function toggle() {
    if (!address) return;
    setError(null);
    const result = await updateProfileSettings({ privacyMode: !privacyMode });
    if (!result.ok) return setError(result.error);
    setPrivacyMode(result.data.privacyMode);
    await loadPublicView(address);
  }

  return (
    <AppShell>
      <main className="shell">
        <div className={styles.hero}>
          <div className={styles.heroLabel}>Public profile</div>
          <h1 className={styles.heroTitle}>Your track record is public — as percentages only.</h1>
          <p className={styles.heroSubtitle}>
            All the accounts you connect are added together at your public address, counted from when you connected
            them. Balances, positions and trades are never shown, and you can’t pick which accounts or numbers to show —
            so the record can’t be cherry-picked.
          </p>
        </div>

        {error && (
          <div className={styles.errorBanner} role="alert" data-testid="error-banner">
            {error}
          </div>
        )}

        {signedIn === null && <p className={styles.note}>Loading…</p>}

        {signedIn === false && (
          <div className={styles.emptyState}>
            <h2>Sign in first</h2>
            <p>Your public profile is tied to you, so sign in and connect an account to get started.</p>
            <Link href="/portfolio" className={styles.btnPrimary} style={{ textDecoration: "none" }}>
              Go to Portfolio
            </Link>
          </div>
        )}

        {signedIn && address && <PublicLinkCard address={address} />}

        {signedIn && address && (
          <div className={styles.card} data-testid="privacy-mode-card">
            <div className={styles.cardTitleRow}>
              <div className={styles.cardTitle}>Full privacy mode</div>
              <div className={styles.dashActions}>
                <button
                  type="button"
                  role="switch"
                  aria-checked={privacyMode}
                  aria-label="Full privacy mode"
                  data-testid="privacy-mode-switch"
                  className={`${styles.switch} ${privacyMode ? styles.switchOn : ""}`}
                  onClick={toggle}
                >
                  <span />
                </button>
                <span className={styles.note} style={{ minWidth: 28 }}>
                  {privacyMode ? "On" : "Off"}
                </span>
              </div>
            </div>
            <p className={styles.note} style={{ margin: 0 }}>
              When on, anyone who opens your address only sees that the profile is in full privacy mode — no numbers, no
              chart, nothing. Turn it off any time.
            </p>
          </div>
        )}

        {signedIn && address && (
          <div data-testid="privacy-preview">
            <div className={styles.cardTitle} style={{ margin: "26px 0 12px" }}>
              This is what others see
            </div>
            {privacyMode && <p className={styles.note}>Full privacy mode — nothing is shown.</p>}
            {!privacyMode && visible && visible.length === 0 && <p className={styles.note}>Connect an account to have a track record.</p>}
            {!privacyMode &&
              visible?.map((track) => <TrackCard key={track.statement.accountId} track={track} chart={{ address }} />)}
          </div>
        )}
      </main>
    </AppShell>
  );
}
