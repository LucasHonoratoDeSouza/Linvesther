// Real ChainClient backed by viem, per the architecture design: "Integração EVM |
// viem, TypeScript | Cliente tipado."

import { type PublicClient, createPublicClient, http } from "viem";
import type { BlockHeader, ChainClient, ChainSnapshot } from "./types.js";

function toHeader(block: { number: bigint | null; hash: `0x${string}` | null; parentHash: `0x${string}` }): BlockHeader {
  if (block.number === null || block.hash === null) {
    throw new Error("pending block has no number/hash yet");
  }
  return { number: block.number, hash: block.hash, parentHash: block.parentHash };
}

export class ViemChainClient implements ChainClient {
  private readonly client: PublicClient;

  constructor(rpcUrl: string) {
    this.client = createPublicClient({ transport: http(rpcUrl) });
  }

  async getSnapshot(): Promise<ChainSnapshot> {
    const [included, safe, finalized] = await Promise.all([
      this.client.getBlock({ blockTag: "latest" }),
      this.client.getBlock({ blockTag: "safe" }),
      this.client.getBlock({ blockTag: "finalized" }),
    ]);
    return { included: toHeader(included), safe: toHeader(safe), finalized: toHeader(finalized) };
  }

  async getBlockByNumber(number: bigint): Promise<BlockHeader> {
    const block = await this.client.getBlock({ blockNumber: number });
    return toHeader(block);
  }
}
