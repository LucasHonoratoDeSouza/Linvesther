import { expect, test } from "@playwright/test";

// The real create -> confirm flow in a real browser against
// the real Anvil + Postgres + deployed contracts globalSetup.ts brings
// up — not mocked at any layer.

test("creating and confirming an identity with the vault method ends active, on-chain", async ({ page }) => {
  await page.goto("/onboarding");

  await page.getByTestId("vault-password-input").fill("correct horse battery staple");
  await page.getByTestId("vault-create-button").click();
  await expect(page.getByTestId("signed-in-indicator")).toBeVisible();

  await page.getByTestId("create-identity-button").click();
  await expect(page.getByTestId("confirm-identity-button")).toBeVisible();
  await expect(page.getByTestId("identity-active-summary")).toHaveCount(0);

  await page.getByTestId("confirm-identity-button").click();
  await expect(page.getByTestId("identity-active-summary")).toBeVisible({ timeout: 15_000 });
  await expect(page.getByTestId("continue-to-portfolio-link")).toBeVisible();
});

test("a passkey-created identity shows a clear not-yet-supported note instead of a broken confirm attempt", async ({ page, context }) => {
  const client = await context.newCDPSession(page);
  await client.send("WebAuthn.enable");
  await client.send("WebAuthn.addVirtualAuthenticator", {
    options: { protocol: "ctap2", transport: "internal", hasResidentKey: true, hasUserVerification: true, isUserVerified: true },
  });

  await page.goto("/onboarding");
  await page.getByTestId("passkey-register-button").click();
  await expect(page.getByTestId("signed-in-indicator")).toBeVisible();

  await page.getByTestId("create-identity-button").click();
  await expect(page.getByTestId("confirm-unsupported-note")).toBeVisible();
  await expect(page.getByTestId("confirm-identity-button")).toHaveCount(0);
});
