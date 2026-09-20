import { type ChildProcess, spawn } from "node:child_process";
import { createPublicClient, http } from "viem";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { MemoryRelayStore } from "../../src/relayer/memoryRelayStore.js";
import { Relayer } from "../../src/relayer/relayer.js";
import { ViemBroadcastClient } from "../../src/relayer/viemBroadcastClient.js";

// Real broadcast against a real node (Anvil), per
// the protocol specification's integration testing rule:
// no mock that returns whatever the implementation expects.

const PORT = 8798;
const RPC_URL = `http://127.0.0.1:${PORT}`;

// Anvil's well-known default account #0 private key — a public test-only
// fixture, not a secret, used by every Foundry/Hardhat local devnet.
const PRIVATE_KEY = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80" as const;
const RECIPIENT = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8" as const; // Anvil default account #1

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

describe("Relayer against a real Anvil node", () => {
  it("a repeated broadcast recovers the existing tx/nonce instead of sending a new one", async () => {
    const client = new ViemBroadcastClient(RPC_URL, PRIVATE_KEY);
    const store = new MemoryRelayStore();
    const relayer = new Relayer(client, store);

    const request = { dedupKey: "cmd-1", to: RECIPIENT, data: "0xdeadbeef" as const, value: 0n };

    const firstHash = await relayer.relay(request);
    await rpc("anvil_mine", ["0x1"]); // land it in a block

    const nonceAfterFirst = await client.getPendingNonce(client.address());

    // Simulate a crash-and-retry: relay the identical request again.
    const secondHash = await relayer.relay(request);

    expect(secondHash).toBe(firstHash);

    // No second transaction was broadcast: the account's pending nonce
    // did not move.
    const nonceAfterSecond = await client.getPendingNonce(client.address());
    expect(nonceAfterSecond).toBe(nonceAfterFirst);

    // Cross-check against the real node directly: exactly one
    // transaction with this calldata exists, and its input is byte for
    // byte what was signed — the relayer never altered the payload.
    const publicClient = createPublicClient({ transport: http(RPC_URL) });
    const onChain = await publicClient.getTransaction({ hash: firstHash });
    expect(onChain.input).toBe(request.data);
    expect(onChain.to?.toLowerCase()).toBe(RECIPIENT.toLowerCase());
  });

  it("direct broadcast via the same client, bypassing the relayer, works identically", async () => {
    // Demonstrates the relayer holds no special privilege: the owner's
    // own SDK could call sendTransaction directly, with the same
    // effect, exactly as the EVM adapter specification requires ("SDK
    // permite gerar comando e transmitir diretamente").
    const client = new ViemBroadcastClient(RPC_URL, PRIVATE_KEY);
    const nonce = await client.getPendingNonce(client.address());
    const hash = await client.sendTransaction({ to: RECIPIENT, data: "0xcafef00d", value: 0n, nonce });
    await rpc("anvil_mine", ["0x1"]);

    const publicClient = createPublicClient({ transport: http(RPC_URL) });
    const receipt = await publicClient.getTransactionReceipt({ hash });
    expect(receipt.status).toBe("success");
  });

  it("different dedup keys never collapse into the same broadcast", async () => {
    const client = new ViemBroadcastClient(RPC_URL, PRIVATE_KEY);
    const store = new MemoryRelayStore();
    const relayer = new Relayer(client, store);

    const first = await relayer.relay({ dedupKey: "cmd-a", to: RECIPIENT, data: "0x01" as const, value: 0n });
    await rpc("anvil_mine", ["0x1"]);
    const second = await relayer.relay({ dedupKey: "cmd-b", to: RECIPIENT, data: "0x02" as const, value: 0n });
    await rpc("anvil_mine", ["0x1"]);

    expect(first).not.toBe(second);
  });

  it("warns when the relayer's balance is below the threshold, before the relay attempt (ONCHAIN-13)", async () => {
    const poorKey = generatePrivateKey();
    const poorAddress = privateKeyToAccount(poorKey).address;
    // Funded with 1 wei — genuinely below any real threshold, but
    // non-zero so the transaction itself still gets attempted (the
    // warning is advance notice, not a hard block — see relayer.ts).
    await rpc("anvil_setBalance", [poorAddress, "0x1"]);

    const client = new ViemBroadcastClient(RPC_URL, poorKey);
    const relayer = new Relayer(client, new MemoryRelayStore());
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

    try {
      await relayer.relay({ dedupKey: "poor-relayer", to: RECIPIENT, data: "0x01" as const, value: 0n }).catch(() => {
        // Expected: 1 wei can't cover gas, the send itself fails — the
        // warning must have already been logged by that point.
      });
      expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining(poorAddress));
      expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining("balance is low"));
    } finally {
      warnSpy.mockRestore();
    }
  });
});
