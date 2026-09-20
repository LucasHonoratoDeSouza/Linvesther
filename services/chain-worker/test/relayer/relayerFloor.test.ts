import { describe, expect, it } from "vitest";
import { MemoryRelayStore, Relayer, RelayerUnderfundedError, type BroadcastClient } from "../../src/index.js";

function client(balance: bigint) {
  const sent: string[] = [];
  const broadcast: BroadcastClient = {
    address: () => "0x000000000000000000000000000000000000dEaD",
    getPendingNonce: async () => 0,
    sendTransaction: async () => {
      sent.push("tx");
      return "0x1111111111111111111111111111111111111111111111111111111111111111";
    },
    getTransaction: async () => null,
    getBalance: async () => balance,
  };
  return { broadcast, sent };
}

const request = { dedupKey: "k", to: "0x0000000000000000000000000000000000000001", data: "0x" } as const;

describe("the relayer's balance floor", () => {
  it("refuses to broadcast below the floor, and says why", async () => {
    const { broadcast, sent } = client(5n);
    const relayer = new Relayer(broadcast, new MemoryRelayStore(), 100n, 10n);
    await expect(relayer.relay(request)).rejects.toBeInstanceOf(RelayerUnderfundedError);
    expect(sent).toEqual([]);
  });

  it("carries a stable code and status for the caller to report", async () => {
    const { broadcast } = client(5n);
    const error = await new Relayer(broadcast, new MemoryRelayStore(), 100n, 10n).relay(request).catch((e) => e);
    expect(error.publicCode).toBe("relayer_underfunded");
    expect(error.statusCode).toBe(503);
  });

  it("broadcasts at or above the floor", async () => {
    const { broadcast, sent } = client(10n);
    await new Relayer(broadcast, new MemoryRelayStore(), 100n, 10n).relay(request);
    expect(sent).toEqual(["tx"]);
  });

  it("has no floor unless one is set", async () => {
    const { broadcast, sent } = client(0n);
    await new Relayer(broadcast, new MemoryRelayStore()).relay(request);
    expect(sent).toEqual(["tx"]);
  });

  it("does not need the balance for a request it already broadcast", async () => {
    const { broadcast } = client(0n);
    const store = new MemoryRelayStore();
    store.save({ dedupKey: "k", nonce: 0, txHash: "0x2222222222222222222222222222222222222222222222222222222222222222" });
    broadcast.getTransaction = async () => ({ hash: "0x2222222222222222222222222222222222222222222222222222222222222222" });
    await expect(new Relayer(broadcast, store, 100n, 10n).relay(request)).resolves.toBe("0x2222222222222222222222222222222222222222222222222222222222222222");
  });
});
