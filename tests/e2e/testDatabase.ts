import { randomBytes } from "node:crypto";
import pg from "pg";

/** The Postgres server the tests may use. Tests never touch a database that
 * already exists there: each run creates its own and drops it afterwards. */
export const ADMIN_DATABASE_URL =
  process.env.DATABASE_URL ?? "postgresql://linvestherzk:linvestherzk-local-dev-only@localhost:5433/postgres";

/** `url` with its database replaced by `name`. */
export function databaseUrl(url: string, name: string): string {
  const parsed = new URL(url);
  parsed.pathname = `/${name}`;
  return parsed.toString();
}

/** A fresh, empty database on the test server, and a way to remove it. */
export async function createThrowawayDatabase(prefix = "linvesther_test", name = `${prefix}_${randomBytes(5).toString("hex")}`) {
  if (!/^[a-z0-9_]+$/.test(name)) throw new Error(`unsafe database name: ${name}`);
  const admin = new pg.Client({ connectionString: databaseUrl(ADMIN_DATABASE_URL, "postgres") });
  await admin.connect();
  try {
    await admin.query(`CREATE DATABASE ${name}`);
  } finally {
    await admin.end();
  }
  return { name, url: databaseUrl(ADMIN_DATABASE_URL, name), drop: () => dropDatabase(name) };
}

/** Removes a database this suite created, closing any connection still open to it. */
export async function dropDatabase(name: string): Promise<void> {
  if (!/^[a-z0-9_]+$/.test(name)) throw new Error(`unsafe database name: ${name}`);
  const cleanup = new pg.Client({ connectionString: databaseUrl(ADMIN_DATABASE_URL, "postgres") });
  await cleanup.connect();
  try {
    await cleanup.query(`DROP DATABASE IF EXISTS ${name} WITH (FORCE)`);
  } finally {
    await cleanup.end();
  }
}
