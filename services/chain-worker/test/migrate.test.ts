import { Pool } from "pg";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { runMigrations } from "../src/migrate.js";

const DATABASE_URL = process.env.DATABASE_URL ?? "postgresql://linvestherzk:linvestherzk-local-dev-only@localhost:5433/linvestherzk";

let pool: Pool;

beforeAll(async () => {
  pool = new Pool({ connectionString: DATABASE_URL });
  // Start from a genuinely clean slate — a previous test run (or T8's
  // suite, which depends on this same schema) may have already applied
  // this migration against the persistent local Postgres.
  await pool.query("DROP TABLE IF EXISTS accounts, tracks, identities, indexed_blocks, schema_migrations CASCADE");
});

afterAll(async () => {
  await pool.end();
});

describe("runMigrations against a real local Postgres", () => {
  it("applies 0001_init.sql once, and a second call is a no-op", async () => {
    const firstRun = await runMigrations(pool);
    expect(firstRun).toEqual(["0001_init.sql"]);

    const { rows } = await pool.query(
      "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' AND table_name = ANY($1)",
      [["indexed_blocks", "identities", "tracks", "accounts"]],
    );
    expect(rows).toHaveLength(4);

    const secondRun = await runMigrations(pool);
    expect(secondRun).toEqual([]);
  });
});
