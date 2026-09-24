# mcp-server

An MCP server that lets an agent read one Linvesther identity's own
account — connected accounts, current value, positions and profit,
performance metrics, executed trades and the value/return series — and do
nothing else.

Two transports, chosen by `MCP_TRANSPORT` — see "Configuration", below,
for the trust model behind each:

- **HTTP** (default), Streamable HTTP per the Model Context Protocol, at
  `/mcp`. Every call must carry its own bearer token; there is no
  configured fallback, on purpose.
- **stdio** (`MCP_TRANSPORT=stdio`), for a personal agent that spawns
  this process directly (Claude Desktop, an IDE, a CLI). Reads with one
  token this process was started with, since stdio has no per-call
  credential of its own.

## The six tools

| Tool | Answers |
| --- | --- |
| `get_account_state` | every connected account, each connection's health, each one's current value and their total |
| `get_portfolio` | positions, average cost, realized and unrealized profit, fees |
| `get_nav` | what the account is worth right now, and of what |
| `get_performance` | CAGR, Sharpe, Sortino, maximum drawdown, win rate |
| `get_trades` | the executions already collected, by market and period, paged |
| `get_chart_series` | value and time-weighted return over a range |

Each declares a JSON Schema for what it takes and what it returns, so
`tools/list` describes both and every answer is validated against its own
shape before it leaves. Each is annotated `readOnlyHint: true`,
`destructiveHint: false`, and its description says in words that it reads
and cannot act.

## Why it cannot write

Not because a check refuses: because the code that writes is not here.

- This package declares no dependency on `@linvestherzk/api`, so no
  module that can connect an exchange, rename or remove an account,
  relay a transaction or start a proof is installed alongside it.
- `src/api.ts` is the only way out, and every request it can make is a
  `GET` under `/mcp/account/`. There are six of them, one per tool.
- Nothing here spawns a process or opens a database connection — the
  API's own worker subprocess and Postgres pool are on the other side of
  an HTTP boundary.

`test/isolation.test.ts` asserts all three against the source, so the
property survives a later edit rather than depending on one being
remembered.

The server holds no session state: a server and a transport are built per
request and closed with it, so one identity's call cannot be answered
with another's state.

## Configuration

| Variable | Meaning |
| --- | --- |
| `LINVESTHER_API_URL` | where `apps/api` is (default `http://127.0.0.1:4301`) |
| `LINVESTHER_API_TIMEOUT_MS` | how long one read may take (default 120000) |
| `MCP_TRANSPORT` | `http` (default) or `stdio` |
| `LINVESTHER_READ_ONLY_TOKEN` | **stdio only** — required there, ignored (with a warning) under `http` |
| `MCP_HOST`, `MCP_PORT` | HTTP only: where to listen (default `127.0.0.1:4302`) |
| `MCP_ALLOWED_HOSTS` | HTTP only: comma-separated hostnames to accept on `Host`/`Origin` (default: loopback aliases) |

A token comes from the owner's own `/settings/api-tokens` page. It looks
like `lvz_ro_…`, reads only that identity's account, and can be revoked
there at any moment.

**HTTP never falls back to a configured token.** A tokenless or
malformed request is refused, in words a caller can act on, whatever
`LINVESTHER_READ_ONLY_TOKEN` happens to be set to — the type this
transport builds its server from (`HttpServerOptions`) cannot even carry
that field, so there is nothing for a fallback to read even if the
handler tried; `http.ts`'s own doc comment covers the two independent
guarantees behind that. This is deliberate: falling back to a configured
token whenever a caller sent none would leak that identity's balance,
positions and trades to any other caller that reached this port and sent
nothing at all — exactly the shape of bug this split exists to make
impossible, not just avoided by convention. Run several identities this
way by leaving no default configured and having each client present its
own token as `Authorization: Bearer lvz_ro_…`; it is passed straight
through to the API, which decides what that token may read.

**stdio is the one place a configured token is honored**, because it is
the one transport with no per-call credential to carry instead: whoever
can spawn this process and read its stdout already controls everything
the token would let them read, the same trust boundary any stdio MCP
server already assumes. `LINVESTHER_READ_ONLY_TOKEN` is required to run
this way — refused up front, not silently unauthenticated, when it is
missing.

**DNS rebinding protection**, HTTP only: every request's `Host` header —
and its `Origin`, when it sends one — must name a hostname in
`MCP_ALLOWED_HOSTS` (default: `localhost`, `127.0.0.1`, `::1`), checked
before the request reaches the MCP transport at all. This defends
against a page whose URL names a domain the attacker controls, which
resolves to `127.0.0.1` only after an initial check passes: the
browser's own `Host` header on that request names the attacker's domain,
never `localhost`, so it is refused. Set `MCP_ALLOWED_HOSTS` only for a
deliberate deployment behind a real hostname.

The default listen address is `127.0.0.1`: this server answers with real
balances and trades, so exposing it to a network is a deployment
decision, never the default. The API's own limits still apply — the
read-only routes are capped per token, so an agent that polls too hard is
refused by Linvesther whatever this server allows.

## Running it

```sh
pnpm install
pnpm --filter @linvestherzk/mcp-server start
```

Point an MCP client at `http://127.0.0.1:4302/mcp`. `GET /healthz`
answers whether the process is up; it reads nothing.

## Tests

```sh
pnpm --filter @linvestherzk/mcp-server test
pnpm --filter @linvestherzk/mcp-server typecheck
```

- `test/tools.test.ts` — a real MCP client over an in-memory transport
  against a stub API: the six tools and nothing that acts, the published
  input/output schemas, each tool's own route and token, filters passed
  and omitted, arguments the schema refuses (with nothing read), an
  unavailable metric passed through as unavailable, and a refusal from
  the API reported as a failed call.
- `test/http.test.ts` — a real MCP client over a real socket: the
  handshake, the token taken from the calling request, a tokenless call
  refused with nothing read (including with a `defaultToken`-shaped
  value forced onto the server's options at runtime — the scenario a
  configured `LINVESTHER_READ_ONLY_TOKEN` plus a tokenless caller would
  produce, refused all the same), the liveness check, one path serving
  the protocol, and the DNS-rebinding Host/Origin guard (accepted
  loopback aliases, a spoofed `Host` refused before any read, an
  operator-widened allowlist).
- `test/stdio.test.ts` — the one piece of stdio's own logic: its
  required `token` becomes `defaultToken` and nothing else about the
  caller's options changes. (A real handshake over stdio needs a spawned
  child process — `StdioClientTransport` only ever talks to one, never
  to injected streams — so it is not exercised here; see this file's own
  comment.)
- `test/isolation.test.ts` — the structural guarantees above.
- `tests/e2e/mcp/mcpServer.e2e.test.ts` — the whole path: a real MCP
  client, this server over HTTP, a real `apps/api` instance, a real
  read-only token, and every one of the six tools.
