import { expect, test, type Page } from "@playwright/test";

// Real-browser coverage for the passkey/vault identity flow
//, using the CDP WebAuthn protocol
// domain's virtual authenticator — the standard way to exercise a real
// navigator.credentials ceremony without a physical authenticator.
//
// This file lives under ./web because that is the directory Playwright scans
// (`pnpm --filter e2e-tests test:web`).

async function addVirtualAuthenticator(page: Page) {
  const client = await page.context().newCDPSession(page);
  await client.send("WebAuthn.enable");
  const { authenticatorId } = await client.send("WebAuthn.addVirtualAuthenticator", {
    options: {
      protocol: "ctap2",
      transport: "internal",
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
    },
  });
  return { client, authenticatorId };
}

test("passkey: register then sign in again in a fresh session, with no injected wallet provider ever present", async ({
  page,
}) => {
  await addVirtualAuthenticator(page);
  await page.goto("/onboarding");

  expect(await page.evaluate(() => "ethereum" in window)).toBe(false);

  await page.getByTestId("passkey-register-button").click();
  await expect(page.getByTestId("signed-in-indicator")).toBeVisible();

  // A fresh session (no cookie) must still be able to authenticate with
  // the same resident credential the virtual authenticator now holds —
  // (a user with an already-registered passkey returns).
  await page.context().clearCookies();
  await page.reload();
  await expect(page.getByTestId("passkey-login-button")).toBeVisible();
  await page.getByTestId("passkey-login-button").click();
  await expect(page.getByTestId("signed-in-indicator")).toBeVisible();

  expect(await page.evaluate(() => "ethereum" in window)).toBe(false);
});

test("vault: creating an identity, a wrong password, then the right password", async ({
  page,
}) => {
  await page.goto("/onboarding");

  await page.getByTestId("vault-password-input").fill("correct horse battery staple");
  await page.getByTestId("vault-create-button").click();
  await expect(page.getByTestId("signed-in-indicator")).toBeVisible();

  // The session survives a reload; sign out to get the unlock form back.
  await page.reload();
  await expect(page.getByTestId("signed-in-indicator")).toBeVisible();
  await page.getByTestId("sign-out-button").click();
  await expect(page.getByTestId("vault-unlock-button")).toBeVisible();

  await page.getByTestId("vault-password-input").fill("the wrong password");
  await page.getByTestId("vault-unlock-button").click();
  await expect(page.getByTestId("error-banner")).toContainText("Incorrect password");
  await expect(page.getByTestId("signed-in-indicator")).toHaveCount(0);

  await page.getByTestId("vault-password-input").fill("correct horse battery staple");
  await page.getByTestId("vault-unlock-button").click();
  await expect(page.getByTestId("signed-in-indicator")).toBeVisible();
});

test("vault: a downloaded backup unlocks the same identity in a browser that never created it", async ({
  page,
  browser,
}) => {
  await page.goto("/onboarding");
  await page.getByTestId("vault-password-input").fill("correct horse battery staple");
  await page.getByTestId("vault-create-button").click();
  await expect(page.getByTestId("signed-in-indicator")).toBeVisible();

  const downloadPromise = page.waitForEvent("download");
  await page.getByTestId("vault-download-backup-button").click();
  const download = await downloadPromise;
  const backupPath = await download.path();
  expect(backupPath).not.toBeNull();

  // A fresh browser context has no cookies and no localStorage — the
  // same guarantee a different physical device would have.
  const otherDevice = await browser.newContext();
  const otherPage = await otherDevice.newPage();
  await otherPage.goto("/onboarding");
  await expect(otherPage.getByTestId("vault-create-button")).toBeVisible();

  await otherPage.getByTestId("vault-import-input").setInputFiles(backupPath!);
  await expect(otherPage.getByTestId("vault-unlock-button")).toBeVisible();
  await otherPage.getByTestId("vault-password-input").fill("correct horse battery staple");
  await otherPage.getByTestId("vault-unlock-button").click();
  await expect(otherPage.getByTestId("signed-in-indicator")).toBeVisible();

  await otherDevice.close();
});

test("cancelling the passkey prompt shows a clear error and creates no partial state", async ({
  page,
}) => {
  // Simulates the browser rejecting navigator.credentials.create the way
  // a real cancel or authenticator timeout does (a NotAllowedError
  // DOMException) — exercises apps/web/lib/webauthn.ts's own
  // cancellation handling directly, independent of whether a given CDP
  // virtual-authenticator build exposes a "simulate cancel" command.
  await page.addInitScript(() => {
    const rejection = () => Promise.reject(new DOMException("The operation was cancelled.", "NotAllowedError"));
    Object.defineProperty(window.navigator, "credentials", {
      value: { ...window.navigator.credentials, create: rejection, get: rejection },
      configurable: true,
    });
  });
  await page.goto("/onboarding");

  await page.getByTestId("passkey-register-button").click();
  await expect(page.getByTestId("error-banner")).toContainText("cancelled or timed out");
  await expect(page.getByTestId("signed-in-indicator")).toHaveCount(0);
});

test("onboarding explains the difference between a passkey and a password identity without crypto jargon", async ({
  page,
}) => {
  await page.goto("/onboarding");
  const copy = page.locator("main");
  await expect(copy).toContainText(/face, fingerprint or screen lock/i);
  await expect(copy).toContainText(/password/i);
});

test("an unsupported browser is never offered the passkey option, and the vault still works", async ({
  page,
}) => {
  await page.addInitScript(() => {
    Object.defineProperty(window, "PublicKeyCredential", {
      get: () => undefined,
      configurable: true,
    });
  });
  await page.goto("/onboarding");

  await expect(page.getByTestId("webauthn-unsupported-note")).toBeVisible();
  await expect(page.getByTestId("passkey-register-button")).toHaveCount(0);
  await expect(page.getByTestId("passkey-login-button")).toHaveCount(0);

  await page.getByTestId("vault-password-input").fill("another password");
  await page.getByTestId("vault-create-button").click();
  await expect(page.getByTestId("signed-in-indicator")).toBeVisible();
});
