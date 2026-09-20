"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { fetchProfileSettings } from "../../lib/api";
import styles from "./portfolio.module.css";

/** The person's public address, ready to paste and share. Every connected
 * account is public there by default — as percentages only. */
export function PublicLinkCard({ address }: { address: `0x${string}` }) {
  const [origin, setOrigin] = useState("");
  const [privacyMode, setPrivacyMode] = useState(false);
  const [copied, setCopied] = useState<"address" | "link" | null>(null);

  useEffect(() => {
    setOrigin(window.location.origin);
    fetchProfileSettings().then((result) => {
      if (result.ok) setPrivacyMode(result.data.privacyMode);
    });
  }, [address]);

  const link = `${origin}/p/${address}`;
  const copy = (what: "address" | "link") => {
    navigator.clipboard?.writeText(what === "address" ? address : link);
    setCopied(what);
    setTimeout(() => setCopied(null), 1500);
  };

  return (
    <div className={styles.publicCard} data-testid="public-link-card">
      <div className={styles.publicTop}>
        <div>
          <div className={styles.dashLabel}>Your public address</div>
          <code className={styles.publicAddress} data-testid="public-address">
            {address}
          </code>
        </div>
        <div className={styles.publicActions}>
          <button type="button" className={styles.btnSecondary} onClick={() => copy("address")}>
            {copied === "address" ? "Copied" : "Copy address"}
          </button>
          <button type="button" className={styles.btnPrimary} onClick={() => copy("link")} data-testid="copy-public-link">
            {copied === "link" ? "Link copied" : "Copy public link"}
          </button>
        </div>
      </div>
      <p className={styles.note} style={{ margin: "12px 0 0" }} data-testid="public-link-status">
        {privacyMode ? (
          <>Privacy mode is on — anyone with this address sees that the profile is private, and nothing else. </>
        ) : (
          <>
            Anyone with this address sees all your accounts added together, as <strong>percentages only</strong> — never
            balances, positions or trades.{" "}
          </>
        )}
        <Link href="/disclose" className={styles.inlineLink}>
          Privacy mode
        </Link>
        {" · "}
        <Link href={`/p/${address}`} className={styles.inlineLink}>
          See it as others do
        </Link>
      </p>
    </div>
  );
}
