import { expect, test, type Page } from "@playwright/test";
import { API_PORT } from "../chainFixtures.js";

// Real browser (Playwright) coverage for apps/web/app/settings/api-tokens:
// generating, listing and revoking a read-only API token, driven through
// the real apps/web app against a real apps/api instance. Signs in the
// same way onboarding.spec.ts and portfolio.spec.ts do, via a CDP
// virtual authenticator standing in for a real passkey.

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

// This page has no sign-in form of its own — like /disclose and
// /claims, it only offers to sign in elsewhere (see the "Sign in
// first" test below) — so a signed-in test signs in on /portfolio
// first and relies on the session cookie carrying over.
async function signIn(page: Page) {
  await addVirtualAuthenticator(page);
  await page.goto("/portfolio");
  await page.getByTestId("passkey-register-button").click();
  // Confirms the session cookie is actually set before leaving the page.
  await expect(page.getByTestId("add-first-account-button")).toBeVisible();
  await page.goto("/settings/api-tokens");
}

test("api tokens: signed out, the page offers to sign in rather than a token form", async ({ page }) => {
  await page.goto("/settings/api-tokens");
  await expect(page.getByRole("heading", { name: "Sign in first" })).toBeVisible();
  await expect(page.getByTestId("create-token-card")).toHaveCount(0);
});

test("api tokens: create one, see it once, then find it revoked in the list", async ({ page }) => {
  await signIn(page);

  await expect(page.getByTestId("create-token-card")).toBeVisible();
  await page.getByTestId("token-label").fill("Hermes");
  await page.getByTestId("create-token").click();

  // The token is shown exactly once, in the clear.
  await expect(page.getByTestId("token-created")).toBeVisible();
  const revealed = await page.getByTestId("token-value").innerText();
  expect(revealed).toMatch(/^lvz_ro_[0-9a-f]{64}$/);

  // It shows up in the list, live.
  const row = page.getByTestId("token-row").filter({ hasText: "Hermes" });
  await expect(row).toBeVisible();
  await expect(row.getByTestId("revoke-token")).toBeVisible();

  // Revoking removes the reveal panel (nothing more to show) and marks
  // the row revoked instead of removing it — a person can see that a
  // token they revoked really is gone.
  await row.getByTestId("revoke-token").click();
  await expect(page.getByTestId("token-value")).toHaveCount(0);
  await expect(row.getByText("Revoked", { exact: true })).toBeVisible();
  await expect(row.getByTestId("revoke-token")).toHaveCount(0);
});

test("api tokens: a revoked token is refused by the read-only API for real", async ({ page }) => {
  await signIn(page);

  await page.getByTestId("token-label").fill("throwaway agent");
  await page.getByTestId("create-token").click();
  const token = await page.getByTestId("token-value").innerText();

  // The port the web app's own build was pointed at (webServer's
  // NEXT_PUBLIC_API_URL in playwright.config.ts) — not a plain
  // process.env read here, which is this test process's own
  // environment, never the browser's.
  const apiBase = `http://localhost:${API_PORT}`;
  const beforeRevoke = await page.request.get(`${apiBase}/mcp/account/state`, {
    headers: { authorization: `Bearer ${token}` },
  });
  expect(beforeRevoke.ok()).toBe(true);

  await page.getByTestId("token-row").filter({ hasText: "throwaway agent" }).getByTestId("revoke-token").click();
  await expect(page.getByTestId("token-value")).toHaveCount(0);

  const afterRevoke = await page.request.get(`${apiBase}/mcp/account/state`, {
    headers: { authorization: `Bearer ${token}` },
  });
  expect(afterRevoke.status()).toBe(401);
});
