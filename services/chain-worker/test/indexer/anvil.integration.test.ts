import { type ChildProcess, spawn } from "node:child_process";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { Indexer } from "../../src/indexer/indexer.js";
import { MemoryProjectionStore } from "../../src/indexer/memoryProjectionStore.js";
import { ViemChainClient } from "../../src/indexer/viemClient.js";

// Real reorg against a real node (Anvil), per
// the protocol specification: "Integração: PostgreSQL
// real/objetos/RPC local e fixtures Binance; não usar mocks que devolvem
// o mesmo valor esperado da implementação." `anvil_reorg` genuinely
// replaces block content at the given depth — this is not a simulated
// double.

const PORT = 8799;
const RPC_URL = `http://127.0.0.1:${PORT}`;

async function rpc(method: string, params: unknown[]): Promise<unknown> {
  const response = await fetch(RPC_URL, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  });
  const body = (await response.json()) as { result?: unknown; error?: { message: string } };
  if (body.error) {
    throw new Error(`${method} failed: ${body.error.message}`);
  }
  return body.result;
}

async function waitForRpc(): Promise<void> {
  for (let attempt = 0; attempt < 50; attempt++) {
    try {
      await rpc("eth_blockNumber", []);
      return;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }
  throw new Error("anvil did not become ready in time");
}

let anvil: ChildProcess;

beforeAll(async () => {
  anvil = spawn("anvil", ["--port", String(PORT), "--silent"], { stdio: "ignore" });
  await waitForRpc();
});

afterAll(() => {
  anvil.kill();
});

describe("Indexer against a real Anvil node", () => {
  it("undoes the abandoned branch and reprocesses the canonical one after a real anvil_reorg", async () => {
    await rpc("anvil_mine", ["0x4"]); // tip = 4

    const store = new MemoryProjectionStore();
    const indexer = new Indexer(store);
    const client = new ViemChainClient(RPC_URL);

    await indexer.sync(client, 0n);
    expect(indexer.blocks().map((b) => b.header.number)).toEqual([0n, 1n, 2n, 3n, 4n]);

    const hashesBeforeReorg = new Map(indexer.blocks().map((b) => [b.header.number, b.header.hash]));

    // Real reorg: anvil_reorg(depth, txBlockPairs) replaces the top
    // `depth` blocks with freshly mined ones.
    await rpc("anvil_reorg", [2, []]);

    const report = await indexer.sync(client, 0n);

    expect(report).not.toBeNull();
    expect(report!.reverted.map((b) => b.header.number)).toEqual([4n, 3n]);
    expect(report!.reverted.map((b) => b.header.hash)).toEqual([hashesBeforeReorg.get(4n), hashesBeforeReorg.get(3n)]);

    // The replacement blocks must have different hashes than the
    // abandoned ones, and must be the only versions of 3/4 now applied.
    const currentHashes = new Map(store.current().map((b) => [b.header.number, b.header.hash]));
    expect(currentHashes.get(3n)).not.toBe(hashesBeforeReorg.get(3n));
    expect(currentHashes.get(4n)).not.toBe(hashesBeforeReorg.get(4n));
    expect(store.current()).toHaveLength(5);

    // Cross-check against the node directly: the indexer's post-reorg
    // view must match what the real chain now reports.
    const canonicalBlock3 = await client.getBlockByNumber(3n);
    const canonicalBlock4 = await client.getBlockByNumber(4n);
    expect(currentHashes.get(3n)).toBe(canonicalBlock3.hash);
    expect(currentHashes.get(4n)).toBe(canonicalBlock4.hash);
  });

  it("newly mined blocks on a fresh local devnet are never reported as safe or finalized", async () => {
    // A bare Anvil devnet (no --optimism/L2 config) never advances the
    // safe/finalized RPC tags past genesis, which this test confirms
    // directly against the real node rather than assuming it.
    await rpc("anvil_mine", ["0x3"]);

    const store = new MemoryProjectionStore();
    const indexer = new Indexer(store);
    const client = new ViemChainClient(RPC_URL);

    await indexer.sync(client, 0n);

    const tip = indexer.blocks().at(-1)!;
    expect(tip.header.number).toBeGreaterThan(0n);
    expect(tip.tag).not.toBe("finalized");
    expect(tip.tag).not.toBe("safe");
  });
});
