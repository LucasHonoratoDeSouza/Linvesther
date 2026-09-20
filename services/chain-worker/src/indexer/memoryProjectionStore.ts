import type { IndexedBlock, ProjectionStore } from "./types.js";

/** Records every apply/revert call in order, keyed by block number and
 * hash, so tests can assert exactly what happened without duplication —
 * a real projection store (e.g. PostgreSQL) would instead insert/delete
 * rows per the architecture design's "Transação
 * local grava evento, projeção e outbox juntos." */
export class MemoryProjectionStore implements ProjectionStore {
  readonly applyLog: IndexedBlock[] = [];
  readonly revertLog: IndexedBlock[] = [];
  private readonly applied = new Map<string, IndexedBlock>();

  private key(block: IndexedBlock): string {
    return `${block.header.number}:${block.header.hash}`;
  }

  apply(block: IndexedBlock): void {
    const key = this.key(block);
    if (this.applied.has(key)) {
      throw new Error(`duplicate apply for block ${key}`);
    }
    this.applied.set(key, block);
    this.applyLog.push(block);
  }

  revert(block: IndexedBlock): void {
    const key = this.key(block);
    if (!this.applied.has(key)) {
      throw new Error(`revert of a block that was never applied: ${key}`);
    }
    this.applied.delete(key);
    this.revertLog.push(block);
  }

  /** Blocks currently applied (survived any reverts), in application
   * order. */
  current(): IndexedBlock[] {
    return [...this.applied.values()];
  }
}
