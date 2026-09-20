// Idempotent relayer, per the operations design:
// "Efeito remoto é reconciliado antes do retry: consultar hash/nonce/tx
// em vez de transmitir novo payload" and the EVM adapter specification:
// "Relayer é substituível; SDK permite gerar comando e transmitir
// diretamente."

/** A pre-signed command's calldata, opaque to the relayer: the relayer
 * never constructs or alters `data` — it only submits exactly the bytes
 * it was given, the same bytes any other relayer or the owner's own SDK
 * could submit directly. */
export interface RelayRequest {
  /** Derived from the signed command (e.g. a digest of payload +
   * signature), stable across retries of the same logical broadcast. */
  dedupKey: string;
  to: `0x${string}`;
  data: `0x${string}`;
  value?: bigint;
}

export interface RelayRecord {
  dedupKey: string;
  nonce: number;
  txHash: `0x${string}`;
}

export interface RelayStore {
  find(dedupKey: string): RelayRecord | undefined;
  save(record: RelayRecord): void;
}
