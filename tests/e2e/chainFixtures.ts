import { ADMIN_DATABASE_URL, databaseUrl } from "./testDatabase.js";

// Shared between globalSetup.ts (deploys and verifies these addresses)
// and playwright.config.ts (passes them to apps/api's webServer as
// static env vars — Playwright evaluates the config once, before
// globalSetup runs, so these can't be discovered dynamically at
// config-load time; they're deterministic CREATE addresses instead,
// verified against the real deploy in globalSetup.ts).
export const ANVIL_PORT = Number(process.env.E2E_ANVIL_PORT ?? 8901);
// Ports well away from a development instance (4300/4301), so the suite can run next to one.
export const WEB_PORT = Number(process.env.E2E_WEB_PORT ?? 14300);
export const API_PORT = Number(process.env.E2E_API_PORT ?? 14301);

// A database of this run's own, created by globalSetup and dropped afterwards.
// Named once, in the first process to load this file, so the config, the
// setup and the servers it starts all agree on it.
process.env.E2E_DATABASE_NAME ??= `linvesther_e2e_${process.pid}`;
export const E2E_DATABASE_NAME = process.env.E2E_DATABASE_NAME;
export const DATABASE_URL = databaseUrl(ADMIN_DATABASE_URL, E2E_DATABASE_NAME);

// Anvil's well-known default account #0 — a public test-only fixture,
// not a secret, used by every Foundry/Hardhat local devnet.
export const DEPLOYER_KEY = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80" as const;

// CREATE address = f(sender, nonce) — deterministic given the deployer
// above and the exact deploy order globalSetup.ts uses on a fresh
// chain (IdentityRegistry=0, AccountRegistry=1, WebAuthnAccount=2,
// P256VaultAccount=3, AccountFactory=4), computed via viem's
// getContractAddress and confirmed against a real local deploy.
export const EXPECTED_IDENTITY_REGISTRY = "0x5FbDB2315678afecb367f032d93F642f64180aa3" as const;
export const EXPECTED_ACCOUNT_REGISTRY = "0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512" as const;
export const EXPECTED_ACCOUNT_FACTORY = "0xDc64a140Aa3E981100a9becA4E685f962f0cF6C9" as const;
