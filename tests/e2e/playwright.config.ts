import { defineConfig } from "@playwright/test";
import { ANVIL_PORT, DATABASE_URL, DEPLOYER_KEY, EXPECTED_ACCOUNT_FACTORY, EXPECTED_ACCOUNT_REGISTRY, EXPECTED_IDENTITY_REGISTRY } from "./chainFixtures.js";

// Drives the real apps/web app in a real browser against a real
// apps/api instance — both started fresh for the test run, not stubbed.
// globalSetup deploys the real on-chain identity contracts to a real
// local Anvil (see chainFixtures.ts) before apps/api's webServer below
// starts, so /identities* is real end to end, not just the rest of the
// app.
export default defineConfig({
  testDir: "./web",
  timeout: 30_000,
  fullyParallel: false,
  workers: 1,
  globalSetup: "./globalSetup.ts",
  use: {
    // Literal "localhost" (not 127.0.0.1): a real WebAuthn ceremony's
    // clientDataJSON.origin is the page's exact browser origin, which
    // must match apps/api's WEBAUTHN_RP_ID/WEBAUTHN_ORIGIN below
    // (see apps/api/src/auth/webauthn.ts and the design notes).
    baseURL: "http://localhost:4300",
  },
  webServer: [
    {
      command: "pnpm --filter @linvestherzk/api start",
      port: 4301,
      cwd: "..",
      reuseExistingServer: false,
      timeout: 20_000,
      env: {
        DEMO_SEED: "1",
        SIWE_DOMAIN: "localhost",
        CORS_ORIGINS: "http://localhost:4300",
        WEBAUTHN_RP_ID: "localhost",
        WEBAUTHN_ORIGIN: "http://localhost:4300",
        CHAIN_RPC_URL: `http://127.0.0.1:${ANVIL_PORT}`,
        CHAIN_RELAYER_PRIVATE_KEY: DEPLOYER_KEY,
        IDENTITY_REGISTRY_ADDRESS: EXPECTED_IDENTITY_REGISTRY,
        ACCOUNT_REGISTRY_ADDRESS: EXPECTED_ACCOUNT_REGISTRY,
        ACCOUNT_FACTORY_ADDRESS: EXPECTED_ACCOUNT_FACTORY,
        DATABASE_URL,
      },
    },
    {
      command: "pnpm --filter @linvestherzk/web dev",
      port: 4300,
      cwd: "..",
      reuseExistingServer: false,
      timeout: 30_000,
      env: {
        NEXT_PUBLIC_API_URL: "http://localhost:4301",
        NEXT_PUBLIC_SIWE_DOMAIN: "localhost",
      },
    },
  ],
});
