// The only way this server reaches Linvesther: HTTP GETs against
// `apps/api`'s read-only account routes, carrying the caller's own
// read-only token.
//
// Deliberately not a package import. This service does not depend on
// `@linvestherzk/api` at all, so no module that can connect an exchange,
// rename or remove an account, relay a transaction or start a proof is
// even present in its dependency graph — isolation by what is installed,
// not by a check at run time.

/** A refusal from the API, carried through to the caller with its own
 * status and code rather than flattened into "something went wrong". */
export class ReadOnlyApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    readonly detail?: string,
  ) {
    super(detail ? `${code}: ${detail}` : code);
    this.name = "ReadOnlyApiError";
  }
}

/** The API could not be reached at all — a different thing from a
 * refusal, and reported as such. */
export class ReadOnlyApiUnreachableError extends Error {
  constructor(cause: unknown) {
    super(cause instanceof Error ? cause.message : "the Linvesther API could not be reached");
    this.name = "ReadOnlyApiUnreachableError";
  }
}

export interface ReadOnlyApiOptions {
  /** Where `apps/api` is, e.g. `http://127.0.0.1:4301`. */
  baseUrl: string;
  /** How long one read may take. A performance read over months of real
   * history is minutes of work in the worker, so this is generous by
   * default — but never unbounded, since an agent must be told the read
   * failed rather than be left waiting. */
  timeoutMs?: number;
  /** Injected by the tests, which drive a real Fastify app in process. */
  fetch?: typeof globalThis.fetch;
}

export const DEFAULT_TIMEOUT_MS = 120_000;

type QueryValue = string | number | undefined;

/** Every read the tools can make. One method per route, so the set of
 * things this server is able to ask for is the list below and nothing
 * else. */
export class ReadOnlyAccountApi {
  private readonly baseUrl: string;
  private readonly timeoutMs: number;
  private readonly fetchImpl: typeof globalThis.fetch;

  constructor(options: ReadOnlyApiOptions) {
    this.baseUrl = options.baseUrl.replace(/\/+$/, "");
    this.timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    this.fetchImpl = options.fetch ?? globalThis.fetch;
  }

  accountState(token: string): Promise<unknown> {
    return this.read("/mcp/account/state", token, {});
  }

  portfolio(token: string, query: { accountId?: string }): Promise<unknown> {
    return this.read("/mcp/account/portfolio", token, query);
  }

  nav(token: string, query: { accountId?: string }): Promise<unknown> {
    return this.read("/mcp/account/nav", token, query);
  }

  performance(token: string, query: { accountId?: string }): Promise<unknown> {
    return this.read("/mcp/account/performance", token, query);
  }

  trades(
    token: string,
    query: { accountId?: string; symbol?: string; since?: number; until?: number; cursor?: string; limit?: number },
  ): Promise<unknown> {
    return this.read("/mcp/account/trades", token, query);
  }

  series(token: string, query: { accountId?: string; range?: string }): Promise<unknown> {
    return this.read("/mcp/account/series", token, query);
  }

  private async read(path: string, token: string, query: Record<string, QueryValue>): Promise<unknown> {
    const url = new URL(`${this.baseUrl}${path}`);
    for (const [name, value] of Object.entries(query)) {
      if (value !== undefined) url.searchParams.set(name, String(value));
    }
    let response: Response;
    try {
      response = await this.fetchImpl(url, {
        method: "GET",
        headers: { authorization: `Bearer ${token}`, accept: "application/json" },
        signal: AbortSignal.timeout(this.timeoutMs),
      });
    } catch (cause) {
      throw new ReadOnlyApiUnreachableError(cause);
    }
    const body = (await response.json().catch(() => ({}))) as Record<string, unknown>;
    if (!response.ok) {
      const code = typeof body.error === "string" ? body.error : `request_failed_${response.status}`;
      throw new ReadOnlyApiError(response.status, code, typeof body.detail === "string" ? body.detail : undefined);
    }
    return body;
  }
}
