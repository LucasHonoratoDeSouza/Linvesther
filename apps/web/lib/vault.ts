// Client-side vault: a P-256 identity generated and used entirely
// locally via SubtleCrypto. The private key is only ever held in
// plaintext inside a non-extractable-after-import CryptoKey; what
// leaves this module is the public key and an AES-GCM encrypted blob —
// never the password, never the plaintext private key.

const PBKDF2_ITERATIONS = 600_000;

export class WrongVaultPasswordError extends Error {
  constructor() {
    super("Incorrect password — this vault could not be unlocked.");
  }
}

export interface EncryptedVaultKey {
  ciphertext: string; // base64
  iv: string; // base64
  salt: string; // base64
  iterations: number;
}

export interface VaultIdentity {
  qx: `0x${string}`;
  qy: `0x${string}`;
  encryptedPrivateKey: EncryptedVaultKey;
}

function toBase64(bytes: ArrayBuffer): string {
  const view = new Uint8Array(bytes);
  let binary = "";
  for (const byte of view) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary);
}

function fromBase64(value: string): Uint8Array<ArrayBuffer> {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

function bytesToHex(bytes: Uint8Array<ArrayBuffer>): `0x${string}` {
  return `0x${Array.from(bytes)
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("")}`;
}

async function deriveAesKey(password: string, salt: Uint8Array<ArrayBuffer>, iterations: number): Promise<CryptoKey> {
  const passwordKey = await crypto.subtle.importKey("raw", new TextEncoder().encode(password), "PBKDF2", false, [
    "deriveKey",
  ]);
  return crypto.subtle.deriveKey(
    { name: "PBKDF2", salt, iterations, hash: "SHA-256" },
    passwordKey,
    { name: "AES-GCM", length: 256 },
    false,
    ["encrypt", "decrypt"],
  );
}

/** Generates a new P-256 identity locally and encrypts its private key
 * with a PBKDF2-derived key (>=600k iterations, SHA-256, per
 * under AES-GCM. Only the public key coordinates and the resulting
 * encrypted blob are returned — the password and the plaintext private
 * key never leave this function. */
export async function createVaultIdentity(password: string): Promise<VaultIdentity> {
  const keyPair = await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, ["sign", "verify"]);
  const rawPublicKey = new Uint8Array(await crypto.subtle.exportKey("raw", keyPair.publicKey));
  const qx = bytesToHex(rawPublicKey.slice(1, 33));
  const qy = bytesToHex(rawPublicKey.slice(33, 65));

  const pkcs8PrivateKey = await crypto.subtle.exportKey("pkcs8", keyPair.privateKey);

  const salt = crypto.getRandomValues(new Uint8Array(16));
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const aesKey = await deriveAesKey(password, salt, PBKDF2_ITERATIONS);
  const ciphertext = await crypto.subtle.encrypt({ name: "AES-GCM", iv }, aesKey, pkcs8PrivateKey);

  return {
    qx,
    qy,
    encryptedPrivateKey: {
      ciphertext: toBase64(ciphertext),
      iv: toBase64(iv.buffer),
      salt: toBase64(salt.buffer),
      iterations: PBKDF2_ITERATIONS,
    },
  };
}

/** Decrypts `encrypted` with `password`, returning a signing key. A
 * wrong password fails AES-GCM's authentication tag check and throws
 * `WrongVaultPasswordError` — a detectable failure, never a
 * silently wrong key that goes on to produce an unexplained invalid
 * signature. */
export async function unlockVaultIdentity(encrypted: EncryptedVaultKey, password: string): Promise<CryptoKey> {
  const salt = fromBase64(encrypted.salt);
  const aesKey = await deriveAesKey(password, salt, encrypted.iterations);

  let pkcs8PrivateKey: ArrayBuffer;
  try {
    pkcs8PrivateKey = await crypto.subtle.decrypt(
      { name: "AES-GCM", iv: fromBase64(encrypted.iv) },
      aesKey,
      fromBase64(encrypted.ciphertext),
    );
  } catch {
    throw new WrongVaultPasswordError();
  }

  return crypto.subtle.importKey("pkcs8", pkcs8PrivateKey, { name: "ECDSA", namedCurve: "P-256" }, false, ["sign"]);
}

export class InvalidVaultBackupError extends Error {
  constructor() {
    super("This file isn't a valid identity backup.");
  }
}

/** A vault identity is already the exact shape a backup file needs —
 * this just names the JSON serialization so a device that never saw
 * the original `createVaultIdentity` call can still unlock the same
 * identity, given the file and the password. The file carries no more
 * than what already sits in this browser's own storage: the encrypted
 * blob is still useless without the password. */
export function serializeVaultBackup(vault: VaultIdentity): string {
  return JSON.stringify(vault, null, 2);
}

export function parseVaultBackup(json: string): VaultIdentity {
  let parsed: unknown;
  try {
    parsed = JSON.parse(json);
  } catch {
    throw new InvalidVaultBackupError();
  }
  const candidate = parsed as Partial<VaultIdentity> | null;
  if (
    typeof candidate !== "object" ||
    candidate === null ||
    typeof candidate.qx !== "string" ||
    typeof candidate.qy !== "string" ||
    typeof candidate.encryptedPrivateKey?.ciphertext !== "string" ||
    typeof candidate.encryptedPrivateKey?.iv !== "string" ||
    typeof candidate.encryptedPrivateKey?.salt !== "string" ||
    typeof candidate.encryptedPrivateKey?.iterations !== "number"
  ) {
    throw new InvalidVaultBackupError();
  }
  return candidate as VaultIdentity;
}

/** Signs `message` (a login challenge or a claim-set digest) with an
 * unlocked vault key — the same ECDSA/SHA-256 raw `r||s` wire format
 * apps/api's vault.ts expects for off-chain verification.
 * For a signature that must also verify on-chain (an EIP-712 command),
 * use `signDigestWithVaultIdentity` instead — the wire format alone
 * isn't enough there, see its own doc comment. */
export async function signWithVaultIdentity(privateKey: CryptoKey, message: string): Promise<`0x${string}`> {
  const signature = await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, privateKey, new TextEncoder().encode(message));
  return bytesToHex(new Uint8Array(signature));
}

// secp256r1 (P-256) curve order — P256.sol's own `N` constant.
const P256_N = 0xffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551n;

/** Signs a raw 32-byte EIP-712 digest (hex-encoded) with an unlocked
 * vault key, for a command that will be verified on-chain by
 * `P256VaultAccount.isValidSignature`. Two things a plain
 * `signWithVaultIdentity` call would get wrong here:
 *
 * 1. `digest` is signed as its raw bytes, not the UTF-8 text of the hex
 *    string — a digest is binary data, not a message to encode.
 * 2. SubtleCrypto's ECDSA signing gives no way to skip its own internal
 *    SHA-256 hash of the input, so what's actually signed is
 *    `sha256(digest)` — `P256VaultAccount`'s `_rawSignatureValidation`
 *    override re-hashes for exactly this reason before verifying.
 *
 * `s` is also normalized to its canonical low form (`min(s, N-s)`):
 * SubtleCrypto doesn't guarantee this, but `P256.sol`'s
 * `_isProperSignature` requires it (rejects `s > N/2` as the standard
 * malleability guard) — about half of otherwise-valid signatures would
 * fail on-chain without this. */
export async function signDigestWithVaultIdentity(privateKey: CryptoKey, digest: `0x${string}`): Promise<`0x${string}`> {
  const signature = new Uint8Array(await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, privateKey, fromHex(digest)));
  const r = signature.slice(0, 32);
  let s = BigInt(bytesToHex(signature.slice(32, 64)));
  if (s > P256_N / 2n) {
    s = P256_N - s;
  }
  return `0x${bytesToHex(r).slice(2)}${s.toString(16).padStart(64, "0")}`;
}

function fromHex(hex: `0x${string}`): Uint8Array<ArrayBuffer> {
  const clean = hex.slice(2);
  const bytes = new Uint8Array(clean.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = Number.parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}
