import { MemoryNonceStore, type NonceStore } from "./nonceStore.js";

const nonceStore: NonceStore = new MemoryNonceStore();

function hexToBytes(hex: `0x${string}`): Uint8Array<ArrayBuffer> {
  const clean = hex.slice(2);
  const bytes = new Uint8Array(clean.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = Number.parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}

/** Issues a fresh, single-use login challenge — same "issue once, consume
 * once" nonce semantics as the SIWE flow this replaces. */
export function beginChallenge(): string {
  return nonceStore.issue();
}

/** Verifies a raw P-256 signature (ECDSA/SHA-256, IEEE P1363 `r||s`, the
 * same wire format `P256VaultAccount.isValidSignature` expects on-chain)
 * over `message`, with no nonce bookkeeping — for callers that already
 * have their own replay/freshness guarantee over `message` (e.g.
 * claims/disclosure.ts's ClaimSet digest, which is only ever accepted
 * once per digest since publishing is a one-time, immutably-stored
 * action, not a repeatable login). */
export async function verifyRawSignature(
  qx: `0x${string}`,
  qy: `0x${string}`,
  message: string,
  signature: `0x${string}`,
): Promise<boolean> {
  const rawPublicKey = new Uint8Array(65);
  rawPublicKey[0] = 0x04; // uncompressed point prefix
  rawPublicKey.set(hexToBytes(qx), 1);
  rawPublicKey.set(hexToBytes(qy), 33);

  const key = await crypto.subtle.importKey("raw", rawPublicKey, { name: "ECDSA", namedCurve: "P-256" }, false, [
    "verify",
  ]);

  return crypto.subtle.verify(
    { name: "ECDSA", hash: "SHA-256" },
    key,
    hexToBytes(signature),
    new TextEncoder().encode(message),
  );
}

/** Verifies a raw P-256 signature over `challenge`, consuming the
 * challenge first — a second verification attempt with the same
 * challenge is a replay and fails even if the signature itself is
 * valid, and an unknown/never-issued challenge fails the same way. Used
 * for login, where each attempt must be tied to a fresh, single-use
 * challenge (see `verifyRawSignature` for signature verification
 * without that login-specific replay bookkeeping). */
export async function verifySignature(
  qx: `0x${string}`,
  qy: `0x${string}`,
  challenge: string,
  signature: `0x${string}`,
): Promise<boolean> {
  if (!nonceStore.consume(challenge)) {
    return false;
  }
  return verifyRawSignature(qx, qy, challenge, signature);
}
