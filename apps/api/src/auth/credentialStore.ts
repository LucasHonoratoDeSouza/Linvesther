import type { Pool } from "pg";

export type CredentialMethod = "webauthn" | "vault";

export interface Credential {
  subjectKey: `0x${string}`;
  method: CredentialMethod;
  qx: `0x${string}`;
  qy: `0x${string}`;
  /** WebAuthn signature counter; unused (stays 0) for the vault method,
   * which has no authenticator-maintained counter to check. */
  counter: number;
}

export interface CredentialStore {
  register(subjectKey: `0x${string}`, qx: `0x${string}`, qy: `0x${string}`, method: CredentialMethod): Promise<void>;
  get(subjectKey: `0x${string}`): Promise<Credential | undefined>;
}

/** In-memory credential store: one P-256 public key per `subjectKey`
 * (`last20Bytes(keccak256(qx || qy))`, computed by the caller — this
 * store treats it as an opaque key), registered once via a passkey or
 * vault ceremony and looked up by that same key on every subsequent
 * login/signature check. */
export class MemoryCredentialStore implements CredentialStore {
  private readonly credentials = new Map<`0x${string}`, Credential>();

  async register(subjectKey: `0x${string}`, qx: `0x${string}`, qy: `0x${string}`, method: CredentialMethod): Promise<void> {
    this.credentials.set(subjectKey, { subjectKey, method, qx, qy, counter: 0 });
  }

  async get(subjectKey: `0x${string}`): Promise<Credential | undefined> {
    return this.credentials.get(subjectKey);
  }
}

/** A passkey as the WebAuthn library needs it to check a later assertion:
 * its COSE public key and signature counter, found by credential ID. */
export interface WebAuthnCredential {
  publicKeyCose: Uint8Array<ArrayBuffer>;
  counter: number;
}

export interface WebAuthnCredentialStore {
  save(credentialId: string, credential: WebAuthnCredential): Promise<void>;
  get(credentialId: string): Promise<WebAuthnCredential | undefined>;
  updateCounter(credentialId: string, counter: number): Promise<void>;
}

export class MemoryWebAuthnCredentialStore implements WebAuthnCredentialStore {
  private readonly credentials = new Map<string, WebAuthnCredential>();

  async save(credentialId: string, credential: WebAuthnCredential): Promise<void> {
    this.credentials.set(credentialId, { ...credential });
  }

  async get(credentialId: string): Promise<WebAuthnCredential | undefined> {
    const stored = this.credentials.get(credentialId);
    return stored ? { ...stored } : undefined;
  }

  async updateCounter(credentialId: string, counter: number): Promise<void> {
    const stored = this.credentials.get(credentialId);
    if (stored) stored.counter = counter;
  }
}

/** Registered identities survive a restart: without this, everyone who signed
 * up could not sign in again after the API restarted. */
export class PostgresCredentialStore implements CredentialStore {
  constructor(private readonly pool: Pool) {}

  async ensureSchema(): Promise<void> {
    await this.pool.query(`
      CREATE TABLE IF NOT EXISTS api_credentials (
        subject_key TEXT PRIMARY KEY,
        method TEXT NOT NULL,
        qx TEXT NOT NULL,
        qy TEXT NOT NULL,
        counter BIGINT NOT NULL DEFAULT 0
      )`);
  }

  async register(subjectKey: `0x${string}`, qx: `0x${string}`, qy: `0x${string}`, method: CredentialMethod): Promise<void> {
    // The first registration of a key wins: a later request must not swap the method or key behind an identity.
    await this.pool.query(
      "INSERT INTO api_credentials (subject_key, method, qx, qy) VALUES ($1, $2, $3, $4) ON CONFLICT (subject_key) DO NOTHING",
      [subjectKey, method, qx, qy],
    );
  }

  async get(subjectKey: `0x${string}`): Promise<Credential | undefined> {
    const result = await this.pool.query<{ subject_key: `0x${string}`; method: CredentialMethod; qx: `0x${string}`; qy: `0x${string}`; counter: string }>(
      "SELECT subject_key, method, qx, qy, counter FROM api_credentials WHERE subject_key = $1",
      [subjectKey],
    );
    const row = result.rows[0];
    return row ? { subjectKey: row.subject_key, method: row.method, qx: row.qx, qy: row.qy, counter: Number(row.counter) } : undefined;
  }
}

/** Passkeys survive a restart, with the signature counter that lets the
 * library notice a cloned authenticator. */
export class PostgresWebAuthnCredentialStore implements WebAuthnCredentialStore {
  constructor(private readonly pool: Pool) {}

  async ensureSchema(): Promise<void> {
    await this.pool.query(`
      CREATE TABLE IF NOT EXISTS api_webauthn_credentials (
        credential_id TEXT PRIMARY KEY,
        public_key_cose BYTEA NOT NULL,
        counter BIGINT NOT NULL
      )`);
  }

  async save(credentialId: string, credential: WebAuthnCredential): Promise<void> {
    await this.pool.query(
      "INSERT INTO api_webauthn_credentials (credential_id, public_key_cose, counter) VALUES ($1, $2, $3) ON CONFLICT (credential_id) DO NOTHING",
      [credentialId, Buffer.from(credential.publicKeyCose), credential.counter],
    );
  }

  async get(credentialId: string): Promise<WebAuthnCredential | undefined> {
    const result = await this.pool.query<{ public_key_cose: Buffer; counter: string }>(
      "SELECT public_key_cose, counter FROM api_webauthn_credentials WHERE credential_id = $1",
      [credentialId],
    );
    const row = result.rows[0];
    if (!row) return undefined;
    const publicKeyCose = new Uint8Array(new ArrayBuffer(row.public_key_cose.length));
    publicKeyCose.set(row.public_key_cose);
    return { publicKeyCose, counter: Number(row.counter) };
  }

  async updateCounter(credentialId: string, counter: number): Promise<void> {
    await this.pool.query("UPDATE api_webauthn_credentials SET counter = $2 WHERE credential_id = $1", [credentialId, counter]);
  }
}
