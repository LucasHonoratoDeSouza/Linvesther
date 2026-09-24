# readonly

The read-only account API: what an outside agent — another program, an
assistant, a script the owner runs — may read about the account whose
token it holds.

Every route here is a `GET` under `/mcp`, authenticated only by a
read-only token, scoped to the identity that token belongs to, and
answered by the same worker reads the browser's own pages already use.
Nothing here computes a figure of its own.

## The credential

A read-only token is not the browser session. They are different
credentials, checked by different code:

| | Browser session | Read-only token |
| --- | --- | --- |
| Presented as | the `sid` cookie | `Authorization: Bearer lvz_ro_…` |
| Checked by | `auth/requireSession.ts` | `auth/requireReadOnlyToken.ts` |
| Authorizes | every route in this API | the six reads below |
| Ends when | it expires, or on sign-out | the owner revokes it |

`requireSession` reads only the cookie and `requireReadOnlyToken` reads
only the header, so neither credential can stand in for the other. A
browser that sends its cookie to `/mcp/account/nav` is answered `401`,
and a token sent to `POST /accounts/:id/binance-connection` is answered
`401` — both are covered by tests, not only by intent.

Tokens are created, listed and revoked from `api-tokens/routes.ts`,
which needs the session: minting a credential with a credential would
make revocation meaningless, since a token could always issue its
replacement. The token itself exists once, in the reply that creates it;
only its SHA-256 is stored, the same treatment a session id gets.

## Routes

`accountId` is optional on every route that takes one. Without it the
answer is the account whose id is the token owner's own address — the
first one every identity gets (see `auth/accountOwnership.ts`). With it,
`isOwner` decides, exactly as it does for the browser's own routes.

| Route | Answers | Read behind it |
| --- | --- | --- |
| `GET /mcp/account/state` | connected accounts, each connection's health, each one's current value and their total | `binance-worker list` + `status` + `nav` |
| `GET /mcp/account/portfolio` | positions, average cost, realized and unrealized profit, fees | `pnl.rs`, via `performance` |
| `GET /mcp/account/nav` | what the account is worth now, and of what | `publish.rs::compute_current_nav` |
| `GET /mcp/account/performance` | CAGR, Sharpe, Sortino, max drawdown, win rate | `history.rs::summarize_performance` |
| `GET /mcp/account/trades` | the executions already collected, filtered and paged | `trade_log.rs` |
| `GET /mcp/account/series` | value and return over a range, for a chart | `history.rs::compute_series` |

`/account/state` is the most expensive of the six: it reads each
connected account in turn, and each one's value is a live market read.
`/account/trades` is the cheapest — stored rows only, no exchange call.

### What an unavailable figure looks like

A metric with too little history is `null` with its own reason
(`sharpeUnavailable`, `cagrUnavailable`), never a zero standing in for
one. The same rule applies to the total balance on `/account/state`: if
one account's value cannot be read, or two accounts are valued in
different currencies, `balance` is `null` and `balanceUnavailable` says
why — a total that quietly left an account out would read as a real
figure while being wrong.

Money is added exactly (`totals.ts`): the worker's decimal strings are
added as integers at the widest scale any of them carried, never as
JavaScript numbers.

## Limits

`security/trafficLimits.ts` counts these routes under their own policy,
60 requests a minute, tighter than a browser's reads because an agent
polls and a person does not. The count is per token rather than per
address, so one agent cannot spend another's allowance, and the key is
the token's hash — no limit table or log line holds a usable credential.

An identity may hold at most `MAX_ACTIVE_TOKENS` live tokens; the cap is
applied inside the insert, so two requests racing cannot both slip past
it.

## What this module cannot do

It imports `binance-connect/worker.ts` (the subprocess contract) and
`binance-connect/types.ts` (types), and never `binance-connect/routes.ts`.
Nothing reachable from here can connect, rename, remove or sync an
account, or start a proof — `readOnlyApi.test.ts` asserts the exact set
of worker subcommands these six routes ever ask for.

## Tests

- `apps/api/test/readOnlyTokens.test.ts` — the store (in memory and on
  Postgres, asserted to behave identically), what is actually written to
  the table, and the middleware: a valid, unknown, malformed, revoked and
  expired token; the scheme in any case; an oversized header refused
  before any lookup; a session cookie ignored entirely.
- `apps/api/test/readOnlyApi.test.ts` — the six routes through the real
  Fastify app against a fake worker binary: the figures each returns,
  an unavailable metric passed through as unavailable, a refused total,
  another identity's account refused on every route, a revoked token
  refused on every route, every write route refusing the token, the
  exact worker subcommands used, the query-string refusals, the traffic
  limit, and exact decimal addition.
- `tests/e2e/mcp/mcpServer.e2e.test.ts` — a real MCP client over HTTP
  against the real MCP server and a real Fastify app.
