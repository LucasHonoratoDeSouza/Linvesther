// Read-only API tokens: the credential an outside agent presents to read
// its owner's account, and nothing else.
//
// Deliberately not the browser session. A session cookie authorizes
// every write this API has (connecting an exchange, renaming, removing,
// relaying a transaction); a token here authorizes reads of one
// identity's own figures and can be thrown away at any moment without
// touching the person's sign-in. The two credentials are checked by
// different code (`requireSession` reads only the cookie, and
// `requireReadOnlyToken` reads only the `Authorization` header), so
// neither can stand in for the other.

import { createHash, randomBytes, randomUUID, timingSafeEqual } from "node:crypto";
import type { Pool } from "pg";

/** The only scope a token here can carry. Whether one identity should be
 * able to hand out a token for some of its connected accounts and not
 * others is a real question, but a token that reads everything is what
 * exists: the scope is stored and checked so a narrower one can be added
 * without a second credential format, never so callers can assume one. */
export const READ_ONLY_SCOPE = "read:account";
export type ReadOnlyScope = typeof READ_ONLY_SCOPE;

/** Distinguishes this credential from a session id at a glance — in a
 * log, a shell history or a support conversation — so one is never
 * pasted where the other belongs. */
const TOKEN_PREFIX = "lvz_ro_";

/** 32 random bytes: the same strength as a session id. */
const TOKEN_BYTES = 32;

/** The longest text worth hashing as a candidate token. A real one is
 * `TOKEN_PREFIX` plus 64 hex characters; anything longer is not a token
 * this service issued, so it is refused before any work is done on it. */
export const MAX_TOKEN_LENGTH = TOKEN_PREFIX.length + TOKEN_BYTES * 2;

/** How many tokens one identity may hold at once. A bound on the table,
 * not a security control: a person with a legitimate need for more than
 * this has a different problem than an attacker does. */
export const MAX_ACTIVE_TOKENS = 20;

/** What a person can see about a token they hold. Never the token. */
export interface ReadOnlyTokenRecord {
  /** Public name for this token — what a revoke addresses. Unrelated to
   * the secret, so it is safe to show and to log. */
  id: string;
  address: `0x${string}`;
  /** The person's own name for it ("Hermes", "laptop agent"). */
  label: string;
  scope: ReadOnlyScope;
  createdAt: string;
  /** When it last authorized a read; `null` while it never has. */
  lastUsedAt: string | null;
  revokedAt: string | null;
  /** Set only when a caller asks for one: a token does not expire on its
   * own. Checked on every use regardless, so an expiring token can be
   * issued without a second code path to trust later. */
  expiresAt: string | null;
}

/** Only ever `false` together with a reason, so a caller cannot mistake
 * "revoked" for "never existed" when it matters. */
export type TokenRefusal = "unknown" | "revoked" | "expired";

export interface ReadOnlyTokenStore {
  /** The token itself is returned exactly once, here. Nothing stores it,
   * so a lost token is replaced, never recovered. */
  issue(
    address: `0x${string}`,
    label: string,
    options?: { expiresAt?: Date; now?: Date },
  ): Promise<{ token: string; record: ReadOnlyTokenRecord }>;
  /** The token behind `presented`, or why it is refused. Records the use
   * on success, which is what the listing's "last used" shows. */
  authenticate(presented: string, now: Date): Promise<ReadOnlyTokenRecord | TokenRefusal>;
  /** Every token of `address`, newest first, revoked ones included. */
  list(address: `0x${string}`): Promise<ReadOnlyTokenRecord[]>;
  /** `false` when `address` holds no live token with that id — including
   * when the id belongs to somebody else, which is indistinguishable
   * from not existing. */
  revoke(address: `0x${string}`, id: string, now: Date): Promise<boolean>;
}

/** Raised rather than silently issuing token 21, so the caller answers
 * with a real refusal. */
export class TooManyReadOnlyTokensError extends Error {
  constructor() {
    super(`an identity may hold at most ${MAX_ACTIVE_TOKENS} read-only tokens at once`);
    this.name = "TooManyReadOnlyTokensError";
  }
}

/** What is kept for a token: a hash, so reading the table gives an
 * attacker nothing they could present. Same treatment as a session id
 * (see `sessionStore.ts`). */
export function readOnlyTokenDigest(token: string): string {
  return createHash("sha256").update(token).digest("hex");
}

export function generateReadOnlyToken(): string {
  return `${TOKEN_PREFIX}${randomBytes(TOKEN_BYTES).toString("hex")}`;
}

/** Everything that is shaped like a token this service issues. Anything
 * else is refused without being hashed or looked up. */
export function looksLikeReadOnlyToken(candidate: string): boolean {
  return candidate.length === MAX_TOKEN_LENGTH && candidate.startsWith(TOKEN_PREFIX) && /^[0-9a-f]+$/.test(candidate.slice(TOKEN_PREFIX.length));
}

/** Compares two digests without leaking, through how long the comparison
 * takes, how much of a wrong one was right. */
function sameDigest(a: string, b: string): boolean {
  const left = Buffer.from(a, "utf8");
  const right = Buffer.from(b, "utf8");
  return left.length === right.length && timingSafeEqual(left, right);
}

function refusalFor(record: { revokedAt: string | null; expiresAt: string | null }, now: Date): TokenRefusal | null {
  if (record.revokedAt !== null) return "revoked";
  if (record.expiresAt !== null && new Date(record.expiresAt).getTime() <= now.getTime()) return "expired";
  return null;
}

interface StoredToken extends ReadOnlyTokenRecord {
  digest: string;
}

export class MemoryReadOnlyTokenStore implements ReadOnlyTokenStore {
  private readonly tokens: StoredToken[] = [];

  constructor(private readonly now: () => Date = () => new Date()) {}

  async issue(
    address: `0x${string}`,
    label: string,
    options: { expiresAt?: Date; now?: Date } = {},
  ): Promise<{ token: string; record: ReadOnlyTokenRecord }> {
    const at = options.now ?? this.now();
    const live = this.tokens.filter((t) => t.address.toLowerCase() === address.toLowerCase() && refusalFor(t, at) === null);
    if (live.length >= MAX_ACTIVE_TOKENS) throw new TooManyReadOnlyTokensError();
    const token = generateReadOnlyToken();
    const record: StoredToken = {
      digest: readOnlyTokenDigest(token),
      id: randomUUID(),
      address,
      label,
      scope: READ_ONLY_SCOPE,
      createdAt: at.toISOString(),
      lastUsedAt: null,
      revokedAt: null,
      expiresAt: options.expiresAt?.toISOString() ?? null,
    };
    this.tokens.push(record);
    return { token, record: visible(record) };
  }

  async authenticate(presented: string, now: Date): Promise<ReadOnlyTokenRecord | TokenRefusal> {
    if (!looksLikeReadOnlyToken(presented)) return "unknown";
    const digest = readOnlyTokenDigest(presented);
    const found = this.tokens.find((t) => sameDigest(t.digest, digest));
    if (!found) return "unknown";
    const refusal = refusalFor(found, now);
    if (refusal) return refusal;
    found.lastUsedAt = now.toISOString();
    return visible(found);
  }

  async list(address: `0x${string}`): Promise<ReadOnlyTokenRecord[]> {
    return this.tokens
      .filter((t) => t.address.toLowerCase() === address.toLowerCase())
      .map(visible)
      .sort((a, b) => b.createdAt.localeCompare(a.createdAt));
  }

  async revoke(address: `0x${string}`, id: string, now: Date): Promise<boolean> {
    const found = this.tokens.find((t) => t.id === id && t.address.toLowerCase() === address.toLowerCase() && t.revokedAt === null);
    if (!found) return false;
    found.revokedAt = now.toISOString();
    return true;
  }
}

function visible(record: StoredToken): ReadOnlyTokenRecord {
  const { digest: _digest, ...rest } = record;
  return { ...rest };
}

interface TokenRow {
  id: string;
  address: `0x${string}`;
  label: string;
  scope: string;
  created_at: Date;
  last_used_at: Date | null;
  revoked_at: Date | null;
  expires_at: Date | null;
}

const fromRow = (row: TokenRow): ReadOnlyTokenRecord => ({
  id: row.id,
  address: row.address,
  label: row.label,
  // A row written by a later version with a scope this one does not know
  // would be a silent widening of what a token may do, so the scope is
  // read back as it was stored rather than assumed.
  scope: row.scope as ReadOnlyScope,
  createdAt: row.created_at.toISOString(),
  lastUsedAt: row.last_used_at?.toISOString() ?? null,
  revokedAt: row.revoked_at?.toISOString() ?? null,
  expiresAt: row.expires_at?.toISOString() ?? null,
});

const COLUMNS = "id, address, label, scope, created_at, last_used_at, revoked_at, expires_at";

/** Tokens outlive a restart of the API — an agent configured with one
 * must not silently stop working because this process was redeployed. */
export class PostgresReadOnlyTokenStore implements ReadOnlyTokenStore {
  constructor(
    private readonly pool: Pool,
    private readonly now: () => Date = () => new Date(),
  ) {}

  async ensureSchema(): Promise<void> {
    await this.pool.query(`
      CREATE TABLE IF NOT EXISTS api_read_only_tokens (
        id TEXT PRIMARY KEY,
        token_digest TEXT NOT NULL UNIQUE,
        address TEXT NOT NULL,
        label TEXT NOT NULL,
        scope TEXT NOT NULL,
        created_at TIMESTAMPTZ NOT NULL,
        last_used_at TIMESTAMPTZ,
        revoked_at TIMESTAMPTZ,
        expires_at TIMESTAMPTZ
      )`);
    await this.pool.query("CREATE INDEX IF NOT EXISTS api_read_only_tokens_address ON api_read_only_tokens (address)");
  }

  async issue(
    address: `0x${string}`,
    label: string,
    options: { expiresAt?: Date; now?: Date } = {},
  ): Promise<{ token: string; record: ReadOnlyTokenRecord }> {
    const at = options.now ?? this.now();
    const token = generateReadOnlyToken();
    // The cap is applied in the insert itself, so two requests racing
    // cannot both read "19 live" and both add one.
    const result = await this.pool.query<TokenRow>(
      `INSERT INTO api_read_only_tokens (id, token_digest, address, label, scope, created_at, expires_at)
       SELECT $1, $2, lower($3), $4, $5, $6, $7
       WHERE (
         SELECT count(*) FROM api_read_only_tokens
         WHERE address = lower($3) AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > $6)
       ) < $8
       RETURNING ${COLUMNS}`,
      [randomUUID(), readOnlyTokenDigest(token), address, label, READ_ONLY_SCOPE, at, options.expiresAt ?? null, MAX_ACTIVE_TOKENS],
    );
    const row = result.rows[0];
    if (!row) throw new TooManyReadOnlyTokensError();
    // `address` is stored lowercased for lookup; the caller's own casing
    // is what every other route answers with, so that is what comes back.
    return { token, record: { ...fromRow(row), address } };
  }

  async authenticate(presented: string, now: Date): Promise<ReadOnlyTokenRecord | TokenRefusal> {
    if (!looksLikeReadOnlyToken(presented)) return "unknown";
    // The digest is the lookup key, so the database never sees the token
    // and no query of this table can be made to return one.
    const found = await this.pool.query<TokenRow>(
      `SELECT ${COLUMNS} FROM api_read_only_tokens WHERE token_digest = $1`,
      [readOnlyTokenDigest(presented)],
    );
    const row = found.rows[0];
    if (!row) return "unknown";
    const record = fromRow(row);
    const refusal = refusalFor(record, now);
    if (refusal) return refusal;
    const used = await this.pool.query<TokenRow>(
      `UPDATE api_read_only_tokens SET last_used_at = $2 WHERE id = $1 RETURNING ${COLUMNS}`,
      [record.id, now],
    );
    return used.rows[0] ? fromRow(used.rows[0]) : record;
  }

  async list(address: `0x${string}`): Promise<ReadOnlyTokenRecord[]> {
    const result = await this.pool.query<TokenRow>(
      `SELECT ${COLUMNS} FROM api_read_only_tokens WHERE address = lower($1) ORDER BY created_at DESC`,
      [address],
    );
    return result.rows.map((row) => ({ ...fromRow(row), address }));
  }

  async revoke(address: `0x${string}`, id: string, now: Date): Promise<boolean> {
    const result = await this.pool.query(
      "UPDATE api_read_only_tokens SET revoked_at = $3 WHERE id = $1 AND address = lower($2) AND revoked_at IS NULL",
      [id, address, now],
    );
    return (result.rowCount ?? 0) > 0;
  }
}
