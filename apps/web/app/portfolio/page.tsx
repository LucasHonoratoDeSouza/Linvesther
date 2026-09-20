"use client";

import { useEffect, useState } from "react";
import { AppShell } from "../../components/AppShell";
import Link from "next/link";
import { Icon } from "../../components/Icon";
import styles from "./portfolio.module.css";
import { AccountView } from "./AccountView";
import { accountName, AllAccountsView } from "./AllAccountsView";
import { AddAccountModal } from "./AddAccountModal";
import { BrokerLogo } from "./BrokerLogo";
import { EyeToggle, PrivacyProvider, usePrivacy } from "./privacy";
import { ProfileEditor } from "./ProfileEditor";
import { PublicLinkCard } from "./PublicLinkCard";
import { brokerById, type BrokerDefinition } from "./brokers";
import { ConnectForm } from "./ConnectForm";
import { money } from "./format";
import {
  beginPasskeyLogin,
  beginPasskeyRegistration,
  beginVaultChallenge,
  checkSession,
  fetchAccounts,
  removeAccount,
  renameAccount,
  fetchBinanceNav,
  fetchBinancePerformance,
  syncBinanceAccount,
  finishPasskeyLogin,
  finishPasskeyRegistration,
  verifyVaultSignature,
  type BinanceNav,
  type BinancePerformance,
  type ConnectedAccount,
} from "../../lib/api";
import {
  authenticatePasskey,
  isWebAuthnSupported,
  registerPasskey,
  WebAuthnCancelledError,
  WebAuthnUnsupportedError,
} from "../../lib/webauthn";
import {
  createVaultIdentity,
  signWithVaultIdentity,
  unlockVaultIdentity,
  WrongVaultPasswordError,
  type VaultIdentity,
} from "../../lib/vault";
import { persistIdentityMethod, readStoredVaultIdentity, vaultAddress } from "../../lib/identitySession";

function describeAuthError(cause: unknown): string {
  if (cause instanceof WebAuthnUnsupportedError) return cause.message;
  if (cause instanceof WebAuthnCancelledError) return cause.message;
  if (cause instanceof WrongVaultPasswordError) return cause.message;
  return cause instanceof Error ? cause.message : "sign-in failed";
}

function PortfolioContent() {
  const [address, setAddress] = useState<`0x${string}` | null>(null);
  const [error, setError] = useState<string | null>(null);

  const [connecting, setConnecting] = useState(false);
  // null until the list has been fetched — distinguishes "still
  // loading" from "genuinely has no connected account yet".
  const [accounts, setAccounts] = useState<ConnectedAccount[] | null>(null);
  const [selected, setSelected] = useState<string>("all");
  const [panel, setPanel] = useState<"add" | "update" | null>(null);
  const [navs, setNavs] = useState<Record<string, BinanceNav>>({});
  const [perfs, setPerfs] = useState<Record<string, BinancePerformance>>({});
  const [loadingPerf, setLoadingPerf] = useState<Record<string, boolean>>({});
  const [refreshing, setRefreshing] = useState(false);
  const [updatedAt, setUpdatedAt] = useState<Date | null>(null);
  const { mask } = usePrivacy();

  const [webAuthnSupported, setWebAuthnSupported] = useState(false);
  const [storedVault, setStoredVault] = useState<VaultIdentity | null>(null);
  useEffect(() => {
    setWebAuthnSupported(isWebAuthnSupported());
    setStoredVault(readStoredVaultIdentity());

    // A session cookie from signing in elsewhere (e.g. /onboarding) is
    // still valid — skip straight to the portfolio instead of asking
    // to sign in again.
    checkSession().then((result) => {
      if (result.ok) {
        setAddress(result.data.address);
      }
    });
  }, []);

  const [vaultPassword, setVaultPassword] = useState("");
  const [passkeyBusy, setPasskeyBusy] = useState(false);
  const [vaultBusy, setVaultBusy] = useState(false);

  async function handleCreatePasskey() {
    setError(null);
    setPasskeyBusy(true);
    try {
      const optionsResult = await beginPasskeyRegistration();
      if (!optionsResult.ok) {
        setError(`Could not start passkey registration: ${optionsResult.error}`);
        return;
      }
      const response = await registerPasskey(optionsResult.data);
      const verifyResult = await finishPasskeyRegistration(response);
      if (!verifyResult.ok) {
        setError(`Passkey registration was rejected: ${verifyResult.error}`);
        return;
      }
      persistIdentityMethod("webauthn");
      setAddress(verifyResult.data.address);
    } catch (cause) {
      setError(describeAuthError(cause));
    } finally {
      setPasskeyBusy(false);
    }
  }

  async function handleLoginPasskey() {
    setError(null);
    setPasskeyBusy(true);
    try {
      const optionsResult = await beginPasskeyLogin();
      if (!optionsResult.ok) {
        setError(`Could not start passkey sign-in: ${optionsResult.error}`);
        return;
      }
      const response = await authenticatePasskey(optionsResult.data);
      const verifyResult = await finishPasskeyLogin(response);
      if (!verifyResult.ok) {
        setError(`Passkey sign-in was rejected: ${verifyResult.error}`);
        return;
      }
      persistIdentityMethod("webauthn");
      setAddress(verifyResult.data.address);
    } catch (cause) {
      setError(describeAuthError(cause));
    } finally {
      setPasskeyBusy(false);
    }
  }

  async function handleCreateVault() {
    setError(null);
    if (!vaultPassword) {
      setError("Enter a password first.");
      return;
    }
    setVaultBusy(true);
    try {
      const vault = await createVaultIdentity(vaultPassword);
      const challengeResult = await beginVaultChallenge();
      if (!challengeResult.ok) {
        setError(`Could not reach the server: ${challengeResult.error}`);
        return;
      }
      const privateKey = await unlockVaultIdentity(vault.encryptedPrivateKey, vaultPassword);
      const signature = await signWithVaultIdentity(privateKey, challengeResult.data.challenge);
      const verifyResult = await verifyVaultSignature(vault.qx, vault.qy, challengeResult.data.challenge, signature);
      if (!verifyResult.ok) {
        setError(`Password sign-in was rejected: ${verifyResult.error}`);
        return;
      }
      persistIdentityMethod("vault", vault);
      setStoredVault(vault);
      setVaultPassword("");
      setAddress(verifyResult.data.address);
    } catch (cause) {
      setError(describeAuthError(cause));
    } finally {
      setVaultBusy(false);
    }
  }

  async function handleUnlockVault() {
    setError(null);
    if (!storedVault) return;
    if (!vaultPassword) {
      setError("Enter your password first.");
      return;
    }
    setVaultBusy(true);
    try {
      const challengeResult = await beginVaultChallenge();
      if (!challengeResult.ok) {
        setError(`Could not reach the server: ${challengeResult.error}`);
        return;
      }
      const privateKey = await unlockVaultIdentity(storedVault.encryptedPrivateKey, vaultPassword);
      const signature = await signWithVaultIdentity(privateKey, challengeResult.data.challenge);
      const verifyResult = await verifyVaultSignature(
        storedVault.qx,
        storedVault.qy,
        challengeResult.data.challenge,
        signature,
      );
      if (!verifyResult.ok) {
        setError(`Password sign-in was rejected: ${verifyResult.error}`);
        return;
      }
      persistIdentityMethod("vault", storedVault);
      setVaultPassword("");
      setAddress(verifyResult.data.address);
    } catch (cause) {
      setError(describeAuthError(cause));
    } finally {
      setVaultBusy(false);
    }
  }

  // Balance and performance load independently and per account: the
  // balance is quick, the performance can take a few seconds, and each
  // part of the page appears as soon as its own data is ready.
  function loadAccount(accountId: string) {
    setLoadingPerf((current) => ({ ...current, [accountId]: true }));
    fetchBinanceNav(accountId).then((result) => {
      if (result.ok) setNavs((current) => ({ ...current, [accountId]: result.data }));
      else setError(`Could not load a balance: ${result.error}`);
    });
    fetchBinancePerformance(accountId)
      .then((result) => {
        if (result.ok) setPerfs((current) => ({ ...current, [accountId]: result.data }));
        else setError(`Could not load performance: ${result.error}`);
      })
      .finally(() => setLoadingPerf((current) => ({ ...current, [accountId]: false })));
  }

  /** "Refresh": ask the exchange for anything new (deposits/withdrawals) for
   * every account, then reload balances and performance. Trades already
   * arrive on their own; this covers the rest without waiting. */
  async function refreshAll() {
    if (!accounts || refreshing) return;
    setError(null);
    setRefreshing(true);
    try {
      await Promise.all(
        accounts.map(async (account) => {
          const result = await syncBinanceAccount(account.accountId);
          if (!result.ok) setError(`Could not refresh ${accountName(account, accounts.indexOf(account))}: ${result.error}`);
        }),
      );
      accounts.forEach((account) => loadAccount(account.accountId));
      setUpdatedAt(new Date());
    } finally {
      setRefreshing(false);
    }
  }

  async function rename(accountId: string, label: string): Promise<boolean> {
    const result = await renameAccount(accountId, label);
    if (!result.ok) {
      setError(`Could not rename the account: ${result.error}`);
      return false;
    }
    setAccounts((current) => current?.map((a) => (a.accountId === accountId ? { ...a, label: result.data.label } : a)) ?? current);
    return true;
  }

  async function remove(accountId: string): Promise<boolean> {
    const result = await removeAccount(accountId);
    if (!result.ok && result.status !== 404) {
      setError(`Could not remove the account: ${result.error}`);
      return false;
    }
    setAccounts((current) => current?.filter((a) => a.accountId !== accountId) ?? current);
    setSelected((current) => (current === accountId ? "all" : current));
    return true;
  }

  async function refreshAccounts(): Promise<ConnectedAccount[]> {
    const result = await fetchAccounts();
    const active = result.ok ? result.data.accounts.filter((a) => a.status === "active") : [];
    setAccounts(active);
    return active;
  }

  // Once signed in: list the connected accounts and load each one, so
  // the page survives a refresh instead of asking to connect again.
  useEffect(() => {
    if (!address) return;
    refreshAccounts().then((active) => {
      setSelected(active.length === 1 ? active[0]!.accountId : "all");
      active.forEach((a) => loadAccount(a.accountId));
      setUpdatedAt(new Date());
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [address]);

  function newAccountId(broker: BrokerDefinition, label: string): string {
    const slug =
      label
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/^-+|-+$/g, "")
        .slice(0, 20) || broker.id;
    const taken = new Set((accounts ?? []).map((a) => a.accountId.toLowerCase()));
    let candidate = `${address}_${slug}`;
    while (taken.has(candidate.toLowerCase())) {
      candidate = `${address}_${slug}-${Math.random().toString(36).slice(2, 6)}`;
    }
    return candidate;
  }

  /** Connects (or re-keys) an account; resolves true when it worked so
   * the form can clear its fields. */
  async function connectAccount(
    accountId: string,
    broker: BrokerDefinition,
    values: { label: string; credentials: Record<string, string> },
  ): Promise<boolean> {
    setError(null);
    setConnecting(true);
    try {
      const result = await broker.connect(accountId, values.credentials, values.label.trim() || undefined);
      if (!result.ok) {
        setError(`Could not connect ${broker.name}: ${result.error}`);
        return false;
      }
      await refreshAccounts();
      loadAccount(accountId);
      setSelected(accountId);
      setPanel(null);
      return true;
    } finally {
      setConnecting(false);
    }
  }

  const hasAccounts = (accounts?.length ?? 0) > 0;
  const selectedAccount = accounts?.find((a) => a.accountId === selected) ?? null;
  const selectedIndex = selectedAccount ? accounts!.indexOf(selectedAccount) : -1;
  const shownNavs = selectedAccount ? [navs[selectedAccount.accountId]] : accounts?.map((a) => navs[a.accountId]) ?? [];
  const totalBalance = shownNavs.reduce((sum, nav) => sum + Number(nav?.nav ?? 0), 0);
  const balanceReady = shownNavs.length > 0 && shownNavs.every(Boolean);

  return (
    <AppShell>
      <main className="shell">
        {!hasAccounts && (
          <div className={styles.hero}>
            <div className={styles.heroLabel}>Portfolio</div>
            <h1 className={styles.heroTitle}>Your performance, in perspective.</h1>
            <p className={styles.heroSubtitle}>
              Connect your exchange once and see how your money is really doing.
            </p>
          </div>
        )}

        {error && (
          <div className={styles.errorBanner} role="alert" data-testid="error-banner">
            {error}
          </div>
        )}

        {!address && (
          <div className={styles.columns}>
            <div className={styles.emptyState}>
              <Icon name="chart" width={38} height={38} />
              <h2>A clear view starts here.</h2>
              <p>
                Sign in to connect an exchange and explore your portfolio. A
                password or your device proves it's you — pick whichever is
                easier.
              </p>
              {webAuthnSupported && (
                <div style={{ marginBottom: 16 }}>
                  <button
                    className={styles.btnPrimary}
                    data-testid="passkey-register-button"
                    onClick={handleCreatePasskey}
                    disabled={passkeyBusy}
                    style={{ marginRight: 8 }}
                  >
                    Set up with my device
                  </button>
                  <button
                    className={styles.btnSecondary}
                    data-testid="passkey-login-button"
                    onClick={handleLoginPasskey}
                    disabled={passkeyBusy}
                  >
                    I already set this up
                  </button>
                </div>
              )}
              {!webAuthnSupported && (
                <p data-testid="webauthn-unsupported-note" className={styles.note}>
                  Your device doesn’t support fingerprint/face sign-in — use a
                  password below.
                </p>
              )}
              <div className={styles.field} style={{ textAlign: "left" }}>
                <label className={styles.fieldLabel} htmlFor="portfolio-vault-password">
                  Password
                </label>
                <input
                  id="portfolio-vault-password"
                  data-testid="vault-password-input"
                  type="password"
                  value={vaultPassword}
                  onChange={(e) => setVaultPassword(e.target.value)}
                />
              </div>
              {storedVault ? (
                <button
                  className={styles.btnPrimary}
                  data-testid="vault-unlock-button"
                  onClick={handleUnlockVault}
                  disabled={vaultBusy}
                >
                  Continue
                </button>
              ) : (
                <button
                  className={styles.btnPrimary}
                  data-testid="vault-create-button"
                  onClick={handleCreateVault}
                  disabled={vaultBusy}
                >
                  Continue
                </button>
              )}
              {storedVault && (
                <p className={styles.note} style={{ marginTop: 12 }}>
                  Signing in as {vaultAddress(storedVault).slice(0, 8)}…{vaultAddress(storedVault).slice(-6)}.{" "}
                  <Link href="/onboarding" className={styles.inlineLink}>
                    Use another identity
                  </Link>
                </p>
              )}
            </div>
            <aside className={styles.aside}>
              <Icon name="lock" width={24} />
              <h2>
                Read your history.
                <br />
                Keep your custody.
              </h2>
              <p>
                Your Binance connection is restricted to reading. Assets remain
                in your exchange account.
              </p>
              <ul>
                <li>
                  <Icon name="check" width={13} />
                  Current valuation
                </li>
                <li>
                  <Icon name="check" width={13} />
                  Historical performance
                </li>
                <li>
                  <Icon name="check" width={13} />
                  Visible coverage and limitations
                </li>
              </ul>
              <a href="/docs/getting-started" className={styles.textLink}>
                Connection guide <Icon name="diagonal" width={14} />
              </a>
            </aside>
          </div>
        )}

        {address && accounts === null && (
          <p className={styles.note} style={{ textAlign: "center" }}>
            Loading your accounts…
          </p>
        )}

        {address && accounts !== null && !hasAccounts && (
          <div className={styles.emptyState}>
            <h2>Connect your first account</h2>
            <p>
              Link an account you already trade in — read-only, so Linvesther can see it but never move anything — and
              your dashboard builds itself.
            </p>
            <button className={styles.btnPrimary} data-testid="add-first-account-button" onClick={() => setPanel("add")}>
              Connect an account
            </button>
          </div>
        )}

        {address && hasAccounts && accounts && (
          <div data-testid="portfolio-dashboard">
            <div className={styles.dashTop}>
              <div>
                <div className={styles.dashLabel}>
                  {selectedAccount ? accountName(selectedAccount, selectedIndex) : `Total balance · ${accounts.length} accounts`}
                </div>
                <div className={styles.dashValue} data-testid="nav-result" style={balanceReady ? undefined : { opacity: 0.4 }}>
                  {balanceReady ? mask(money(String(totalBalance))) : "…"} <sup>{selectedAccount ? (navs[selectedAccount.accountId]?.currency ?? "USD") : "USD"}</sup>
                </div>
              </div>
              <div className={styles.dashActions}>
                <EyeToggle />
                <button
                  type="button"
                  className={styles.btnSecondary}
                  onClick={refreshAll}
                  disabled={refreshing}
                  data-testid="refresh-button"
                  title="Fetch anything new from your accounts and reload the numbers"
                >
                  {refreshing ? (
                    <>
                      <span className={styles.spinner} aria-hidden="true" /> Refreshing…
                    </>
                  ) : (
                    "↻ Refresh"
                  )}
                </button>
                {selectedAccount && (
                  <div className={styles.dashConnected}>
                    <BrokerLogo brokerId={selectedAccount.broker} size={22} /> {brokerById(selectedAccount.broker)?.name ?? "Account"} connected
                    <button type="button" className={styles.linkButton} onClick={() => setPanel(panel === "update" ? null : "update")}>
                      {panel === "update" ? "Cancel" : "Change keys"}
                    </button>
                  </div>
                )}
              </div>
            </div>
            {updatedAt && (
              <p className={styles.note} style={{ margin: "-10px 0 16px" }} data-testid="updated-at">
                Updated {updatedAt.toLocaleTimeString("en-US", { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
              </p>
            )}

            <PublicLinkCard address={address} />
            <ProfileEditor />

            <div className={styles.chips} role="tablist" aria-label="Accounts">
              {accounts.length > 1 && (
                <button
                  type="button"
                  className={`${styles.chip} ${selected === "all" ? styles.chipActive : ""}`}
                  onClick={() => setSelected("all")}
                >
                  All accounts
                </button>
              )}
              {accounts.map((account, index) => (
                <button
                  type="button"
                  key={account.accountId}
                  className={`${styles.chip} ${selected === account.accountId ? styles.chipActive : ""}`}
                  onClick={() => setSelected(account.accountId)}
                >
                  {accountName(account, index)}
                </button>
              ))}
              <button
                type="button"
                className={`${styles.chip} ${styles.chipAdd}`}
                data-testid="add-account-button"
                onClick={() => setPanel(panel === "add" ? null : "add")}
              >
                + Add account
              </button>
            </div>

            {panel === "update" && selectedAccount && brokerById(selectedAccount.broker) && (
              <ConnectForm
                broker={brokerById(selectedAccount.broker)!}
                askForName={false}
                submitLabel="Save keys"
                busy={connecting}
                onSubmit={(values) =>
                  connectAccount(selectedAccount.accountId, brokerById(selectedAccount.broker)!, {
                    ...values,
                    label: selectedAccount.label ?? "",
                  })
                }
                onCancel={() => setPanel(null)}
              />
            )}

            {selectedAccount ? (
              <AccountView
                accountId={selectedAccount.accountId}
                sinceMs={selectedAccount.connectedAtMs}
                refreshKey={updatedAt?.getTime() ?? 0}
                nav={navs[selectedAccount.accountId] ?? null}
                performance={perfs[selectedAccount.accountId] ?? null}
                loadingPerformance={Boolean(loadingPerf[selectedAccount.accountId])}
              />
            ) : (
              <AllAccountsView accounts={accounts} navs={navs} perfs={perfs} onOpen={setSelected} onRename={rename} onRemove={remove} />
            )}

            <p className={styles.note} style={{ textAlign: "center" }}>
              Ready to prove something? <Link href="/disclose" className={styles.inlineLink}>Create a claim</Link>
            </p>
          </div>
        )}
        {address && panel === "add" && (
          <AddAccountModal
            busy={connecting}
            onClose={() => setPanel(null)}
            onConnect={(broker, values) => connectAccount(hasAccounts ? newAccountId(broker, values.label) : address, broker, values)}
          />
        )}
      </main>
    </AppShell>
  );
}

export default function PortfolioPage() {
  return (
    <PrivacyProvider>
      <PortfolioContent />
    </PrivacyProvider>
  );
}
