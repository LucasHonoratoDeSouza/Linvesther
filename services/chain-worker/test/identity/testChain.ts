// Shared Anvil + real-contract-deployment harness for the identity
// module's integration tests (events, relayFlow, postgresProjectionStore)
// — every one of them needs the same real IdentityRegistry/AccountRegistry
// deployed on a real local node, so this is extracted once rather than
// repeated per test file.
import { readFileSync } from "node:fs";
import path from "node:path";
import { type ChildProcess, spawn } from "node:child_process";
import { createPublicClient, createWalletClient, http, type Hex } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import {
  identityRegistryAbi,
  accountRegistryAbi,
  accountFactoryAbi,
  webAuthnAccountAbi,
  p256VaultAccountAbi,
} from "../../src/identity/abi.js";

const CONTRACTS_OUT = path.resolve(import.meta.dirname, "../../../../contracts/out");

// Anvil's well-known default account #0 — a public test-only fixture.
export const DEPLOYER_KEY = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80" as const;

function bytecode(contractFile: string, contractName: string): Hex {
  const artifactPath = path.join(CONTRACTS_OUT, `${contractFile}.sol`, `${contractName}.json`);
  const artifact = JSON.parse(readFileSync(artifactPath, "utf8")) as { bytecode: { object: Hex } };
  return artifact.bytecode.object;
}

async function waitForRpc(rpcUrl: string): Promise<void> {
  for (let attempt = 0; attempt < 50; attempt++) {
    try {
      await fetch(rpcUrl, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "eth_blockNumber", params: [] }),
      });
      return;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }
  throw new Error("anvil did not become ready in time");
}

export interface TestChain {
  rpcUrl: string;
  anvil: ChildProcess;
  identityRegistry: `0x${string}`;
  accountRegistry: `0x${string}`;
  accountFactory: `0x${string}`;
  webAuthnImplementation: `0x${string}`;
  p256VaultImplementation: `0x${string}`;
}

/** Spawns a real Anvil node on `port` and deploys the real, compiled
 * IdentityRegistry/AccountRegistry/AccountFactory (+ its two account
 * implementations) — never a stub or a mock ABI. */
export async function startTestChain(port: number): Promise<TestChain> {
  const rpcUrl = `http://127.0.0.1:${port}`;
  const anvil = spawn("anvil", ["--port", String(port), "--silent"], { stdio: "ignore" });
  await waitForRpc(rpcUrl);

  const account = privateKeyToAccount(DEPLOYER_KEY);
  const wallet = createWalletClient({ account, transport: http(rpcUrl) });
  const publicClient = createPublicClient({ transport: http(rpcUrl) });

  async function deploy(contractFile: string, contractName: string, abi: readonly unknown[], args: unknown[] = []) {
    const hash = await wallet.deployContract({
      chain: undefined,
      abi: abi as never,
      bytecode: bytecode(contractFile, contractName),
      args: args as never,
    });
    const receipt = await publicClient.waitForTransactionReceipt({ hash });
    if (!receipt.contractAddress) throw new Error(`${contractName} deployment produced no address`);
    return receipt.contractAddress;
  }

  const identityRegistry = await deploy("IdentityRegistry", "IdentityRegistry", identityRegistryAbi);
  const accountRegistry = await deploy("AccountRegistry", "AccountRegistry", accountRegistryAbi, [identityRegistry]);
  const webAuthnImplementation = await deploy("WebAuthnAccount", "WebAuthnAccount", webAuthnAccountAbi);
  const p256VaultImplementation = await deploy("P256VaultAccount", "P256VaultAccount", p256VaultAccountAbi);
  const accountFactory = await deploy("AccountFactory", "AccountFactory", accountFactoryAbi, [
    webAuthnImplementation,
    p256VaultImplementation,
  ]);

  return { rpcUrl, anvil, identityRegistry, accountRegistry, accountFactory, webAuthnImplementation, p256VaultImplementation };
}

export function stopTestChain(chain: TestChain): void {
  chain.anvil.kill();
}

/** Signs an `IdentityRegistry` EIP-712 command with `privateKey` — the
 * exact domain/typehash IdentityRegistry.sol itself defines, not a
 * hand-guessed shape. Used by tests that need a real owner signature
 * (createTrack, proposeOwnerRotation, confirmOwnerRotation). */
export async function signIdentityRegistryCommand(
  chain: TestChain,
  privateKey: Hex,
  primaryType: "CreateTrack" | "ProposeOwnerRotation" | "ConfirmOwnerRotation",
  message: Record<string, unknown>,
): Promise<Hex> {
  const account = privateKeyToAccount(privateKey);
  const domain = { name: "LinvestherZK-IdentityRegistry", version: "1", chainId: 31337, verifyingContract: chain.identityRegistry };
  const types = {
    CreateTrack: [
      { name: "identityId", type: "bytes32" },
      { name: "financialProfileId", type: "bytes32" },
      { name: "denominationCommitment", type: "bytes32" },
      { name: "ownerEpoch", type: "uint64" },
      { name: "nonce", type: "uint64" },
      { name: "deadline", type: "uint256" },
    ],
    ProposeOwnerRotation: [
      { name: "identityId", type: "bytes32" },
      { name: "newOwner", type: "address" },
      { name: "ownerEpoch", type: "uint64" },
      { name: "nonce", type: "uint64" },
      { name: "deadline", type: "uint256" },
    ],
    ConfirmOwnerRotation: [
      { name: "identityId", type: "bytes32" },
      { name: "newOwner", type: "address" },
      { name: "ownerEpoch", type: "uint64" },
      { name: "nonce", type: "uint64" },
      { name: "deadline", type: "uint256" },
    ],
  } as const;
  return account.signTypedData({ domain, types, primaryType, message: message as never });
}

export interface TestVaultKey {
  qx: `0x${string}`;
  qy: `0x${string}`;
  privateKey: CryptoKey;
}

function bytesToHex(bytes: Uint8Array): `0x${string}` {
  return `0x${Array.from(bytes).map((b) => b.toString(16).padStart(2, "0")).join("")}`;
}

function hexToBytes(hex: `0x${string}`): Uint8Array<ArrayBuffer> {
  const clean = hex.slice(2);
  const bytes = new Uint8Array(clean.length / 2);
  for (let i = 0; i < bytes.length; i++) bytes[i] = Number.parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  return bytes;
}

/** Generates a real P-256 keypair via SubtleCrypto — the exact same
 * primitive `apps/web/lib/vault.ts` uses, not a Foundry cheatcode. */
export async function generateVaultKey(): Promise<TestVaultKey> {
  const keyPair = await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, ["sign", "verify"]);
  const raw = new Uint8Array(await crypto.subtle.exportKey("raw", keyPair.publicKey));
  return { qx: bytesToHex(raw.slice(1, 33)), qy: bytesToHex(raw.slice(33, 65)), privateKey: keyPair.privateKey };
}

// secp256r1 (P-256) curve order — P256.sol's own `N` constant.
const P256_N = 0xffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551n;

/** Signs `digest` (a 32-byte EIP-712 digest, hex-encoded) the way a
 * real vault does: SubtleCrypto's ECDSA sign always SHA-256-hashes its
 * input first (no raw-digest mode exists), so this signs the raw
 * digest bytes directly and lets SubtleCrypto do that hash — matching
 * what `P256VaultAccount._rawSignatureValidation`'s sha256 re-wrap
 * expects on the other end. `s` is normalized to its canonical low
 * form (`min(s, N-s)`) afterward — SubtleCrypto doesn't guarantee this,
 * but `P256.sol`'s `_isProperSignature` requires it (rejects `s > N/2`
 * as non-canonical, the standard malleability guard). */
export async function signVaultDigest(privateKey: CryptoKey, digest: `0x${string}`): Promise<`0x${string}`> {
  const signature = new Uint8Array(await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, privateKey, hexToBytes(digest)));
  const r = signature.slice(0, 32);
  let s = BigInt(bytesToHex(signature.slice(32, 64)));
  if (s > P256_N / 2n) {
    s = P256_N - s;
  }
  const sHex = s.toString(16).padStart(64, "0");
  return `0x${bytesToHex(r).slice(2)}${sHex}`;
}

/** Same as `signIdentityRegistryCommand`, for `AccountRegistry`. */
export async function signAccountRegistryCommand(
  chain: TestChain,
  privateKey: Hex,
  primaryType: "RegisterAccount" | "ActivateAccount" | "RemoveAccount",
  message: Record<string, unknown>,
): Promise<Hex> {
  const account = privateKeyToAccount(privateKey);
  const domain = { name: "LinvestherZK-AccountRegistry", version: "1", chainId: 31337, verifyingContract: chain.accountRegistry };
  const types = {
    RegisterAccount: [
      { name: "trackId", type: "bytes32" },
      { name: "venueId", type: "bytes32" },
      { name: "authenticatedIdCommitment", type: "bytes32" },
      { name: "environment", type: "uint8" },
      { name: "ownerEpoch", type: "uint64" },
      { name: "nonce", type: "uint64" },
      { name: "deadline", type: "uint256" },
    ],
    ActivateAccount: [
      { name: "accountId", type: "bytes32" },
      { name: "eligibleFrom", type: "uint64" },
      { name: "reasonHash", type: "bytes32" },
      { name: "ownerEpoch", type: "uint64" },
      { name: "nonce", type: "uint64" },
      { name: "deadline", type: "uint256" },
    ],
    RemoveAccount: [
      { name: "accountId", type: "bytes32" },
      { name: "reasonHash", type: "bytes32" },
      { name: "ownerEpoch", type: "uint64" },
      { name: "nonce", type: "uint64" },
      { name: "deadline", type: "uint256" },
    ],
  } as const;
  return account.signTypedData({ domain, types, primaryType, message: message as never });
}
