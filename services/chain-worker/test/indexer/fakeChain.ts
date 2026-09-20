import type { BlockHeader, ChainClient } from "../../src/indexer/types.js";

let hashCounter = 0;
function nextHash(): `0x${string}` {
  hashCounter += 1;
  return `0x${hashCounter.toString(16).padStart(64, "0")}`;
}

/** A small, deterministic in-memory chain: real parent-hash linkage and
 * a real reorg operation (replaces the top `depth` blocks with fresh
 * hashes at the same heights), not a stub that returns whatever the
 * indexer expects. Used for fast unit tests of the reconciliation logic;
 * `anvil.integration.test.ts` covers the same behavior against a real
 * node. */
export class FakeChain {
  private blocksByNumber = new Map<bigint, BlockHeader>();
  private tip = -1n;
  private safeNumber: bigint | null = null;
  private finalizedNumber: bigint | null = null;

  constructor() {
    this.mine(1); // genesis
  }

  mine(count: number): void {
    for (let i = 0; i < count; i++) {
      this.tip += 1n;
      const parent = this.blocksByNumber.get(this.tip - 1n);
      this.blocksByNumber.set(this.tip, { number: this.tip, hash: nextHash(), parentHash: parent?.hash ?? "0x0" });
    }
  }

  /** Replaces the top `depth` blocks with new ones at the same heights —
   * a real reorg of the local chain's tip, not a relabeling. */
  reorg(depth: bigint): void {
    const from = this.tip - depth + 1n;
    for (let n = from; n <= this.tip; n++) {
      const parent = this.blocksByNumber.get(n - 1n);
      this.blocksByNumber.set(n, { number: n, hash: nextHash(), parentHash: parent?.hash ?? "0x0" });
    }
  }

  setSafe(number: bigint): void {
    this.safeNumber = number;
  }

  setFinalized(number: bigint): void {
    this.finalizedNumber = number;
  }

  tipNumber(): bigint {
    return this.tip;
  }

  client(): ChainClient {
    return {
      getSnapshot: async () => ({
        included: this.mustGet(this.tip),
        safe: this.safeNumber === null ? null : this.mustGet(this.safeNumber),
        finalized: this.finalizedNumber === null ? null : this.mustGet(this.finalizedNumber),
      }),
      getBlockByNumber: async (number: bigint) => this.mustGet(number),
    };
  }

  private mustGet(number: bigint): BlockHeader {
    const block = this.blocksByNumber.get(number);
    if (!block) {
      throw new Error(`fake chain has no block ${number}`);
    }
    return block;
  }
}
