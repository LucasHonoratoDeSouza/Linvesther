import { getAddress, isAddress, verifyMessage } from "viem";
import { createSiweMessage, parseSiweMessage } from "viem/siwe";
import type { NonceStore } from "../auth/nonceStore.js";

/** Checks a signature made by a smart-contract wallet (ERC-1271) on a given network. */
export type ContractSignatureVerifier = (input: { chainId: number; address: `0x${string}`; message: string; signature: `0x${string}` }) => Promise<boolean>;

export interface WalletProofOptions {
  /** The host this service is served from; a proof made for any other host is refused. */
  domain: string;
  /** The web origin shown in the message. */
  uri: string;
  nonces: NonceStore;
  /** Absent: only externally-owned accounts can be proven. */
  verifyContract?: ContractSignatureVerifier;
  now: () => Date;
}

/** Why a proof was refused. Never says more than which check failed. */
export type ProofFailure = "malformed" | "wrong_domain" | "wrong_account" | "expired" | "replayed" | "bad_signature" | "unsupported_wallet";

const CHALLENGE_TTL_MS = 5 * 60_000;
const STATEMENT = "Link this wallet to your Linvesther account to read its balances and history. This signature does not move funds or grant any access to them.";

/** The message a person signs to show they hold an address: a Sign-In with Ethereum
 * message bound to this host, this account, one network and a single-use nonce.
 * Signing it is not a transaction and costs nothing. */
export async function issueWalletChallenge(options: WalletProofOptions, input: { accountId: string; address: string; chainId: number }): Promise<string> {
  const issuedAt = options.now();
  return createSiweMessage({
    domain: options.domain,
    address: getAddress(input.address),
    statement: STATEMENT,
    uri: options.uri,
    version: "1",
    chainId: input.chainId,
    nonce: Buffer.from(await options.nonces.issue()).toString("hex"), // a SIWE nonce must be alphanumeric
    requestId: input.accountId,
    issuedAt,
    expirationTime: new Date(issuedAt.getTime() + CHALLENGE_TTL_MS),
  });
}

/** Accepts a signed challenge for `accountId` and returns the address that signed it.
 * The nonce is spent even when the signature is then refused, so a challenge is tried once. */
export async function verifyWalletProof(
  options: WalletProofOptions,
  input: { accountId: string; message: string; signature: string },
): Promise<{ ok: true; address: `0x${string}` } | { ok: false; reason: ProofFailure }> {
  const fail = (reason: ProofFailure) => ({ ok: false as const, reason });
  const parsed = parseSiweMessage(input.message);
  if (!parsed.address || !parsed.nonce || !parsed.domain || !parsed.chainId || !isAddress(parsed.address, { strict: false }) || !/^0x[0-9a-fA-F]+$/.test(input.signature)) {
    return fail("malformed");
  }
  if (parsed.statement !== STATEMENT) return fail("malformed");
  if (parsed.domain !== options.domain) return fail("wrong_domain");
  if (parsed.requestId !== input.accountId) return fail("wrong_account");
  const now = options.now();
  if (!parsed.expirationTime || parsed.expirationTime.getTime() <= now.getTime()) return fail("expired");
  // The message must be exactly what was issued for these fields: re-render it and compare,
  // so nothing outside what was checked here could have been changed.
  const canonical = createSiweMessage({ ...parsed, address: getAddress(parsed.address), domain: parsed.domain, uri: parsed.uri ?? options.uri, version: "1", chainId: parsed.chainId, nonce: parsed.nonce });
  if (canonical !== input.message) return fail("malformed");
  if (!(await options.nonces.consume(Buffer.from(parsed.nonce, "hex").toString()))) return fail("replayed");

  const address = getAddress(parsed.address);
  const signature = input.signature as `0x${string}`;
  if (await verifyMessage({ address, message: input.message, signature }).catch(() => false)) {
    return { ok: true, address };
  }
  if (!options.verifyContract) return fail("unsupported_wallet");
  const holds = await options.verifyContract({ chainId: parsed.chainId, address, message: input.message, signature }).catch(() => false);
  return holds ? { ok: true, address } : fail("bad_signature");
}

/** Checks smart-contract wallet signatures against the node configured for that network in
 * `WALLET_RPC_URL_<chainId>` (the same setting the worker reads balances with). A network
 * without one is not verifiable, so a contract wallet there is refused. */
export function contractVerifierFromEnv(env: Record<string, string | undefined>): ContractSignatureVerifier {
  return async ({ chainId, address, message, signature }) => {
    const url = env[`WALLET_RPC_URL_${chainId}`];
    if (!url) return false;
    const { createPublicClient, http } = await import("viem");
    return createPublicClient({ transport: http(url) }).verifyMessage({ address, message, signature });
  };
}
