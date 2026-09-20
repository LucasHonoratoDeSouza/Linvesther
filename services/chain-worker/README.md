# chain-worker

## indexer

Indexer with finality and reorg, per the EVM adapter specification: "Indexer
acompanha `included`, `safe` e `finalized`, não uma quantidade fixa de
confirmações que presume finalização... Reorg desfaz projeções do branch
abandonado e reprocessa eventos canônicos."

## Scope

- `indexer/types.ts` — `BlockTag` (`included`/`safe`/`finalized`),
  `ChainSnapshot` (the three RPC-reported tags per poll; `safe`/
  `finalized` are `null`, never defaulted to genesis, when the chain
  hasn't advanced them), and the `ChainClient`/`ProjectionStore`
  interfaces.
- `indexer/indexer.ts` — `Indexer.sync()`: on cold start, applies every
  block from a starting number up to the current head. On later syncs,
  walks the local chain back from its tip comparing each block's hash
  against the canonical chain at the same height; on divergence, reverts
  every abandoned block's projection (tip-first) and reprocesses the
  canonical replacements. Every retained block's tag is re-derived from
  the *current* snapshot on every sync — finality only ever advances
  because the RPC says so, never from local retention alone.
- `indexer/viemClient.ts` — real `ChainClient` backed by viem
  (`getBlock({ blockTag })`/`getBlock({ blockNumber })`), per the architecture design's
  chosen EVM client.
- `indexer/memoryProjectionStore.ts` — an in-memory `ProjectionStore`
  used by tests; throws on a duplicate apply or reverting a block that
  was never applied, so a test bug (double-apply, revert-then-forget)
  surfaces immediately instead of silently passing.

## Tests

- `test/indexer/indexer.test.ts` — fast unit tests against `FakeChain`
  (a small deterministic in-memory chain with real parent-hash linkage
  and a real reorg operation, not a stub returning expected values):
  cold start, steady-state sync (no duplication), reorg at various
  depths (including all the way to genesis), and finality separation.
- `test/indexer/anvil.integration.test.ts` — the same behavior against a
  **real** local Anvil node: mines real blocks, calls the real
  `anvil_reorg` RPC method to genuinely replace chain content, and
  confirms the indexer's post-reorg view matches what the node reports
  directly. Also confirms that on a bare Anvil devnet (no `--optimism`/L2
  config), the real `safe`/`finalized` RPC tags never advance past
  genesis — so newly mined blocks are correctly never reported as
  `safe`/`finalized`, checked against the actual node, not assumed.

### Out of scope

This delivers the indexer's core reconciliation state machine and a real
`ChainClient`. It does not implement a persistent (PostgreSQL) projection
store, a polling loop/scheduler, or backfill from an arbitrary historical
start — `ProjectionStore` and `sync()`'s `fromNumber` parameter are the
seams a future task wires those into.

## relayer

Idempotent relayer, per the operations design:
"Efeito remoto é reconciliado antes do retry: consultar hash/nonce/tx em
vez de transmitir novo payload" and the EVM adapter specification: "Relayer é
substituível; SDK permite gerar comando e transmitir diretamente."

### Scope

- `relayer/types.ts` — `RelayRequest` (a pre-signed command's calldata,
  opaque to the relayer — it never constructs or edits `data`) keyed by
  a `dedupKey` stable across retries of the same logical broadcast.
- `relayer/relayer.ts` — `Relayer.relay()`: if `dedupKey` was already
  broadcast and the recorded tx is still known to the node, returns the
  existing tx hash without sending anything new. Otherwise takes the
  account's pending nonce and broadcasts. Holds no privilege beyond what
  `BroadcastClient` exposes — the same interface a direct SDK call would
  use — so relaying and direct submission are equally valid paths.
- `relayer/viemBroadcastClient.ts` — real `BroadcastClient` backed by
  viem (`sendTransaction`, `getTransactionCount`, `getTransaction`).

### Tests

`test/relayer/anvil.integration.test.ts`, against a real local Anvil
node:

- A repeated `relay()` call with the same `dedupKey` (simulating a
  crash-and-retry) recovers the existing tx hash; the account's pending
  nonce does not move, confirming no second transaction was broadcast.
  The on-chain transaction's `input` is checked byte-for-byte against
  the original `data`.
- A direct `sendTransaction` call through the same `BroadcastClient`,
  bypassing `Relayer` entirely, succeeds identically — demonstrating the
  relayer is substitutable, not a required intermediary.
- Different `dedupKey`s never collapse into the same broadcast.

### Out of scope

This does not implement dropped-transaction resubmission (a mined-but-
since-reorged or mempool-evicted tx being genuinely resent with a bumped
fee), persistent (PostgreSQL) storage for `RelayRecord`, or EIP-712
command construction/signing — `RelayRequest.data` is assumed already
signed and fixed by the caller.
