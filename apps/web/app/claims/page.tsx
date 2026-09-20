"use client";

import Link from "next/link";
import { useCallback, useEffect, useState } from "react";
import { AppShell } from "../../components/AppShell";
import {
  checkSession,
  discloseClaimSet,
  fetchClaimContext,
  fetchMyClaims,
  type ClaimContext,
  type ClaimSet,
  type MyClaim,
} from "../../lib/api";
import { consentToClaimSet, usesPassword } from "../../lib/claimConsent";
import {
  CLAIM_METRICS,
  describeClaim,
  type ClaimMetricKey,
} from "../../lib/claimFormat";
import styles from "../portfolio/portfolio.module.css";
import own from "./claims.module.css";

const VALID_FOR_DAYS = 30;

const ERRORS: Record<string, string> = {
  claim_not_satisfied:
    "Your record doesn't meet that yet, so it can't be claimed. Try a lower bar.",
  metric_unavailable: "There isn't enough history for that figure yet.",
  nothing_to_prove:
    "Connect an account first — there is no record to claim about.",
  unsupported_claim: "That claim isn't supported.",
  signature_invalid: "The password or device check didn't match this identity.",
};

/** Claims: a specific statement about the combined track record — "return at
 * least 10%" — that is only accepted if it is true right now, is authorized by
 * the owner's own password or device, and is shared as a link. The exact
 * figure behind it stays private. */
export default function ClaimsPage() {
  const [signedIn, setSignedIn] = useState<boolean | null>(null);
  const [context, setContext] = useState<ClaimContext["current"]>(null);
  const [claims, setClaims] = useState<MyClaim[]>([]);
  const [metric, setMetric] = useState<ClaimMetricKey>("return");
  const [percent, setPercent] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const [needsPassword, setNeedsPassword] = useState(false);

  const load = useCallback(async () => {
    const [ctx, mine] = await Promise.all([
      fetchClaimContext(),
      fetchMyClaims(),
    ]);
    if (ctx.ok) setContext(ctx.data.current);
    if (mine.ok) setClaims(mine.data.claims);
  }, []);

  useEffect(() => {
    setNeedsPassword(usesPassword());
    checkSession().then((session) => {
      setSignedIn(session.ok);
      if (session.ok) void load();
    });
  }, [load]);

  async function publish() {
    setError(null);
    const bar = Number(percent);
    if (!percent.trim() || !Number.isFinite(bar))
      return setError("Enter a percentage.");
    const meta = CLAIM_METRICS[metric];
    setBusy(true);
    try {
      const fresh = await fetchClaimContext();
      if (!fresh.ok) return setError("Sign in again first.");
      const current = fresh.data.current;
      if (!current) return setError(ERRORS.nothing_to_prove!);
      const claimSet: ClaimSet = {
        identityId: fresh.data.address,
        trackId: "all",
        checkpointId: current.computedAt,
        periodStart: current.since,
        periodEnd: current.computedAt,
        claims: [{ type: meta.type, metric, threshold: String(bar / 100) }],
        audience: "public",
        nonce: crypto.randomUUID(),
        expiresAt: new Date(
          Date.now() + VALID_FOR_DAYS * 24 * 60 * 60_000,
        ).toISOString(),
      };
      const credential = await consentToClaimSet(claimSet, password);
      const result = await discloseClaimSet(claimSet, credential);
      if (!result.ok)
        return setError(
          ERRORS[result.error] ?? `Could not publish: ${result.error}`,
        );
      setPassword("");
      setPercent("");
      await load();
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : "Could not publish the claim.",
      );
    } finally {
      setBusy(false);
    }
  }

  const copy = (digest: string) => {
    navigator.clipboard?.writeText(`${window.location.origin}/c/${digest}`);
    setCopied(digest);
    setTimeout(() => setCopied(null), 1500);
  };

  return (
    <AppShell>
      <main className="shell">
        <div className={styles.hero}>
          <div className={styles.heroLabel}>Claims</div>
          <h1 className={styles.heroTitle}>
            Prove a point. Reveal nothing else.
          </h1>
          <p className={styles.heroSubtitle}>
            Pick a statement about your combined record — like “return at least
            10%” — and share it as a link. It is only accepted if it is true
            right now, you authorize it yourself, and the exact figure behind it
            is never shown. A claim stays valid for {VALID_FOR_DAYS} days.
          </p>
        </div>

        {error && (
          <div
            className={styles.errorBanner}
            role="alert"
            data-testid="error-banner"
          >
            {error}
          </div>
        )}

        {signedIn === null && <p className={styles.note}>Loading…</p>}

        {signedIn === false && (
          <div className={styles.emptyState}>
            <h2>Sign in first</h2>
            <p>
              Claims are tied to you, so sign in and connect an account to get
              started.
            </p>
            <Link
              href="/portfolio"
              className={styles.btnPrimary}
              style={{ textDecoration: "none" }}
            >
              Go to Portfolio
            </Link>
          </div>
        )}

        {signedIn && (
          <>
            <div className={styles.card}>
              <div className={styles.cardTitleRow}>
                <span className={styles.cardTitle}>New claim</span>
              </div>
              {!context && (
                <p className={styles.note}>
                  Connect an account and let some history build up to make a
                  claim.
                </p>
              )}
              <div className={own.row}>
                <div className={styles.field}>
                  <label className={styles.fieldLabel} htmlFor="claim-metric">
                    Statement
                  </label>
                  <select
                    id="claim-metric"
                    className={own.select}
                    value={metric}
                    onChange={(e) =>
                      setMetric(e.target.value as ClaimMetricKey)
                    }
                  >
                    {(Object.keys(CLAIM_METRICS) as ClaimMetricKey[]).map(
                      (key) => (
                        <option key={key} value={key}>
                          {CLAIM_METRICS[key].label} {CLAIM_METRICS[key].phrase}
                          …
                        </option>
                      ),
                    )}
                  </select>
                </div>
                <div className={styles.field}>
                  <label className={styles.fieldLabel} htmlFor="claim-percent">
                    Percent
                  </label>
                  <input
                    id="claim-percent"
                    inputMode="decimal"
                    placeholder="10"
                    value={percent}
                    onChange={(e) => setPercent(e.target.value)}
                  />
                </div>
              </div>
              {needsPassword && (
                <div className={styles.field}>
                  <label className={styles.fieldLabel} htmlFor="claim-password">
                    Your password — to authorize this claim
                  </label>
                  <input
                    id="claim-password"
                    type="password"
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                  />
                </div>
              )}
              <button
                type="button"
                className={styles.btnPrimary}
                onClick={publish}
                disabled={busy || !context}
                data-testid="publish-claim"
              >
                {busy ? "Authorizing…" : "Authorize and publish"}
              </button>
              <p className={styles.note}>
                The claim covers your combined record, counted from when you
                connected. It is checked against your real figures before it is
                accepted.
              </p>
            </div>

            <div className={styles.card}>
              <div className={styles.cardTitleRow}>
                <span className={styles.cardTitle}>Your claims</span>
              </div>
              {claims.length === 0 && (
                <p className={styles.note}>No claims yet.</p>
              )}
              <ul className={own.list}>
                {claims.map(({ digest, claimSet, publishedAt }) => (
                  <li key={digest} className={own.item}>
                    <div>
                      <strong>
                        {claimSet.claims.map(describeClaim).join(" · ")}
                      </strong>
                      <span className={own.meta}>
                        {publishedAt
                          ? `Published ${new Date(publishedAt).toLocaleDateString()} · `
                          : ""}
                        valid until{" "}
                        {new Date(claimSet.expiresAt).toLocaleDateString()}
                      </span>
                    </div>
                    <div className={own.actions}>
                      <a href={`/c/${digest}`} className={styles.btnSecondary}>
                        Open
                      </a>
                      <button
                        type="button"
                        className={styles.btnPrimary}
                        onClick={() => copy(digest)}
                      >
                        {copied === digest ? "Copied" : "Copy link"}
                      </button>
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          </>
        )}
      </main>
    </AppShell>
  );
}
