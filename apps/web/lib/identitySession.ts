// Which credential method (and, for the vault method, the encrypted
// vault itself) this browser last signed in with — shared by every
// page that needs to know, without re-asking, whether this browser
// already has an identity set up (onboarding, portfolio, disclose).
// Nothing stored here is more sensitive than what apps/api already
// holds: the vault entry is the same AES-GCM-encrypted blob, useless
// without the password. If this storage is cleared,
// that identity has no recovery path — there is
// no server-side backup to fall back to.
import { concat, getAddress, keccak256, slice } from "viem";
import type { VaultIdentity } from "./vault";

const IDENTITY_METHOD_KEY = "lz-identity-method";
const IDENTITY_VAULT_KEY = "lz-identity-vault";
const IDENTITY_VAULTS_KEY = "lz-identity-vaults";

/** The public address this password identity signs in as. */
export function vaultAddress(vault: VaultIdentity): `0x${string}` {
  return getAddress(slice(keccak256(concat([vault.qx, vault.qy])), 12));
}

const sameVault = (a: VaultIdentity, b: VaultIdentity) => a.qx === b.qx && a.qy === b.qy;

function parseVault(raw: string | null): VaultIdentity | null {
  if (!raw) return null;
  try {
    return JSON.parse(raw) as VaultIdentity;
  } catch {
    return null;
  }
}

/** Every password identity saved in this browser — creating another never
 * replaces the earlier ones. */
export function readStoredVaultIdentities(): VaultIdentity[] {
  let list: VaultIdentity[] = [];
  try {
    const parsed = JSON.parse(window.localStorage.getItem(IDENTITY_VAULTS_KEY) ?? "[]");
    if (Array.isArray(parsed)) list = parsed as VaultIdentity[];
  } catch {
    list = [];
  }
  const current = parseVault(window.localStorage.getItem(IDENTITY_VAULT_KEY));
  if (current && !list.some((v) => sameVault(v, current))) list.push(current);
  return list;
}

function writeVaults(list: VaultIdentity[]) {
  window.localStorage.setItem(IDENTITY_VAULTS_KEY, JSON.stringify(list));
}

export function persistIdentityMethod(method: "webauthn" | "vault", vault?: VaultIdentity) {
  window.localStorage.setItem(IDENTITY_METHOD_KEY, method);
  if (vault) {
    window.localStorage.setItem(IDENTITY_VAULT_KEY, JSON.stringify(vault));
    const list = readStoredVaultIdentities();
    if (!list.some((v) => sameVault(v, vault))) writeVaults([...list, vault]);
  }
}

export function readStoredMethod(): "webauthn" | "vault" | null {
  const value = window.localStorage.getItem(IDENTITY_METHOD_KEY);
  return value === "webauthn" || value === "vault" ? value : null;
}

/** The password identity used last on this browser. */
export function readStoredVaultIdentity(): VaultIdentity | null {
  return parseVault(window.localStorage.getItem(IDENTITY_VAULT_KEY)) ?? readStoredVaultIdentities()[0] ?? null;
}

/** Forgets one password identity saved on this device — the way forward when
 * its password is lost, since there is no server-side recovery. Others stay. */
export function forgetStoredVaultIdentity(vault: VaultIdentity) {
  writeVaults(readStoredVaultIdentities().filter((v) => !sameVault(v, vault)));
  const current = parseVault(window.localStorage.getItem(IDENTITY_VAULT_KEY));
  if (current && sameVault(current, vault)) {
    window.localStorage.removeItem(IDENTITY_VAULT_KEY);
    window.localStorage.removeItem(IDENTITY_METHOD_KEY);
  }
}
