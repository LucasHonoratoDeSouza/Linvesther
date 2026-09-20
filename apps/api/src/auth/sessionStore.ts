import { createHash, randomBytes } from "node:crypto";
import type { Pool } from "pg";

export interface Session {
  id: string;
  address: `0x${string}`;
  csrfToken: string;
}

/** How long a sign-in lasts before the person has to sign in again. */
export const SESSION_TTL_MS = 30 * 24 * 60 * 60 * 1000;

export interface SessionStore {
  create(address: `0x${string}`): Promise<Session>;
  /** A session past its expiry is never returned. */
  get(id: string): Promise<Session | undefined>;
  delete(id: string): Promise<void>;
}

function randomToken(): string {
  return randomBytes(32).toString("hex");
}

export class MemorySessionStore implements SessionStore {
  private readonly sessions = new Map<string, Session & { expiresAt: number }>();

  constructor(private readonly now: () => Date = () => new Date()) {}

  async create(address: `0x${string}`): Promise<Session> {
    const session = { id: randomToken(), address, csrfToken: randomToken(), expiresAt: this.now().getTime() + SESSION_TTL_MS };
    this.sessions.set(session.id, session);
    return { id: session.id, address, csrfToken: session.csrfToken };
  }

  async get(id: string): Promise<Session | undefined> {
    const session = this.sessions.get(id);
    if (!session) return undefined;
    if (session.expiresAt <= this.now().getTime()) {
      this.sessions.delete(id);
      return undefined;
    }
    return { id: session.id, address: session.address, csrfToken: session.csrfToken };
  }

  async delete(id: string): Promise<void> {
    this.sessions.delete(id);
  }
}

/** What is kept in the database for a session id: a hash, so that reading the
 * table gives an attacker nothing they could present as a cookie. */
const stored = (id: string) => createHash("sha256").update(id).digest("hex");

/** Sign-ins survive a restart of the API instead of logging everyone out. The
 * id a person holds is random and secret; only its hash is stored. */
export class PostgresSessionStore implements SessionStore {
  constructor(
    private readonly pool: Pool,
    private readonly now: () => Date = () => new Date(),
  ) {}

  async ensureSchema(): Promise<void> {
    await this.pool.query(`
      CREATE TABLE IF NOT EXISTS api_sessions (
        id TEXT PRIMARY KEY,
        address TEXT NOT NULL,
        csrf_token TEXT NOT NULL,
        expires_at TIMESTAMPTZ NOT NULL
      )`);
    await this.pool.query("CREATE INDEX IF NOT EXISTS api_sessions_expires_at ON api_sessions (expires_at)");
  }

  async create(address: `0x${string}`): Promise<Session> {
    const session: Session = { id: randomToken(), address, csrfToken: randomToken() };
    const expiresAt = new Date(this.now().getTime() + SESSION_TTL_MS);
    await this.pool.query("INSERT INTO api_sessions (id, address, csrf_token, expires_at) VALUES ($1, $2, $3, $4)", [stored(session.id), address, session.csrfToken, expiresAt]);
    // Expired rows are only ever dead weight; clearing them here keeps the table bounded.
    await this.pool.query("DELETE FROM api_sessions WHERE expires_at <= $1", [this.now()]);
    return session;
  }

  async get(id: string): Promise<Session | undefined> {
    const result = await this.pool.query<{ id: string; address: `0x${string}`; csrf_token: string }>(
      "SELECT id, address, csrf_token FROM api_sessions WHERE id = $1 AND expires_at > $2",
      [stored(id), this.now()],
    );
    const row = result.rows[0];
    return row ? { id, address: row.address, csrfToken: row.csrf_token } : undefined;
  }

  async delete(id: string): Promise<void> {
    await this.pool.query("DELETE FROM api_sessions WHERE id = $1", [stored(id)]);
  }
}
