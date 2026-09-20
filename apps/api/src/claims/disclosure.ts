import { verify as verifyIdentitySignature, type IdentityCredentialProof } from "../auth/identitySignature.js";
import { claimSetDigest } from "./claimSet.js";
import type { ClaimSet, DisclosureError } from "./types.js";

export type ConsentResult = { ok: true; digest: string } | { ok: false; error: "expired" | "signature_invalid" };

/** Expiration check, shared by publish-time consent verification and a
 * later read of an already-published record. A credential proof
 * (WebAuthn assertion or vault signature) is single-use — its challenge
 * is consumed the moment it's verified — so unlike the EOA signature
 * this replaces, it cannot be re-verified on every read; a read instead
 * trusts the record's presence (proven valid once, at publish time) and
 * only re-checks whether it has since expired. */
export function checkExpiry(claimSet: ClaimSet, now: Date): "expired" | null {
  return now.getTime() >= new Date(claimSet.expiresAt).getTime() ? "expired" : null;
}

/** Validates, once, that `credential` covers the *exact* ClaimSet
 * presented — consent is over the exact content: changing
 * `checkpointId`, the period, a claim, or the audience after signing
 * produces a different digest, which the original credential proof then
 * fails to cover. Dispatches to identitySignature.ts (WebAuthn or
 * vault) rather than checking a raw secp256k1 signature, so the same
 * credential a user registered with at onboarding also authorizes
 * disclosure. */
export async function verifyConsent(claimSet: ClaimSet, credential: IdentityCredentialProof, subjectKey: `0x${string}`, now: Date): Promise<ConsentResult> {
  const expiryError = checkExpiry(claimSet, now);
  if (expiryError) {
    return { ok: false, error: expiryError };
  }
  const digest = claimSetDigest(claimSet);
  const valid = await verifyIdentitySignature(subjectKey, digest, credential);
  if (!valid) {
    return { ok: false, error: "signature_invalid" };
  }
  return { ok: true, digest };
}

/** Read-time access check: `"public"` is reusable by any requester;
 * anything else must match exactly. */
export function checkAudience(claimSet: ClaimSet, requesterAudience: string): DisclosureError | null {
  if (claimSet.audience !== "public" && claimSet.audience !== requesterAudience) {
    return "audience_mismatch";
  }
  return null;
}
