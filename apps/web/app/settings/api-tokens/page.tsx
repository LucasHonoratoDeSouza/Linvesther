"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { AppShell } from "../../../components/AppShell";
import {
  checkSession,
  createApiToken,
  fetchApiTokens,
  revokeApiToken,
  type ApiToken,
} from "../../../lib/api";
import styles from "../../portfolio/portfolio.module.css";
import own from "./apiTokens.module.css";

/** Tokens an owner hands to a program that should read their account and
 * nothing else. The token is shown once, here, and never again: only its
 * hash is kept, so a lost one is replaced rather than recovered. */
const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:4301";

const when = (iso: string | null) => (iso === null ? "never" : new Date(iso).toLocaleString());

const isLive = (token: ApiToken) => token.revokedAt === null;

export default function ApiTokensPage() {
  const [signedIn, setSignedIn] = useState<boolean | null>(null);
  const [tokens, setTokens] = useState<ApiToken[] | null>(null);
  const [maxActive, setMaxActive] = useState<number | null>(null);
  const [label, setLabel] = useState("");
  const [creating, setCreating] = useState(false);
  const [revoking, setRevoking] = useState<string | null>(null);
  /** The token just created, held only in this page's state for as long
   * as the person is looking at it. */
  const [revealed, setRevealed] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function load() {
    const result = await fetchApiTokens();
    if (!result.ok) return setError(`Could not load your tokens: ${result.error}`);
    setTokens(result.data.tokens);
    setMaxActive(result.data.maxActive);
  }

  useEffect(() => {
    checkSession().then(async (session) => {
      setSignedIn(session.ok);
      if (session.ok) await load();
    });
  }, []);

  async function create() {
    setError(null);
    setRevealed(null);
    setCopied(false);
    setCreating(true);
    try {
      const result = await createApiToken(label.trim());
      if (!result.ok) return setError(`Could not create the token: ${result.error}`);
      setRevealed(result.data.token);
      setLabel("");
      await load();
    } finally {
      setCreating(false);
    }
  }

  async function revoke(token: ApiToken) {
    setError(null);
    setRevoking(token.id);
    try {
      const result = await revokeApiToken(token.id);
      if (!result.ok) return setError(`Could not revoke “${token.label}”: ${result.error}`);
      // The revealed token may be the one just revoked; either way it is
      // no longer something to keep on screen.
      setRevealed(null);
      await load();
    } finally {
      setRevoking(null);
    }
  }

  const live = tokens?.filter(isLive).length ?? 0;
  const atCap = maxActive !== null && live >= maxActive;

  return (
    <AppShell>
      <main className="shell">
        <div className={styles.hero}>
          <div className={styles.heroLabel}>API tokens</div>
          <h1 className={styles.heroTitle}>Let a program read your account — and nothing else.</h1>
          <p className={styles.heroSubtitle}>
            A token here reads your balance, positions, profit, performance, trades and chart. It cannot place or cancel
            an order, move funds, connect or disconnect an account, or change any setting. You can revoke one at any
            moment, and it stops working immediately.
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
            <p>A token reads your own account, so sign in before creating one.</p>
            <Link href="/portfolio" className={styles.btnPrimary} style={{ textDecoration: "none" }}>
              Go to Portfolio
            </Link>
          </div>
        )}

        {signedIn && (
          <>
            <div className={styles.card} data-testid="create-token-card">
              <div className={styles.cardTitleRow}>
                <div className={styles.cardTitle}>Create a token</div>
              </div>
              {revealed && (
                <>
                  <div className={styles.statusBanner} role="status" data-testid="token-created">
                    Copy this token now — it is shown once. Only its fingerprint is stored, so it cannot be shown again.
                    If you lose it, revoke it and create another.
                  </div>
                  <div className={own.tokenReveal}>
                    <code className={own.tokenValue} data-testid="token-value">
                      {revealed}
                    </code>
                    <button
                      type="button"
                      className={styles.btnSecondary}
                      style={{ width: "auto" }}
                      data-testid="copy-token"
                      onClick={async () => {
                        await navigator.clipboard.writeText(revealed);
                        setCopied(true);
                      }}
                    >
                      {copied ? "Copied" : "Copy"}
                    </button>
                  </div>
                </>
              )}
              <div className={styles.field}>
                <label className={styles.fieldLabel} htmlFor="token-label">
                  What will hold it
                </label>
                <input
                  id="token-label"
                  value={label}
                  maxLength={40}
                  placeholder="Hermes, my laptop agent…"
                  data-testid="token-label"
                  onChange={(event) => setLabel(event.target.value)}
                />
              </div>
              <button
                type="button"
                className={styles.btnPrimary}
                disabled={creating || label.trim().length === 0 || atCap}
                data-testid="create-token"
                onClick={create}
              >
                {creating && <span className={styles.spinner} />}
                {creating ? "Creating…" : "Create token"}
              </button>
              {atCap && (
                <p className={styles.note} style={{ marginTop: 12 }}>
                  You already hold {maxActive} tokens, the most at once. Revoke one you no longer use to create another.
                </p>
              )}
            </div>

            <div className={styles.card} data-testid="tokens-card">
              <div className={styles.cardTitleRow}>
                <div className={styles.cardTitle}>Your tokens</div>
                {tokens !== null && <span className={styles.badge}>{live} in use</span>}
              </div>
              {tokens === null && <p className={styles.note}>Loading…</p>}
              {tokens !== null && tokens.length === 0 && (
                <p className={styles.note}>You have not created any token yet.</p>
              )}
              {tokens?.map((token) => (
                <div className={own.tokenRow} key={token.id} data-testid="token-row">
                  <div className={own.tokenMeta}>
                    <div className={`${own.tokenName} ${isLive(token) ? "" : own.tokenRevoked}`}>{token.label}</div>
                    <div className={own.tokenDates}>
                      {token.scope} · created {when(token.createdAt)} · last used {when(token.lastUsedAt)}
                      {token.revokedAt !== null && ` · revoked ${when(token.revokedAt)}`}
                    </div>
                  </div>
                  {isLive(token) ? (
                    <button
                      type="button"
                      className={own.revokeButton}
                      disabled={revoking === token.id}
                      data-testid="revoke-token"
                      onClick={() => revoke(token)}
                    >
                      {revoking === token.id ? "Revoking…" : "Revoke"}
                    </button>
                  ) : (
                    <span className={styles.badge}>Revoked</span>
                  )}
                </div>
              ))}
            </div>

            <div className={styles.card}>
              <div className={styles.cardTitleRow}>
                <div className={styles.cardTitle}>How a program uses it</div>
              </div>
              <p className={styles.note} style={{ marginBottom: 14 }}>
                Send the token as a bearer credential. Every route below is a read.
              </p>
              <pre className={own.usage}>
                {`curl -H "Authorization: Bearer lvz_ro_…" \\\n  ${API_BASE_URL}/mcp/account/state`}
              </pre>
              <p className={styles.note} style={{ marginTop: 14 }}>
                For an assistant that speaks the Model Context Protocol, point it at the MCP server in{" "}
                <code>services/mcp-server</code> and give it the same token — it offers the same reads as six tools and
                registers nothing that writes.
              </p>
            </div>
          </>
        )}
      </main>
    </AppShell>
  );
}
