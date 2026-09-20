// Spawns a real local Anvil node and deploys the real, compiled
// IdentityRegistry/AccountRegistry/AccountFactory (+ its two account
// implementations) before apps/api's webServer starts — playwright.config.ts's
// CHAIN_RPC_URL/contract-address env vars for that webServer are the
// deterministic CREATE addresses this exact deploy sequence produces
// from Anvil's default account #0 on a fresh chain (verified below,
// not just assumed).
import { readFileSync } from "node:fs";
import path from "node:path";
import { type ChildProcess, spawn } from "node:child_process";
import { createPublicClient, createWalletClient, http, type Hex } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { Pool } from "pg";
import { dropDatabase } from "./testDatabase.js";
import {
  accountFactoryAbi,
  accountRegistryAbi,
  identityRegistryAbi,
  runMigrations,
} from "@linvestherzk/chain-worker";
import {
  ANVIL_PORT,
  DATABASE_URL,
  E2E_DATABASE_NAME,
  DEPLOYER_KEY,
  EXPECTED_ACCOUNT_FACTORY,
  EXPECTED_ACCOUNT_REGISTRY,
  EXPECTED_IDENTITY_REGISTRY,
} from "./chainFixtures.js";

const RPC_URL = `http://127.0.0.1:${ANVIL_PORT}`;
const CONTRACTS_OUT = path.resolve(import.meta.dirname, "../../contracts/out");

function bytecode(contractFile: string, contractName: string): Hex {
  const artifact = JSON.parse(readFileSync(path.join(CONTRACTS_OUT, `${contractFile}.sol`, `${contractName}.json`), "utf8")) as {
    bytecode: { object: Hex };
  };
  return artifact.bytecode.object;
}

async function waitForRpc(): Promise<void> {
  for (let attempt = 0; attempt < 50; attempt++) {
    try {
      await fetch(RPC_URL, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "eth_blockNumber", params: [] }) });
      return;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }
  throw new Error("anvil did not become ready in time");
}

let anvil: ChildProcess;

export default async function globalSetup(): Promise<() => Promise<void>> {
  anvil = spawn("anvil", ["--port", String(ANVIL_PORT), "--silent"], { stdio: "ignore", detached: true });
  await waitForRpc();

  const deployerAccount = privateKeyToAccount(DEPLOYER_KEY);
  const wallet = createWalletClient({ account: deployerAccount, transport: http(RPC_URL) });
  const publicClient = createPublicClient({ transport: http(RPC_URL) });

  async function deploy(contractFile: string, contractName: string, abi: readonly unknown[], args: unknown[] = []) {
    const hash = await wallet.deployContract({ chain: undefined, abi: abi as never, bytecode: bytecode(contractFile, contractName), args: args as never });
    const receipt = await publicClient.waitForTransactionReceipt({ hash });
    if (!receipt.contractAddress) throw new Error(`${contractName} deployment produced no address`);
    return receipt.contractAddress;
  }

  const identityRegistry = await deploy("IdentityRegistry", "IdentityRegistry", identityRegistryAbi);
  const accountRegistry = await deploy("AccountRegistry", "AccountRegistry", accountRegistryAbi, [identityRegistry]);
  const webAuthnImpl = await deploy("WebAuthnAccount", "WebAuthnAccount", []);
  const p256VaultImpl = await deploy("P256VaultAccount", "P256VaultAccount", []);
  const accountFactory = await deploy("AccountFactory", "AccountFactory", accountFactoryAbi, [webAuthnImpl, p256VaultImpl]);

  // Fails loudly rather than silently handing apps/api the wrong
  // addresses if Anvil's nonce/CREATE behavior ever changes.
  if (identityRegistry.toLowerCase() !== EXPECTED_IDENTITY_REGISTRY.toLowerCase()) {
    throw new Error(`IdentityRegistry deployed to ${identityRegistry}, expected ${EXPECTED_IDENTITY_REGISTRY} — update chainFixtures.ts`);
  }
  if (accountRegistry.toLowerCase() !== EXPECTED_ACCOUNT_REGISTRY.toLowerCase()) {
    throw new Error(`AccountRegistry deployed to ${accountRegistry}, expected ${EXPECTED_ACCOUNT_REGISTRY} — update chainFixtures.ts`);
  }
  if (accountFactory.toLowerCase() !== EXPECTED_ACCOUNT_FACTORY.toLowerCase()) {
    throw new Error(`AccountFactory deployed to ${accountFactory}, expected ${EXPECTED_ACCOUNT_FACTORY} — update chainFixtures.ts`);
  }

  // This run's own database, created by prepareDatabase.ts before the API
  // started: nothing here can touch data that already exists.
  const pool = new Pool({ connectionString: DATABASE_URL });
  await runMigrations(pool);
  await pool.end();

  return async () => {
    anvil.kill();
    await dropDatabase(E2E_DATABASE_NAME);
  };
}
