import {
  beginPasskeyLogin,
  type ClaimSet,
  type IdentityCredentialProof,
} from "./api";
import { readStoredMethod, readStoredVaultIdentity } from "./identitySession";
import { signWithVaultIdentity, unlockVaultIdentity } from "./vault";
import { authenticatePasskey } from "./webauthn";

// The browser and API canonicalize the same claim fields before hashing.
function canonicalJson(value: unknown): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  const record = value as Record<string, unknown>;
  return `{${Object.keys(record)
    .sort()
    .map((key) => `${JSON.stringify(key)}:${canonicalJson(record[key])}`)
    .join(",")}}`;
}

async function claimSetDigestHex(claimSet: ClaimSet): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(canonicalJson(claimSet)),
  );
  return Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

export const usesPassword = () => readStoredMethod() === "vault";

/** Authorizes exactly this claim set with the owner's own credential:
 * their password (vault) or their device (passkey). */
export async function consentToClaimSet(
  claimSet: ClaimSet,
  password: string,
): Promise<IdentityCredentialProof> {
  const method = readStoredMethod();
  if (!method) throw new Error("Sign in from Your identity first.");
  if (method === "vault") {
    const stored = readStoredVaultIdentity();
    if (!stored)
      throw new Error(
        "This browser has no password identity stored — sign in again from Your identity.",
      );
    if (!password)
      throw new Error("Enter your password to authorize this claim.");
    const privateKey = await unlockVaultIdentity(
      stored.encryptedPrivateKey,
      password,
    );
    return {
      method: "vault",
      signature: await signWithVaultIdentity(
        privateKey,
        await claimSetDigestHex(claimSet),
      ),
    };
  }
  const options = await beginPasskeyLogin();
  if (!options.ok)
    throw new Error(`Could not start passkey verification: ${options.error}`);
  return {
    method: "webauthn",
    response: await authenticatePasskey(options.data),
  };
}
