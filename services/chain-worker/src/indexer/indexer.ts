// Core indexer state machine, per the EVM adapter specification: "Reorg
// desfaz projeções do branch abandonado e reprocessa eventos canônicos.
// Registros locais observados permanecem em trilha de auditoria, nunca
// como histórico final confirmado." and per the acceptance criteria: reorg
// undoes the abandoned branch's projections and reprocesses canonical
// blocks, and a block tagged only `safe`/`included` never reports as
// `finalized`.

import type { BlockTag, ChainClient, ChainSnapshot, IndexedBlock, ProjectionStore, ReorgReport } from "./types.js";

function tagFor(number: bigint, snapshot: ChainSnapshot): BlockTag {
  if (snapshot.finalized !== null && number <= snapshot.finalized.number) return "finalized";
  if (snapshot.safe !== null && number <= snapshot.safe.number) return "safe";
  return "included";
}

export class Indexer {
  private chain: IndexedBlock[] = [];
  // Requests that resync at the same moment must not both walk the same blocks.
  private running: Promise<unknown> = Promise.resolve();

  constructor(private readonly projections: ProjectionStore) {}

  /** The current canonical view, ascending by number. Never contains a
   * block from an abandoned branch — those are removed by `sync`, not
   * merely relabeled. */
  blocks(): readonly IndexedBlock[] {
    return this.chain;
  }

  /** Runs one polling tick. `fromNumber` only matters for the very first
   * call (cold start, nothing indexed yet); later calls always
   * reconcile from wherever the local view currently is. */
  sync(client: ChainClient, fromNumber: bigint): Promise<ReorgReport | null> {
    const turn = this.running.then(() => this.syncOnce(client, fromNumber));
    this.running = turn.catch(() => undefined);
    return turn;
  }

  private async syncOnce(client: ChainClient, fromNumber: bigint): Promise<ReorgReport | null> {
    const snapshot = await client.getSnapshot();

    if (this.chain.length === 0) {
      await this.extendTo(client, fromNumber, snapshot.included.number, snapshot);
      this.retag(snapshot);
      return null;
    }

    const divergedAt = await this.findDivergence(client);
    let report: ReorgReport | null = null;

    if (divergedAt !== null) {
      const revertIndex = this.chain.findIndex((b) => b.header.number === divergedAt);
      const reverted = this.chain.slice(revertIndex).reverse();
      for (const block of reverted) {
        await this.projections.revert(block);
      }
      this.chain = this.chain.slice(0, revertIndex);

      const applied = await this.extendTo(client, divergedAt, snapshot.included.number, snapshot);
      report = { commonAncestor: divergedAt - 1n, reverted, applied };
    } else {
      const tip = this.chain.at(-1)!.header.number;
      await this.extendTo(client, tip + 1n, snapshot.included.number, snapshot);
    }

    this.retag(snapshot);
    return report;
  }

  /** Walks the local chain from its tip backward, comparing each local
   * block's hash against the canonical chain at that same number, until
   * it finds one that still matches (the common ancestor) or exhausts
   * local history. Returns the lowest number that diverged, or `null`
   * if the tip itself still matches (no reorg). */
  private async findDivergence(client: ChainClient): Promise<bigint | null> {
    let divergedAt: bigint | null = null;
    for (let i = this.chain.length - 1; i >= 0; i--) {
      const local = this.chain[i]!;
      const remote = await client.getBlockByNumber(local.header.number);
      if (remote.hash === local.header.hash) {
        break;
      }
      divergedAt = local.header.number;
    }
    return divergedAt;
  }

  private async extendTo(client: ChainClient, from: bigint, to: bigint, snapshot: ChainSnapshot): Promise<IndexedBlock[]> {
    const applied: IndexedBlock[] = [];
    for (let n = from; n <= to; n++) {
      const header = await client.getBlockByNumber(n);
      const block: IndexedBlock = { header, tag: tagFor(n, snapshot) };
      await this.projections.apply(block);
      this.chain.push(block);
      applied.push(block);
    }
    return applied;
  }

  /** Re-derives every retained block's tag from the latest snapshot.
   * Finality only ever advances because the snapshot says so — never
   * because a block has been locally retained for a while. */
  private retag(snapshot: ChainSnapshot): void {
    this.chain = this.chain.map((block) => ({ ...block, tag: tagFor(block.header.number, snapshot) }));
  }
}
