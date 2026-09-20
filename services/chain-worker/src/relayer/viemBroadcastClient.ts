import { type Account, type PublicClient, type WalletClient, createPublicClient, createWalletClient, http } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import type { BroadcastClient } from "./relayer.js";

export class ViemBroadcastClient implements BroadcastClient {
  private readonly account: Account;
  private readonly wallet: WalletClient;
  private readonly publicClient: PublicClient;

  constructor(rpcUrl: string, privateKey: `0x${string}`) {
    this.account = privateKeyToAccount(privateKey);
    this.wallet = createWalletClient({ account: this.account, transport: http(rpcUrl) });
    this.publicClient = createPublicClient({ transport: http(rpcUrl) });
  }

  address(): `0x${string}` {
    return this.account.address;
  }

  async getPendingNonce(address: `0x${string}`): Promise<number> {
    return this.publicClient.getTransactionCount({ address, blockTag: "pending" });
  }

  async getBalance(address: `0x${string}`): Promise<bigint> {
    return this.publicClient.getBalance({ address });
  }

  async sendTransaction(tx: { to: `0x${string}`; data: `0x${string}`; value: bigint; nonce: number }): Promise<`0x${string}`> {
    // A fixed, generous gas limit — well above what any of the relayed
    // identity/account calls actually consume — instead of leaving gas
    // unset (which makes viem call eth_estimateGas first). On a
    // load-balanced public RPC (e.g. Base Sepolia's shared endpoint)
    // that pre-flight estimate can hit a node that hasn't yet caught up
    // to a transaction this same relay sequence just had mined moments
    // earlier, producing a spurious revert for a call that would
    // actually succeed. Skipping estimation removes that race; the real
    // network execution at mining time remains the actual arbiter of
    // success, exactly what waitForSuccessfulReceipt already checks.
    return this.wallet.sendTransaction({
      account: this.account,
      chain: undefined,
      to: tx.to,
      data: tx.data,
      value: tx.value,
      nonce: tx.nonce,
      gas: 600_000n,
    });
  }

  async getTransaction(hash: `0x${string}`): Promise<{ hash: `0x${string}` } | null> {
    try {
      const tx = await this.publicClient.getTransaction({ hash });
      return { hash: tx.hash };
    } catch {
      return null;
    }
  }
}
