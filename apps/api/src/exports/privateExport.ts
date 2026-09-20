import { createCipheriv, createDecipheriv, randomBytes, scryptSync } from "node:crypto";

/** The private artifact: everything needed to recompute and produce new
 * proofs (witness, raw private series, salts, credential references) —
 * per commitments.md, "outro artefato cifrado, opcional, que permite
 * novos cálculos e proving." Never published alongside the public
 * bundle. */
export interface PrivateExportPayload {
  witness: Record<string, unknown>;
  rawPrivateSeries: Record<string, unknown>[];
  privateSalts: string[];
}

export interface EncryptedExport {
  ciphertext: string;
  iv: string;
  authTag: string;
  salt: string;
}

const SCRYPT_KEY_LENGTH = 32;

function deriveKey(passphrase: string, salt: Buffer): Buffer {
  return scryptSync(passphrase, salt, SCRYPT_KEY_LENGTH);
}

/** AES-256-GCM with a per-export random salt/IV and a scrypt-derived
 * key — the passphrase never touches disk, only the derived key does,
 * transiently. The result is portable: `decryptPrivateExport` with the
 * same passphrase reconstructs the exact original payload anywhere. */
export function encryptPrivateExport(payload: PrivateExportPayload, passphrase: string): EncryptedExport {
  const salt = randomBytes(16);
  const iv = randomBytes(12);
  const key = deriveKey(passphrase, salt);
  const cipher = createCipheriv("aes-256-gcm", key, iv);
  const plaintext = Buffer.from(JSON.stringify(payload), "utf8");
  const ciphertext = Buffer.concat([cipher.update(plaintext), cipher.final()]);
  const authTag = cipher.getAuthTag();

  return {
    ciphertext: ciphertext.toString("base64"),
    iv: iv.toString("base64"),
    authTag: authTag.toString("base64"),
    salt: salt.toString("base64"),
  };
}

/** Throws (via AES-GCM's authentication tag check) if `passphrase` is
 * wrong or `encrypted` was tampered with — never silently returns
 * corrupted or partial data. */
export function decryptPrivateExport(encrypted: EncryptedExport, passphrase: string): PrivateExportPayload {
  const salt = Buffer.from(encrypted.salt, "base64");
  const iv = Buffer.from(encrypted.iv, "base64");
  const key = deriveKey(passphrase, salt);
  const decipher = createDecipheriv("aes-256-gcm", key, iv);
  decipher.setAuthTag(Buffer.from(encrypted.authTag, "base64"));
  const plaintext = Buffer.concat([decipher.update(Buffer.from(encrypted.ciphertext, "base64")), decipher.final()]);
  return JSON.parse(plaintext.toString("utf8")) as PrivateExportPayload;
}
