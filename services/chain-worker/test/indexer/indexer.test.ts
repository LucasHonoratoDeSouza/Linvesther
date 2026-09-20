import { describe, expect, it } from "vitest";
import { Indexer } from "../../src/indexer/indexer.js";
import { MemoryProjectionStore } from "../../src/indexer/memoryProjectionStore.js";
import { FakeChain } from "./fakeChain.js";

describe("Indexer cold start and steady-state sync", () => {
  it("applies every block from genesis to the current head on first sync", async () => {
    const chain = new FakeChain();
    chain.mine(4); // tip = 4
    const store = new MemoryProjectionStore();
    const indexer = new Indexer(store);

    await indexer.sync(chain.client(), 0n);

    expect(indexer.blocks().map((b) => b.header.number)).toEqual([0n, 1n, 2n, 3n, 4n]);
    expect(store.applyLog).toHaveLength(5);
  });

  it("applies only newly mined blocks on a later sync, never re-applying earlier ones", async () => {
    const chain = new FakeChain();
    const store = new MemoryProjectionStore();
    const indexer = new Indexer(store);
    await indexer.sync(chain.client(), 0n);
    expect(store.applyLog).toHaveLength(1);

    chain.mine(3);
    await indexer.sync(chain.client(), 0n);

    expect(store.applyLog).toHaveLength(4);
    expect(indexer.blocks().map((b) => b.header.number)).toEqual([0n, 1n, 2n, 3n]);
  });
});

describe("Reorg: abandoned branch undone, canonical branch reprocessed", () => {
  it("reverts exactly the abandoned blocks and applies exactly the canonical replacements", async () => {
    const chain = new FakeChain();
    chain.mine(4); // tip = 4
    const store = new MemoryProjectionStore();
    const indexer = new Indexer(store);
    await indexer.sync(chain.client(), 0n);

    const abandonedHashesAt = new Map(indexer.blocks().map((b) => [b.header.number, b.header.hash]));

    chain.reorg(2n); // blocks 3 and 4 replaced
    const report = await indexer.sync(chain.client(), 0n);

    expect(report).not.toBeNull();
    expect(report!.reverted.map((b) => b.header.number)).toEqual([4n, 3n]);
    expect(report!.reverted.map((b) => b.header.hash)).toEqual([abandonedHashesAt.get(4n), abandonedHashesAt.get(3n)]);
    expect(report!.applied.map((b) => b.header.number)).toEqual([3n, 4n]);
    expect(report!.commonAncestor).toBe(2n);

    // The abandoned blocks' hashes must be gone from the projection
    // store; blocks 0-2 (untouched by the reorg) must still be there
    // exactly once, and the two replacement blocks must be applied
    // exactly once — never duplicated with the abandoned versions.
    const currentHashes = new Set(store.current().map((b) => b.header.hash));
    expect(currentHashes.has(abandonedHashesAt.get(3n)!)).toBe(false);
    expect(currentHashes.has(abandonedHashesAt.get(4n)!)).toBe(false);
    expect(store.current()).toHaveLength(5); // 0,1,2 retained + 3,4 replaced

    expect(indexer.blocks().map((b) => b.header.number)).toEqual([0n, 1n, 2n, 3n, 4n]);
  });

  it("a reorg reaching all the way to genesis reverts and reprocesses everything", async () => {
    const chain = new FakeChain();
    chain.mine(2); // tip = 2
    const store = new MemoryProjectionStore();
    const indexer = new Indexer(store);
    await indexer.sync(chain.client(), 0n);

    chain.reorg(3n); // depth covers 0,1,2 (clamped to what exists)
    const report = await indexer.sync(chain.client(), 0n);

    expect(report).not.toBeNull();
    expect(report!.reverted.map((b) => b.header.number)).toEqual([2n, 1n, 0n]);
    expect(report!.applied.map((b) => b.header.number)).toEqual([0n, 1n, 2n]);
    expect(store.current()).toHaveLength(3);
  });
});

describe("Finality separation: safe/included never report as finalized", () => {
  it("tags blocks strictly by the current snapshot, never optimistically", async () => {
    const chain = new FakeChain();
    chain.mine(5); // tip = 5
    chain.setFinalized(1n);
    chain.setSafe(3n);
    const store = new MemoryProjectionStore();
    const indexer = new Indexer(store);

    await indexer.sync(chain.client(), 0n);

    const byNumber = new Map(indexer.blocks().map((b) => [b.header.number, b.tag]));
    expect(byNumber.get(0n)).toBe("finalized");
    expect(byNumber.get(1n)).toBe("finalized");
    expect(byNumber.get(2n)).toBe("safe");
    expect(byNumber.get(3n)).toBe("safe");
    expect(byNumber.get(4n)).toBe("included");
    expect(byNumber.get(5n)).toBe("included");

    // Every block above the finalized boundary must not be finalized.
    for (const block of indexer.blocks()) {
      if (block.header.number > 1n) {
        expect(block.tag).not.toBe("finalized");
      }
    }
  });

  it("finality only advances because the snapshot says so, never from local retention alone", async () => {
    const chain = new FakeChain();
    chain.mine(3);
    const store = new MemoryProjectionStore();
    const indexer = new Indexer(store);

    await indexer.sync(chain.client(), 0n);
    // No safe/finalized set: everything must still read "included", no
    // matter that it has been indexed since the very first sync.
    expect(indexer.blocks().every((b) => b.tag === "included")).toBe(true);

    await indexer.sync(chain.client(), 0n);
    expect(indexer.blocks().every((b) => b.tag === "included")).toBe(true);
  });
});

describe("Syncs that overlap", () => {
  it("apply each block once when several requests ask to sync at the same moment", async () => {
    const chain = new FakeChain();
    chain.mine(4);
    const store = new MemoryProjectionStore();
    const indexer = new Indexer(store);

    await Promise.all([1, 2, 3, 4].map(() => indexer.sync(chain.client(), 0n)));

    expect(store.applyLog).toHaveLength(5);
    expect(indexer.blocks().map((b) => b.header.number)).toEqual([0n, 1n, 2n, 3n, 4n]);
  });

  it("keeps working after one sync fails", async () => {
    const chain = new FakeChain();
    const store = new MemoryProjectionStore();
    const indexer = new Indexer(store);
    const broken = { ...chain.client(), getSnapshot: async () => { throw new Error("rpc down"); } };

    await expect(indexer.sync(broken, 0n)).rejects.toThrow("rpc down");
    await indexer.sync(chain.client(), 0n);

    expect(indexer.blocks()).toHaveLength(1);
  });
});
