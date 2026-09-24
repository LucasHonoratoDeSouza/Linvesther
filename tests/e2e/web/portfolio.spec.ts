import { expect, test, type Page } from "@playwright/test";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";

// Real browser (Playwright) coverage for apps/web/app/portfolio, the
// dashboard built on top of the real Binance connector
// (services/collector/binance). Previously signed in via a scripted
// injected wallet provider (see web/onboarding.spec.ts's prior
// version); A later change replaced portfolio's own duplicated wallet sign-in with
// the same passkey/vault credential flow as onboarding, so this file
// now signs in the same way via a CDP virtual authenticator.
//
// Both tests below run with FAKE Binance credentials on purpose: the
// point is to prove the real request pipeline (browser -> apps/api ->
// binance-worker -> real Binance API) reports a real failure rather
// than fabricating a connection, without needing a live account's
// secrets in CI. The credentialed, full-history path (real NAV,
// Sharpe/Sortino/CAGR, win rate) is covered by
// tests/e2e/binance-connect/binanceConnect.e2e.test.ts, which is
// skipped unless BINANCE_API_KEY/BINANCE_API_SECRET are present.

async function addVirtualAuthenticator(page: Page) {
  const client = await page.context().newCDPSession(page);
  await client.send("WebAuthn.enable");
  await client.send("WebAuthn.addVirtualAuthenticator", {
    options: {
      protocol: "ctap2",
      transport: "internal",
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
    },
  });
}

async function openBinanceForm(page: import("@playwright/test").Page) {
  await page.getByTestId("add-first-account-button").click();
  await page.getByTestId("broker-option-binance").click();
}

test("portfolio: signing in offers to connect a first account, and the popup lists the brokers that can be connected", async ({ page }) => {
  await addVirtualAuthenticator(page);
  await page.goto("/portfolio");

  await page.getByTestId("passkey-register-button").click();
  await page.getByTestId("add-first-account-button").click();

  const modal = page.getByTestId("add-account-modal");
  await expect(modal).toBeVisible();
  await expect(modal.getByTestId("broker-option-binance")).toBeVisible();
  // Every broker that can be connected is listed, crypto and stocks alike.
  await expect(modal.getByTestId("broker-option-coinbase")).toBeVisible();
  await expect(modal.getByTestId("broker-option-kraken")).toBeVisible();
  await expect(modal.getByTestId("broker-option-ibkr")).toBeVisible();

  await modal.getByTestId("broker-option-binance").click();
  // isOwner's self-ownership rule: the signed-in identity owns the
  // accountId equal to its own address, so the form is usable with no
  // prior bindAccount.
  await expect(page.getByTestId("connect-binance-button")).toBeVisible();
  await expect(page.getByTestId("connect-binance-button")).toBeDisabled();
});

test("portfolio: a fake Binance key produces a real, visible failure — never a fabricated connection", async ({ page }) => {
  await addVirtualAuthenticator(page);
  await page.goto("/portfolio");

  await page.getByTestId("passkey-register-button").click();
  await openBinanceForm(page);
  await expect(page.getByTestId("connect-binance-button")).toBeVisible();

  await page.getByTestId("binance-api-key-input").fill("not-a-real-key");
  await page.getByTestId("binance-api-secret-input").fill("not-a-real-secret");
  await page.getByTestId("connect-binance-button").click();

  await expect(page.getByTestId("error-banner")).toBeVisible();
  await expect(page.getByTestId("portfolio-dashboard")).toHaveCount(0);
});

// A browser wallet is stood in for by a script that announces itself the way real wallets
// do (EIP-6963). The key that signs lives in the test process, and every request the page
// makes to the wallet is recorded, so what the page asks of a wallet can be checked.
async function installWallet(page: Page, behaviour: "signs" | "declines") {
  const requested: string[] = [];
  const holder = privateKeyToAccount(generatePrivateKey());
  await page.exposeFunction("recordWalletRequest", (method: string) => requested.push(method));
  await page.exposeFunction("signWithHolder", (message: string) => holder.signMessage({ message }));
  await page.addInitScript(
    ({ address, behaviour }) => {
      const provider = {
        async request({ method, params }: { method: string; params?: unknown[] }) {
          await (window as unknown as { recordWalletRequest: (m: string) => void }).recordWalletRequest(method);
          if (method === "eth_requestAccounts") return [address];
          if (method === "eth_chainId") return "0x2105";
          if (method === "personal_sign") {
            if (behaviour === "declines") throw Object.assign(new Error("User rejected the request."), { code: 4001 });
            return (window as unknown as { signWithHolder: (m: string) => Promise<string> }).signWithHolder(String((params ?? [])[0]));
          }
          throw new Error(`unsupported ${method}`);
        },
      };
      const info = { uuid: "0b2d4d6a-6a54-4b1e-9c53-7d8c3c1a0001", name: "Test Wallet", icon: "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciLz4=", rdns: "test.wallet" };
      const announce = () => window.dispatchEvent(new CustomEvent("eip6963:announceProvider", { detail: Object.freeze({ info, provider }) }));
      window.addEventListener("eip6963:requestProvider", announce);
      announce();
    },
    { address: holder.address, behaviour },
  );
  return { requested, address: holder.address };
}

test("wallet: choosing a wallet and signing links it without ever asking the wallet for a transaction", async ({ page }) => {
  const wallet = await installWallet(page, "signs");
  await addVirtualAuthenticator(page);
  await page.goto("/portfolio");
  await page.getByTestId("passkey-register-button").click();
  await page.getByTestId("add-first-account-button").click();
  await page.getByTestId("broker-option-wallet").click();

  const connection = page.waitForRequest((request) => request.url().endsWith("/wallet-connection") && request.method() === "POST");
  await page.getByTestId("wallet-option-test.wallet").click();
  const sent = (await connection).postDataJSON() as { message: string; signature: string };

  // The signed message names this host and this one task, and the address travels only inside it.
  expect(sent.message).toContain("does not move funds");
  expect(sent.signature).toMatch(/^0x[0-9a-f]+$/i);
  expect(wallet.requested).toEqual(["eth_requestAccounts", "eth_chainId", "personal_sign"]);

  // The proof was accepted: whatever happens next (reading the chain needs the operator's
  // explorer keys) is not a refusal of the signature.
  const answer = await (await connection).response();
  expect((await answer!.json()).error).not.toBe("wallet_proof_failed");
  await expect(page.getByText(wallet.address, { exact: false })).toHaveCount(0);
});

test("wallet: closing the wallet's prompt connects nothing and says so", async ({ page }) => {
  await installWallet(page, "declines");
  await addVirtualAuthenticator(page);
  await page.goto("/portfolio");
  await page.getByTestId("passkey-register-button").click();
  await page.getByTestId("add-first-account-button").click();
  await page.getByTestId("broker-option-wallet").click();

  let connectionAttempts = 0;
  page.on("request", (request) => request.url().endsWith("/wallet-connection") && connectionAttempts++);
  await page.getByTestId("wallet-option-test.wallet").click();
  await expect(page.getByTestId("wallet-problem")).toContainText("Nothing was connected");
  expect(connectionAttempts).toBe(0);
});

test("wallet: with no wallet in the browser, the person is told what to install", async ({ page }) => {
  await addVirtualAuthenticator(page);
  await page.goto("/portfolio");
  await page.getByTestId("passkey-register-button").click();
  await page.getByTestId("add-first-account-button").click();
  await page.getByTestId("broker-option-wallet").click();
  await expect(page.getByTestId("wallet-none")).toBeVisible();
});
