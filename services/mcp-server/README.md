# mcp-server

An MCP server that lets an agent read one Linvesther identity's own
account — connected accounts, current value, positions and profit,
performance metrics, executed trades and the value/return series — and do
nothing else.

Transport is Streamable HTTP, per the Model Context Protocol, at `/mcp`.

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
| `LINVESTHER_READ_ONLY_TOKEN` | the token to read with when a call carries none of its own |
| `LINVESTHER_API_TIMEOUT_MS` | how long one read may take (default 120000) |
| `MCP_HOST`, `MCP_PORT` | where to listen (default `127.0.0.1:4302`) |

A token comes from the owner's own `/settings/api-tokens` page. It looks
like `lvz_ro_…`, reads only that identity's account, and can be revoked
there at any moment.

There are two ways to give this server a token, and a call's own always
wins:

- **One person, their own agent.** Set
  `LINVESTHER_READ_ONLY_TOKEN` and run the server locally. Every call
  reads that identity's account.
- **Several identities.** Leave it unset and have each client present its
  own token as `Authorization: Bearer lvz_ro_…` on the MCP request; it is
  passed through to the API, which decides what that token may read.

Because a call's own token always wins, a server configured for one
person never answers somebody else's call with that person's credential.

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
  handshake, the token taken from the calling request, the configured
  fallback, a call with no token reading nothing, the liveness check, and
  one path serving the protocol.
- `test/isolation.test.ts` — the structural guarantees above.
- `tests/e2e/mcp/mcpServer.e2e.test.ts` — the whole path: a real MCP
  client, this server over HTTP, a real `apps/api` instance, a real
  read-only token, and every one of the six tools.
