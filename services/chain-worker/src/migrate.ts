// Minimal migration runner — same spirit as sqlx's `migrate` feature
// (already used by services/collector/binance/worker in Rust): apply
// numbered .sql files once each, track which ones ran, never reapply.
// No framework: this is the whole thing.
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import type { Pool } from "pg";

const MIGRATIONS_DIR = path.resolve(import.meta.dirname, "../migrations");

async function ensureMigrationsTable(pool: Pool): Promise<void> {
  await pool.query(`CREATE TABLE IF NOT EXISTS schema_migrations (name TEXT PRIMARY KEY, applied_at TIMESTAMPTZ NOT NULL DEFAULT now())`);
}

/** Applies every `.sql` file under `migrations/` not already recorded
 * in `schema_migrations`, in filename order (numeric prefix), each
 * inside its own transaction. Safe to call on every process start —
 * a migration already applied is a no-op, never reapplied. */
export async function runMigrations(pool: Pool): Promise<string[]> {
  await ensureMigrationsTable(pool);
  const { rows } = await pool.query<{ name: string }>("SELECT name FROM schema_migrations");
  const applied = new Set(rows.map((row) => row.name));

  const files = readdirSync(MIGRATIONS_DIR)
    .filter((name) => name.endsWith(".sql"))
    .sort();

  const newlyApplied: string[] = [];
  for (const file of files) {
    if (applied.has(file)) continue;
    const sql = readFileSync(path.join(MIGRATIONS_DIR, file), "utf8");
    const client = await pool.connect();
    try {
      await client.query("BEGIN");
      await client.query(sql);
      await client.query("INSERT INTO schema_migrations (name) VALUES ($1)", [file]);
      await client.query("COMMIT");
      newlyApplied.push(file);
    } catch (error) {
      await client.query("ROLLBACK");
      throw error;
    } finally {
      client.release();
    }
  }
  return newlyApplied;
}
