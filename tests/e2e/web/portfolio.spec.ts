import { expect, test, type Page } from "@playwright/test";

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
