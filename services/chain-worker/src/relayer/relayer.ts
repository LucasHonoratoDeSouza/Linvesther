import type { RelayRequest, RelayStore } from "./types.js";

export interface BroadcastClient {
  address(): `0x${string}`;
  getPendingNonce(address: `0x${string}`): Promise<number>;
  sendTransaction(tx: { to: `0x${string}`; data: `0x${string}`; value: bigint; nonce: number }): Promise<`0x${string}`>;
  /** `null` if the transaction is unknown to the node (e.g. dropped from
   * the mempool and never mined) — the signal the relayer uses to decide
   * whether a retry needs to resend at all. */
  getTransaction(hash: `0x${string}`): Promise<{ hash: `0x${string}` } | null>;
  getBalance(address: `0x${string}`): Promise<bigint>;
}

// 0.01 ETH — comfortably above a single relayed call's gas cost on an
// L2 like Base, low enough to warn well before the relayer actually
// runs dry. Not a hard limit: relay() still attempts the transaction
// and lets a real out-of-gas failure surface on its own.
const DEFAULT_LOW_BALANCE_THRESHOLD_WEI = 10_000_000_000_000_000n;

/** Thrown instead of broadcasting when the relayer cannot pay for the
 * transaction. A clear refusal up front, so a person is told to try later
 * instead of watching a transaction fail halfway. */
export class RelayerUnderfundedError extends Error {
  /** A stable code and status for a caller to report. */
  readonly publicCode = "relayer_underfunded";
  readonly statusCode = 503;

  constructor(readonly balanceWei: bigint, readonly minBalanceWei: bigint) {
    super(`the relayer's balance (${balanceWei} wei) is below its floor (${minBalanceWei} wei)`);
    this.name = "RelayerUnderfundedError";
  }
}

/** Broadcasts `RelayRequest`s idempotently: a repeated call for a
 * `dedupKey` already broadcast recovers the existing tx/nonce rather
 * than transmitting a new payload — see
 * the operations design's "Efeito remoto é
 * reconciliado antes do retry". The relayer never constructs or edits
 * `request.data`; it submits exactly the bytes it was given, so any
 * other relayer — or the owner's own SDK, calling `sendTransaction`
 * directly — can submit the identical payload with the identical
 * effect. This class holds no privilege the direct path lacks.
 */
export class Relayer {
  constructor(
    private readonly client: BroadcastClient,
    private readonly store: RelayStore,
    private readonly lowBalanceThresholdWei: bigint = DEFAULT_LOW_BALANCE_THRESHOLD_WEI,
    /** Below this the relayer refuses to broadcast. 0 disables the floor. */
    private readonly minBalanceWei: bigint = 0n,
  ) {}

  async relay(request: RelayRequest): Promise<`0x${string}`> {
    const existing = this.store.find(request.dedupKey);
    if (existing) {
      const onChain = await this.client.getTransaction(existing.txHash);
      if (onChain) {
        return existing.txHash;
      }
    }

    const address = this.client.address();
    const balance = await this.client.getBalance(address);
    if (balance < this.minBalanceWei) {
      throw new RelayerUnderfundedError(balance, this.minBalanceWei);
    }
    if (balance < this.lowBalanceThresholdWei) {
      console.warn(`relayer ${address} balance is low (${balance} wei, threshold ${this.lowBalanceThresholdWei} wei) — it may soon be unable to relay`);
    }

    const nonce = await this.client.getPendingNonce(address);
    const txHash = await this.client.sendTransaction({ to: request.to, data: request.data, value: request.value ?? 0n, nonce });
    this.store.save({ dedupKey: request.dedupKey, nonce, txHash });
    return txHash;
  }
}
