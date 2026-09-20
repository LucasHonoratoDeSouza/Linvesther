// Registry states per the protocol specification: "UNANCHORED, INCLUDED,
// SAFE, FINALIZED, REORGED." This module covers the three block-level
// tags an indexer tracks — "Indexer acompanha included, safe e
// finalized, não uma quantidade fixa de confirmações que presume
// finalização" (the EVM adapter specification) — never a fixed confirmation
// count standing in for real finality.

export type BlockTag = "included" | "safe" | "finalized";

export interface BlockHeader {
  number: bigint;
  hash: `0x${string}`;
  parentHash: `0x${string}`;
}

/** The three RPC-reported tags for one polling tick. `safe`/`finalized`
 * are `null` when the chain has not advanced either tag past genesis
 * yet — never defaulted to "block 0 is finalized" by this type, so a
 * caller cannot accidentally treat "unknown" as "finalized". */
export interface ChainSnapshot {
  included: BlockHeader;
  safe: BlockHeader | null;
  finalized: BlockHeader | null;
}

export interface IndexedBlock {
  header: BlockHeader;
  tag: BlockTag;
}

/** Read-only chain access the indexer needs beyond the three tags: the
 * ability to fetch any historical block by number, used to walk back to
 * a common ancestor during reorg detection and to fetch canonical
 * replacement blocks. */
export interface ChainClient {
  getSnapshot(): Promise<ChainSnapshot>;
  getBlockByNumber(number: bigint): Promise<BlockHeader>;
}

/** What a reorg did, for callers that want to log/alert on it. */
export interface ReorgReport {
  /** The last block number both the old and new chain agree on. */
  commonAncestor: bigint;
  /** Blocks removed from the local view, tip-first. */
  reverted: IndexedBlock[];
  /** Canonical replacement blocks applied, in order. */
  applied: IndexedBlock[];
}

export interface ProjectionStore {
  apply(block: IndexedBlock): void | Promise<void>;
  revert(block: IndexedBlock): void | Promise<void>;
}
