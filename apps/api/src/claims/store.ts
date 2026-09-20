import type { Pool } from "pg";
import type { IdentityCredentialProof } from "../auth/identitySignature.js";
import type { ClaimSet } from "./types.js";

export interface DisclosureRecord {
  claimSet: ClaimSet;
  credential: IdentityCredentialProof;
  owner: `0x${string}`;
  publishedAt?: string;
}

export interface DisclosureStore {
  save(digest: string, record: DisclosureRecord): Promise<void>;
  listByOwner(owner: string): Promise<{ digest: string; record: DisclosureRecord }[]>;
  get(digest: string): Promise<DisclosureRecord | undefined>;
}

export class MemoryDisclosureStore implements DisclosureStore {
  private readonly records = new Map<string, DisclosureRecord>();

  async save(digest: string, record: DisclosureRecord): Promise<void> {
    this.records.set(digest, record);
  }

  async listByOwner(owner: string): Promise<{ digest: string; record: DisclosureRecord }[]> {
    return [...this.records.entries()]
      .filter(([, record]) => record.owner.toLowerCase() === owner.toLowerCase())
      .map(([digest, record]) => ({ digest, record }));
  }

  async get(digest: string): Promise<DisclosureRecord | undefined> {
    return this.records.get(digest);
  }
}

/** Published claims outlive a restart: a link someone shared must keep working until it expires. */
export class PostgresDisclosureStore implements DisclosureStore {
  constructor(private readonly pool: Pool) {}

  async ensureSchema(): Promise<void> {
    await this.pool.query(`
      CREATE TABLE IF NOT EXISTS api_disclosures (
        digest TEXT PRIMARY KEY,
        owner TEXT NOT NULL,
        claim_set JSONB NOT NULL,
        credential JSONB NOT NULL,
        published_at TIMESTAMPTZ NOT NULL
      )`);
    await this.pool.query("CREATE INDEX IF NOT EXISTS api_disclosures_owner ON api_disclosures ((lower(owner)))");
  }

  async save(digest: string, record: DisclosureRecord): Promise<void> {
    // A digest is the hash of the whole claim, so saving it twice can only mean the same claim.
    await this.pool.query(
      `INSERT INTO api_disclosures (digest, owner, claim_set, credential, published_at)
       VALUES ($1, $2, $3, $4, $5) ON CONFLICT (digest) DO NOTHING`,
      [digest, record.owner, JSON.stringify(record.claimSet), JSON.stringify(record.credential), record.publishedAt ?? new Date().toISOString()],
    );
  }

  async listByOwner(owner: string): Promise<{ digest: string; record: DisclosureRecord }[]> {
    const result = await this.pool.query<Row>("SELECT digest, owner, claim_set, credential, published_at FROM api_disclosures WHERE lower(owner) = lower($1) ORDER BY published_at", [owner]);
    return result.rows.map((row) => ({ digest: row.digest, record: toRecord(row) }));
  }

  async get(digest: string): Promise<DisclosureRecord | undefined> {
    const result = await this.pool.query<Row>("SELECT digest, owner, claim_set, credential, published_at FROM api_disclosures WHERE digest = $1", [digest]);
    return result.rows[0] ? toRecord(result.rows[0]) : undefined;
  }
}

interface Row {
  digest: string;
  owner: `0x${string}`;
  claim_set: ClaimSet;
  credential: IdentityCredentialProof;
  published_at: Date;
}

function toRecord(row: Row): DisclosureRecord {
  return { claimSet: row.claim_set, credential: row.credential, owner: row.owner, publishedAt: row.published_at.toISOString() };
}
