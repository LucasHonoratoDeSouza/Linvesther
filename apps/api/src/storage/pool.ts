import pg from "pg";

/** A connection pool that survives the database restarting.
 *
 * A pool emits `error` when an idle connection is dropped (a restart, a
 * failover, a network blip). With no listener Node treats that as an
 * uncaught exception and the whole API exits. Here it is logged and the pool
 * simply opens a new connection on the next query. */
export function createPool(connectionString: string, options: { max?: number } = {}): pg.Pool {
  const pool = new pg.Pool({
    connectionString,
    max: options.max ?? 10,
    connectionTimeoutMillis: 5_000,
    idleTimeoutMillis: 30_000,
    // No single query is allowed to hold a connection for long.
    statement_timeout: 30_000,
  });
  pool.on("error", (error) => {
    console.error(`database connection error (the pool reconnects on next use): ${error.message}`);
  });
  return pool;
}
