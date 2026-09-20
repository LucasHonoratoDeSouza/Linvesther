import type { AuthenticationResponseJSON } from "@simplewebauthn/server";
import { MemoryCredentialStore, type CredentialStore } from "./credentialStore.js";
import { verifyRawSignature as verifyVaultSignature } from "./vault.js";
import { finishAuthentication as finishWebAuthnAuthentication } from "./webauthn.js";

/** The registered-credential store both the login handshake and
 * claims/disclosure's consent check read from — one shared instance, the
 * same way vault.ts/webauthn.ts each own their nonce store. */
export let credentialStore: CredentialStore = new MemoryCredentialStore();

/** Chooses where registered identities are kept. In memory by default; a
 * deployment with a database passes a durable store. */
export function useCredentialStore(store: CredentialStore): void {
  credentialStore = store;
}

export type IdentityCredentialProof =
  | { method: "vault"; signature: `0x${string}` }
  | { method: "webauthn"; response: AuthenticationResponseJSON };

/** Verifies that `proof` authenticates `subjectKey` over `digest`,
 * dispatching to the vault or WebAuthn verifier per the method the
 * credential was registered under. An unregistered `subjectKey`, or a
 * proof whose method doesn't match the one on record, both fail closed
 * (return `false`) rather than throwing — the same "invalid signature"
 * outcome callers already get from a wrong signature itself.
 *
 * For the WebAuthn method, challenge freshness is verified internally by
 * webauthn.ts against the challenge it issued for that ceremony —
 * `digest` is not separately re-checked on that branch. */
export async function verify(subjectKey: `0x${string}`, digest: string, proof: IdentityCredentialProof): Promise<boolean> {
  const credential = await credentialStore.get(subjectKey);
  if (!credential || credential.method !== proof.method) {
    return false;
  }

  if (proof.method === "vault") {
    return verifyVaultSignature(credential.qx, credential.qy, digest, proof.signature);
  }

  const result = await finishWebAuthnAuthentication(proof.response);
  return result.verified && result.qx === credential.qx && result.qy === credential.qy;
}
