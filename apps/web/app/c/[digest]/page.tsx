"use client";

import { useParams } from "next/navigation";
import { useEffect, useState } from "react";
import { AppShell } from "../../../components/AppShell";
import { fetchPublicClaim, type PublicClaim } from "../../../lib/api";
import { OriginBadge } from "../../../components/OriginBadge";
import { describeClaim } from "../../../lib/claimFormat";
import styles from "../../portfolio/portfolio.module.css";

/** A shared claim: what was claimed, by whom, until when — never the figure behind it. */
export default function PublicClaimPage() {
  const { digest } = useParams<{ digest: string }>();
  const [state, setState] = useState<
    | { status: "loading" }
    | { status: "missing" }
    | { status: "expired" }
    | { status: "ready"; claim: PublicClaim }
  >({ status: "loading" });

  useEffect(() => {
    fetchPublicClaim(digest).then((result) => {
      if (result.ok) setState({ status: "ready", claim: result.data });
      else setState({ status: result.status === 410 ? "expired" : "missing" });
    });
  }, [digest]);

  return (
    <AppShell>
      <main className="shell">
        <div className={styles.hero}>
          <div className={styles.heroLabel}>Claim</div>
          {state.status === "loading" && (
            <p className={styles.note}>Loading…</p>
          )}
          {state.status === "missing" && (
            <h1 className={styles.heroTitle}>This claim doesn’t exist.</h1>
          )}
          {state.status === "expired" && (
            <h1 className={styles.heroTitle}>This claim has expired.</h1>
          )}
          {state.status === "ready" && (
            <>
              <h1 className={styles.heroTitle} data-testid="claim-title">
                {state.claim.claimSet.claims.map(describeClaim).join(" · ")}
              </h1>
              <p className={styles.heroSubtitle}>
                Claimed by{" "}
                <a
                  href={`/p/${state.claim.owner}`}
                  style={{ textDecoration: "underline" }}
                >
                  {state.claim.owner.slice(0, 6)}…{state.claim.owner.slice(-4)}
                </a>{" "}
                about their combined track record, counted since{" "}
                {new Date(
                  state.claim.claimSet.periodStart,
                ).toLocaleDateString()}
                . It was true when it was signed, and the owner authorized
                exactly this statement. The figure behind it stays private.
                Valid until{" "}
                {new Date(state.claim.claimSet.expiresAt).toLocaleDateString()}.
              </p>
              <p>
                <OriginBadge origin={state.claim.origin} />
              </p>
            </>
          )}
        </div>
        {state.status === "ready" && (
          <div className={styles.card}>
            <div className={styles.cardTitleRow}>
              <span className={styles.cardTitle}>
                What this does and doesn’t show
              </span>
            </div>
            <p className={styles.note}>
              The numbers come from a read-only connection to the exchange and
              were checked by Linvesther at signing time; this is not yet a
              zero-knowledge proof. The claim id is{" "}
              <code>{digest.slice(0, 16)}…</code>.
            </p>
          </div>
        )}
      </main>
    </AppShell>
  );
}
