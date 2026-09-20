import { defineConfig } from "@playwright/test";
import { ANVIL_PORT, API_PORT, DATABASE_URL, WEB_PORT, DEPLOYER_KEY, EXPECTED_ACCOUNT_FACTORY, EXPECTED_ACCOUNT_REGISTRY, EXPECTED_IDENTITY_REGISTRY } from "./chainFixtures.js";

// Drives the real apps/web app in a real browser against a real
// apps/api instance — both started fresh for the test run, not stubbed.
// globalSetup deploys the real on-chain identity contracts to a real
// local Anvil (see chainFixtures.ts) before apps/api's webServer below
// starts, so /identities* is real end to end, not just the rest of the
// app.
export default defineConfig({
  testDir: "./web",
  // Only the browser specs. The other files in ./web are Vitest unit tests
  // (*.e2e.test.ts), which Playwright must not try to run.
  testMatch: /.*\.spec\.ts/,
  timeout: 30_000,
  fullyParallel: false,
  workers: 1,
  globalSetup: "./globalSetup.ts",
  use: {
    // Literal "localhost" (not 127.0.0.1): a real WebAuthn ceremony's
    // clientDataJSON.origin is the page's exact browser origin, which
    // must match apps/api's WEBAUTHN_RP_ID/WEBAUTHN_ORIGIN below
    // (see apps/api/src/auth/webauthn.ts and the design notes).
    baseURL: `http://localhost:${WEB_PORT}`,
  },
  webServer: [
    {
      command: "pnpm --filter e2e-tests exec tsx prepareDatabase.ts && pnpm --filter @linvestherzk/api start",
      port: API_PORT,
      cwd: "..",
      reuseExistingServer: false,
      timeout: 20_000,
      env: {
        PORT: String(API_PORT),
        DEMO_SEED: "1",
        SIWE_DOMAIN: "localhost",
        CORS_ORIGINS: `http://localhost:${WEB_PORT}`,
        WEBAUTHN_RP_ID: "localhost",
        WEBAUTHN_ORIGIN: `http://localhost:${WEB_PORT}`,
        CHAIN_RPC_URL: `http://127.0.0.1:${ANVIL_PORT}`,
        CHAIN_RELAYER_PRIVATE_KEY: DEPLOYER_KEY,
        IDENTITY_REGISTRY_ADDRESS: EXPECTED_IDENTITY_REGISTRY,
        ACCOUNT_REGISTRY_ADDRESS: EXPECTED_ACCOUNT_REGISTRY,
        ACCOUNT_FACTORY_ADDRESS: EXPECTED_ACCOUNT_FACTORY,
        DATABASE_URL,
        // The chain here is a fresh local Anvil: never inherit a real deployment's block or registry from the caller's environment.
        // The browser suite signs in far faster than a person, so it runs with the per-client limits scaled up.
        TRAFFIC_LIMIT_MULTIPLIER: "1000",
        CHAIN_DEPLOY_BLOCK: "0",
        PROFILE_REGISTRY_ADDRESS: "",
      },
    },
    {
      command: `pnpm --filter @linvestherzk/web exec next dev --port ${WEB_PORT}`,
      port: WEB_PORT,
      cwd: "..",
      reuseExistingServer: false,
      timeout: 30_000,
      env: {
        NEXT_PUBLIC_API_URL: `http://localhost:${API_PORT}`,
        NEXT_PUBLIC_SIWE_DOMAIN: "localhost",
      },
    },
  ],
});
