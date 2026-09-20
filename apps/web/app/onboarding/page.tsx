"use client";

import { passwordProblem } from "../../lib/passwordPolicy";
import { useEffect, useState, type ChangeEvent } from "react";
import { AppShell } from "../../components/AppShell";
import Link from "next/link";
import { Icon } from "../../components/Icon";
import styles from "./onboarding.module.css";
import {
  beginConfirmIdentity,
  beginPasskeyLogin,
  beginPasskeyRegistration,
  beginVaultChallenge,
  checkSession,
  confirmIdentity,
  createIdentity,
  finishPasskeyLogin,
  fetchMyIdentity,
  finishPasskeyRegistration,
  signOut,
  verifyVaultSignature,
  type Identity,
} from "../../lib/api";
import {
  forgetStoredVaultIdentity,
  persistIdentityMethod,
  readStoredMethod,
  readStoredVaultIdentities,
  readStoredVaultIdentity,
  vaultAddress,
} from "../../lib/identitySession";
import {
  authenticatePasskey,
  isWebAuthnSupported,
  registerPasskey,
  WebAuthnCancelledError,
  WebAuthnUnsupportedError,
} from "../../lib/webauthn";
import {
  createVaultIdentity,
  InvalidVaultBackupError,
  parseVaultBackup,
  serializeVaultBackup,
  signDigestWithVaultIdentity,
  signWithVaultIdentity,
  unlockVaultIdentity,
  WrongVaultPasswordError,
  type VaultIdentity,
} from "../../lib/vault";
import { hashTypedData } from "viem";

function describeAuthError(cause: unknown): string {
  if (cause instanceof WebAuthnUnsupportedError) return cause.message;
  if (cause instanceof WebAuthnCancelledError) return cause.message;
  if (cause instanceof WrongVaultPasswordError) return cause.message;
  if (cause instanceof InvalidVaultBackupError) return cause.message;
  return cause instanceof Error ? cause.message : "sign-in failed";
}

/** Triggers a browser download of the vault's encrypted blob so it can
 * be carried to another device — the file is no more sensitive than
 * what already sits in this browser's own storage (still useless
 * without the password), it just isn't confined to this one browser. */
function downloadVaultBackup(vault: VaultIdentity) {
  const blob = new Blob([serializeVaultBackup(vault)], {
    type: "application/json",
  });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = "linvesther-identity-backup.json";
  link.click();
  URL.revokeObjectURL(url);
}

export default function OnboardingPage() {
  const [signedIn, setSignedIn] = useState(false);
  const [signedInMethod, setSignedInMethod] = useState<
    "webauthn" | "vault" | null
  >(null);
  // Kept only for the lifetime of this page — non-extractable
  // (imported with extractable: false in unlockVaultIdentity), so
  // holding it in state doesn't expose raw key material any more than
  // holding a reference to it during the sign-in call already did.
  // Confirming identity creation is a second signature, after the
  // first sign-in call already returned.
  const [vaultPrivateKey, setVaultPrivateKey] = useState<CryptoKey | null>(
    null,
  );
  const [identity, setIdentity] = useState<Identity | null>(null);
  const [createBusy, setCreateBusy] = useState(false);
  const [confirmBusy, setConfirmBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Detected after mount, not during render: a server-rendered
  // pass always lacks `window`, so support must be checked client-side
  // before any passkey button can appear — never attempted-then-caught.
  async function loadMyIdentity() {
    const result = await fetchMyIdentity();
    if (result.ok) setIdentity(result.data.identity);
  }

  const [webAuthnSupported, setWebAuthnSupported] = useState(false);
  // WebAuthn's spec requires the site's effective domain to be a real
  // (registrable) domain or "localhost" — browsers reject a bare IP
  // literal as an rpId with a SecurityError ("The operation is
  // insecure.") the moment navigator.credentials.create() is called.
  // isWebAuthnSupported() alone can't see this (the API itself is
  // present), so a site reached by IP needs its own check to avoid
  // offering a button that's guaranteed to fail.
  const [isIpHost, setIsIpHost] = useState(false);
  const [storedVault, setStoredVault] = useState<VaultIdentity | null>(null);
  const [storedVaults, setStoredVaults] = useState<VaultIdentity[]>([]);
  const [addingIdentity, setAddingIdentity] = useState(false);
  useEffect(() => {
    setWebAuthnSupported(isWebAuthnSupported());
    setIsIpHost(/^(\d{1,3}\.){3}\d{1,3}$/.test(window.location.hostname));
    setStoredVault(readStoredVaultIdentity());
    setStoredVaults(readStoredVaultIdentities());

    // A session cookie from an earlier sign-in (on this or another
    // page) is still valid — skip straight past the sign-in form
    // instead of asking again, the same session most other pages now
    // check for too.
    checkSession().then((result) => {
      if (result.ok) {
        setSignedInMethod(readStoredMethod());
        setSignedIn(true);
        loadMyIdentity();
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
        setError(
          `Could not start passkey registration: ${optionsResult.error}`,
        );
        return;
      }
      const response = await registerPasskey(optionsResult.data);
      const verifyResult = await finishPasskeyRegistration(response);
      if (!verifyResult.ok) {
        setError(`Passkey registration was rejected: ${verifyResult.error}`);
        return;
      }
      persistIdentityMethod("webauthn");
      setSignedInMethod("webauthn");
      setSignedIn(true);
      loadMyIdentity();
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
      setSignedInMethod("webauthn");
      setSignedIn(true);
      loadMyIdentity();
    } catch (cause) {
      setError(describeAuthError(cause));
    } finally {
      setPasskeyBusy(false);
    }
  }

  async function handleCreateVault() {
    setError(null);
    const problem = passwordProblem(vaultPassword);
    if (problem) {
      setError(problem);
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
      const privateKey = await unlockVaultIdentity(
        vault.encryptedPrivateKey,
        vaultPassword,
      );
      const signature = await signWithVaultIdentity(
        privateKey,
        challengeResult.data.challenge,
      );
      const verifyResult = await verifyVaultSignature(
        vault.qx,
        vault.qy,
        challengeResult.data.challenge,
        signature,
      );
      if (!verifyResult.ok) {
        setError(`Password sign-in was rejected: ${verifyResult.error}`);
        return;
      }
      persistIdentityMethod("vault", vault);
      setStoredVault(vault);
      setStoredVaults(readStoredVaultIdentities());
      setAddingIdentity(false);
      setVaultPrivateKey(privateKey);
      setVaultPassword("");
      setSignedInMethod("vault");
      setSignedIn(true);
      loadMyIdentity();
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
      const privateKey = await unlockVaultIdentity(
        storedVault.encryptedPrivateKey,
        vaultPassword,
      );
      const signature = await signWithVaultIdentity(
        privateKey,
        challengeResult.data.challenge,
      );
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
      setVaultPrivateKey(privateKey);
      setVaultPassword("");
      setSignedInMethod("vault");
      setSignedIn(true);
      loadMyIdentity();
    } catch (cause) {
      setError(describeAuthError(cause));
    } finally {
      setVaultBusy(false);
    }
  }

  /** Ends the session without touching any stored identity, so a
   * stale sign-in (or the wrong one) can be backed out of and a
   * different stored identity picked instead. */
  async function handleSignOut() {
    setError(null);
    await signOut();
    setSignedIn(false);
    setSignedInMethod(null);
    setVaultPrivateKey(null);
    setIdentity(null);
    setStoredVault(readStoredVaultIdentity());
    setStoredVaults(readStoredVaultIdentities());
  }

  function handleForgetStoredVault() {
    if (!storedVault) return;
    forgetStoredVaultIdentity(storedVault);
    const remaining = readStoredVaultIdentities();
    setStoredVaults(remaining);
    setStoredVault(remaining[0] ?? null);
    setVaultPassword("");
    setError(null);
  }

  async function handleImportVaultBackup(event: ChangeEvent<HTMLInputElement>) {
    setError(null);
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) return;
    try {
      const vault = parseVaultBackup(await file.text());
      persistIdentityMethod("vault", vault);
      setStoredVaults(readStoredVaultIdentities());
      setStoredVault(vault);
      setAddingIdentity(false);
    } catch (cause) {
      setError(describeAuthError(cause));
    }
  }

  async function handleCreateIdentity() {
    setError(null);
    setCreateBusy(true);
    try {
      const result = await createIdentity();
      if (!result.ok) {
        setError(`Could not create identity: ${result.error}`);
        return;
      }
      setIdentity(result.data);
    } finally {
      setCreateBusy(false);
    }
  }

  /** Confirming is a second real signature over the identity's actual
   * on-chain rotation challenge — only the vault method can produce it
   * right now (apps/api/src/lifecycle/chainLifecycle.ts's
   * UnsupportedConfirmationMethodError): a passkey confirmation needs
   * its WebAuthn assertion re-encoded into WebAuthnAccount's on-chain
   * wire format, which nothing in this codebase does yet. */
  async function handleConfirmIdentity() {
    if (!identity || signedInMethod !== "vault" || !vaultPrivateKey) return;
    setError(null);
    setConfirmBusy(true);
    try {
      const challengeResult = await beginConfirmIdentity(identity.identityId);
      if (!challengeResult.ok) {
        setError(`Could not start confirmation: ${challengeResult.error}`);
        return;
      }
      const challenge = challengeResult.data;
      const digest = hashTypedData({
        domain: challenge.domain,
        types: challenge.types,
        primaryType: challenge.primaryType,
        message: {
          identityId: challenge.message.identityId,
          newOwner: challenge.message.newOwner,
          ownerEpoch: BigInt(challenge.message.ownerEpoch),
          nonce: BigInt(challenge.message.nonce),
          deadline: BigInt(challenge.message.deadline),
        },
      });
      const signature = await signDigestWithVaultIdentity(
        vaultPrivateKey,
        digest,
      );
      const confirmResult = await confirmIdentity(
        identity.identityId,
        challenge,
        signature,
      );
      if (!confirmResult.ok) {
        setError(`Confirmation was rejected: ${confirmResult.error}`);
        return;
      }
      setIdentity(confirmResult.data);
    } catch (cause) {
      setError(describeAuthError(cause));
    } finally {
      setConfirmBusy(false);
    }
  }

  const steps = [
    { done: signedIn, label: "Sign in" },
    { done: identity !== null, label: "Create your account" },
    { done: identity?.state === "active", label: "Activate it" },
  ];

  return (
    <AppShell>
      <main className="shell">
        {error && (
          <div
            className={styles.errorBanner}
            role="alert"
            data-testid="error-banner"
          >
            {error}
          </div>
        )}

        <div className={styles.columns}>
          <div>
            <div className={styles.card}>
              <div className={styles.cardTitleRow}>
                <div className={styles.cardTitle}>Your identity</div>
                <span className={styles.eyebrow}>
                  {steps.filter((step) => step.done).length} / 3
                </span>
              </div>
              <ul className={styles.stepList}>
                {steps.map((step, i) => (
                  <li key={i} className={step.done ? styles.done : ""}>
                    <span>{step.done ? "✓" : i + 1}</span>
                    <span>{step.label}</span>
                  </li>
                ))}
              </ul>

              {!signedIn && (
                <>
                  <p className={styles.lead}>
                    Choose one way to sign in. Either one proves an account is
                    yours — pick whichever is easier for you.
                  </p>

                  {webAuthnSupported && !isIpHost && (
                    <div style={{ marginBottom: 20 }}>
                      <div className={styles.sectionLabel}>
                        Option A — Face, fingerprint or screen lock
                      </div>
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
                  {!webAuthnSupported && !isIpHost && (
                    <p
                      data-testid="webauthn-unsupported-note"
                      className={styles.note}
                    >
                      Your device doesn’t support fingerprint/face sign-in — use
                      a password below instead.
                    </p>
                  )}
                  {isIpHost && (
                    <p
                      data-testid="webauthn-ip-host-note"
                      className={styles.note}
                      style={{ marginBottom: 20 }}
                    >
                      Face/fingerprint sign-in needs a real web address — it's
                      not available on this preview link. Use a password below
                      instead.
                    </p>
                  )}

                  <div className={styles.sectionLabel}>Option B — Password</div>
                  {storedVault &&
                    !addingIdentity &&
                    storedVaults.length > 1 && (
                      <div className={styles.field}>
                        <label
                          className={styles.fieldLabel}
                          htmlFor="vault-identity"
                        >
                          Which identity?
                        </label>
                        <select
                          id="vault-identity"
                          data-testid="vault-identity-select"
                          value={storedVaults.findIndex(
                            (v) =>
                              v.qx === storedVault.qx &&
                              v.qy === storedVault.qy,
                          )}
                          onChange={(e) => {
                            setStoredVault(
                              storedVaults[Number(e.target.value)] ??
                                storedVault,
                            );
                            setVaultPassword("");
                          }}
                        >
                          {storedVaults.map((v, i) => (
                            <option key={v.qx + v.qy} value={i}>
                              {vaultAddress(v).slice(0, 8)}…
                              {vaultAddress(v).slice(-6)}
                            </option>
                          ))}
                        </select>
                        <button
                          type="button"
                          className={styles.btnSecondary}
                          data-testid="vault-download-backup-picker-button"
                          onClick={() => downloadVaultBackup(storedVault)}
                          style={{ marginTop: 8 }}
                        >
                          Save a backup of this identity
                        </button>
                      </div>
                    )}
                  <div className={styles.field}>
                    <label
                      className={styles.fieldLabel}
                      htmlFor="vault-password"
                    >
                      {storedVault && !addingIdentity
                        ? "Enter the password you set before"
                        : "Choose a password"}
                    </label>
                    <input
                      id="vault-password"
                      data-testid="vault-password-input"
                      type="password"
                      value={vaultPassword}
                      onChange={(e) => setVaultPassword(e.target.value)}
                    />
                  </div>
                  {storedVault && !addingIdentity ? (
                    <>
                      <button
                        className={styles.btnPrimary}
                        data-testid="vault-unlock-button"
                        onClick={handleUnlockVault}
                        disabled={vaultBusy}
                        style={{ marginRight: 8 }}
                      >
                        Continue
                      </button>
                      <button
                        type="button"
                        className={styles.btnSecondary}
                        data-testid="vault-forget-button"
                        onClick={handleForgetStoredVault}
                      >
                        Forgot it? Remove this identity
                      </button>
                      <button
                        type="button"
                        className={styles.btnSecondary}
                        data-testid="vault-add-identity-button"
                        onClick={() => {
                          setAddingIdentity(true);
                          setVaultPassword("");
                        }}
                        style={{ marginLeft: 8 }}
                      >
                        Create another identity
                      </button>
                    </>
                  ) : (
                    <>
                      <button
                        className={styles.btnPrimary}
                        data-testid="vault-create-button"
                        onClick={handleCreateVault}
                        disabled={vaultBusy}
                        style={{ marginRight: 8 }}
                      >
                        Continue
                      </button>
                      <label
                        className={styles.btnSecondary}
                        style={{ display: "inline-flex", cursor: "pointer" }}
                      >
                        Or restore from a backup file
                        <input
                          type="file"
                          accept="application/json"
                          data-testid="vault-import-input"
                          onChange={handleImportVaultBackup}
                          style={{ display: "none" }}
                        />
                      </label>
                      {addingIdentity && (
                        <button
                          type="button"
                          className={styles.btnSecondary}
                          onClick={() => setAddingIdentity(false)}
                          style={{ marginLeft: 8 }}
                        >
                          Back to sign-in
                        </button>
                      )}
                    </>
                  )}
                </>
              )}
              {signedIn && (
                <>
                  <p
                    data-testid="signed-in-indicator"
                    className={styles.lead}
                    style={{ marginBottom: 14, fontWeight: 590 }}
                  >
                    You’re signed in.{" "}
                    <button
                      type="button"
                      className={styles.inlineTextLink}
                      data-testid="sign-out-button"
                      onClick={handleSignOut}
                      style={{ font: "inherit" }}
                    >
                      Not you? Sign out
                    </button>
                  </p>

                  {storedVault && (
                    <>
                      <button
                        className={styles.btnSecondary}
                        data-testid="vault-download-backup-button"
                        onClick={() => downloadVaultBackup(storedVault)}
                        style={{ marginBottom: 8, display: "inline-flex" }}
                      >
                        Save a backup file
                      </button>
                      <p className={styles.note} style={{ marginBottom: 16 }}>
                        Keep this file somewhere safe — it's the only way to
                        sign in from another device or browser.
                      </p>
                    </>
                  )}

                  {!identity && (
                    <button
                      className={styles.btnPrimary}
                      data-testid="create-identity-button"
                      onClick={handleCreateIdentity}
                      disabled={createBusy}
                    >
                      {createBusy ? (
                        <>
                          <span className={styles.spinner} aria-hidden="true" />
                          Setting up your account — a few seconds…
                        </>
                      ) : (
                        "Create my account"
                      )}
                    </button>
                  )}
                  {identity && identity.state === "pending_confirmation" && (
                    <>
                      <div
                        className={styles.statusBanner}
                        data-testid="identity-state"
                      >
                        Almost done — one more click to activate your account.
                      </div>
                      {signedInMethod === "vault" && (
                        <button
                          className={styles.btnPrimary}
                          data-testid="confirm-identity-button"
                          onClick={handleConfirmIdentity}
                          disabled={confirmBusy}
                        >
                          {confirmBusy && (
                            <span
                              className={styles.spinner}
                              aria-hidden="true"
                            />
                          )}
                          {confirmBusy
                            ? "Activating — a few seconds…"
                            : "Activate my account"}
                        </button>
                      )}
                      {signedInMethod === "webauthn" && (
                        <p
                          data-testid="confirm-unsupported-note"
                          className={styles.note}
                        >
                          Finishing setup with a fingerprint/face sign-in isn’t
                          available yet — please use a password instead for now.
                        </p>
                      )}
                    </>
                  )}
                  {identity && identity.state === "active" && (
                    <div data-testid="identity-active-summary">
                      <div
                        className={styles.statusBanner}
                        data-testid="identity-state"
                      >
                        Your account is set up and active.
                      </div>
                      <div className={styles.summaryBox}>
                        <p>
                          <strong>Never share:</strong> your password (or the
                          backup file). Anyone who has it can control your
                          account. Linvesther never sees or stores it.
                        </p>
                        <p>
                          <strong>Safe to share:</strong> your account address
                          below — it only identifies you, it can't be used to
                          access anything.
                        </p>
                        <p>
                          <code>{identity.owner}</code>
                        </p>
                      </div>
                      <Link
                        href="/portfolio"
                        className={styles.btnPrimary}
                        data-testid="continue-to-portfolio-link"
                        style={{ textDecoration: "none" }}
                      >
                        Go to my portfolio
                      </Link>
                    </div>
                  )}
                </>
              )}
            </div>

            <p className={styles.note}>
              Your password or device sign-in is what proves the account is
              yours. Some actions may take a few seconds to finish while they're
              confirmed.
            </p>
          </div>
          <aside className={styles.aside}>
            <Icon name="shield" width={24} />
            <h2>
              Your account.
              <br />
              Your rules.
            </h2>
            <p>
              You sign in with your own password or device — Linvesther never
              sees or stores it. You decide what, if anything, ever becomes
              visible to others.
            </p>
            <ul>
              <li>
                <Icon name="check" width={13} />
                Sign in with a password or your device
              </li>
              <li>
                <Icon name="check" width={13} />
                Choose what you ever make public
              </li>
              <li>
                <Icon name="check" width={13} />
                Your trading history stays private by default
              </li>
            </ul>
          </aside>
        </div>
      </main>
    </AppShell>
  );
}
