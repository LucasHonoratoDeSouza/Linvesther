"use client";

import { useEffect, useState } from "react";
import { hashTypedData } from "viem";
import { beginSetProfile, fetchIdentityProfile, submitProfile, type OnChainProfile } from "../../lib/api";
import { readStoredMethod, readStoredVaultIdentity } from "../../lib/identitySession";
import { signDigestWithVaultIdentity, unlockVaultIdentity, WrongVaultPasswordError } from "../../lib/vault";
import styles from "./portfolio.module.css";

const NAME_MAX = 40;
const BIO_MAX = 280;

/** An optional public name and bio, written on-chain so anyone can read the
 * current text — and see it change — without trusting any server. Changing
 * it is a signature from the owner, so the password is asked for each time. */
export function ProfileEditor() {
  const [state, setState] = useState<{ enabled: boolean; active: boolean; profile: OnChainProfile | null } | null>(null);
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState("");
  const [bio, setBio] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    fetchIdentityProfile().then((result) => result.ok && setState(result.data));
  }, []);

  if (!state?.enabled || !state.active) return null;

  const usesPassword = readStoredMethod() === "vault";
  const vault = readStoredVaultIdentity();

  function open() {
    setName(state?.profile?.name ?? "");
    setBio(state?.profile?.bio ?? "");
    setPassword("");
    setError(null);
    setEditing(true);
  }

  async function save() {
    if (!vault) return;
    setError(null);
    setBusy(true);
    try {
      const challengeResult = await beginSetProfile(name.trim(), bio.trim());
      if (!challengeResult.ok) return setError(challengeResult.error);
      const challenge = challengeResult.data;
      const privateKey = await unlockVaultIdentity(vault.encryptedPrivateKey, password);
      const digest = hashTypedData({
        domain: challenge.domain,
        types: challenge.types,
        primaryType: challenge.primaryType,
        message: { ...challenge.message, nonce: BigInt(challenge.message.nonce), deadline: BigInt(challenge.message.deadline) },
      });
      const signature = await signDigestWithVaultIdentity(privateKey, digest);
      const result = await submitProfile(challenge, signature);
      if (!result.ok) return setError(result.error);
      setState({ ...state!, profile: result.data.profile });
      setEditing(false);
      setSaved(true);
      setTimeout(() => setSaved(false), 2500);
    } catch (cause) {
      setError(cause instanceof WrongVaultPasswordError ? cause.message : cause instanceof Error ? cause.message : "could not save");
    } finally {
      setBusy(false);
    }
  }

  const profile = state.profile;
  return (
    <div className={styles.card} data-testid="profile-editor">
      <div className={styles.cardTitleRow}>
        <div className={styles.cardTitle}>Public name and bio</div>
        {!editing && (
          <button type="button" className={styles.btnSecondary} onClick={open} data-testid="edit-profile-button">
            {profile ? "Edit" : "Add"}
          </button>
        )}
        {saved && <span className={styles.note}>Saved on-chain</span>}
      </div>

      {!editing && (
        <>
          {profile ? (
            <div data-testid="profile-current">
              {profile.name && <div className={styles.assetName}>{profile.name}</div>}
              {profile.bio && <p className={styles.note} style={{ margin: "6px 0 0" }}>{profile.bio}</p>}
            </div>
          ) : (
            <p className={styles.note} style={{ margin: 0 }}>Optional. Shown on your public page.</p>
          )}
        </>
      )}

      {editing && (
        <>
          <div className={styles.field}>
            <label className={styles.fieldLabel} htmlFor="profile-name">Name</label>
            <input id="profile-name" data-testid="profile-name-input" maxLength={NAME_MAX} value={name} onChange={(e) => setName(e.target.value)} />
          </div>
          <div className={styles.field}>
            <label className={styles.fieldLabel} htmlFor="profile-bio">Bio</label>
            <textarea id="profile-bio" data-testid="profile-bio-input" rows={3} maxLength={BIO_MAX} value={bio} onChange={(e) => setBio(e.target.value)} />
          </div>
          {usesPassword && vault ? (
            <div className={styles.field}>
              <label className={styles.fieldLabel} htmlFor="profile-password">Your password (to sign the change)</label>
              <input id="profile-password" data-testid="profile-password-input" type="password" value={password} onChange={(e) => setPassword(e.target.value)} />
            </div>
          ) : (
            <p className={styles.note}>Changing this needs a password identity — fingerprint/face sign-in can’t sign it yet.</p>
          )}
          <p className={styles.note}>
            This is written on a public blockchain: anyone can read it, and earlier versions stay visible in its history. Don’t put private information here.
          </p>
          {error && <div className={styles.errorBanner} role="alert">{error}</div>}
          <div style={{ display: "flex", gap: 8 }}>
            <button className={styles.btnPrimary} data-testid="save-profile-button" disabled={busy || !password || !usesPassword} onClick={save}>
              {busy ? "Saving on-chain — a few seconds…" : "Save"}
            </button>
            <button className={styles.btnSecondary} type="button" onClick={() => setEditing(false)} disabled={busy}>
              Cancel
            </button>
          </div>
        </>
      )}
    </div>
  );
}
