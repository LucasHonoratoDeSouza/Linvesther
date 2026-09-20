import { expect, test, type Page } from "@playwright/test";

// Drives the real apps/web app in a real browser
// against a real apps/api instance — both started fresh for the test
// run, not stubbed. Previously signed in via a scripted injected wallet
// provider; that flow was removed (no crypto wallet required
// anywhere in onboarding) in favor of the passkey/vault credential flow
// — see tests/e2e/web/passkeyIdentity.e2e.spec.ts for dedicated
// coverage of that flow's own edge cases. This file keeps its original
// focus: the identity/account lifecycle (create, bind, remove) and
// disclosure, once signed in.

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

test("onboarding: sign in with a passkey, then create the identity", async ({
  page,
}) => {
  await addVirtualAuthenticator(page);
  await page.goto("/onboarding");

  await page.getByTestId("passkey-register-button").click();
  await expect(page.getByTestId("signed-in-indicator")).toBeVisible();

  // Creating the identity is a real transaction on the local chain.
  await page.getByTestId("create-identity-button").click();
  await expect(page.getByTestId("identity-state")).toContainText("one more click", { timeout: 30_000 });
});

test("failure: a wrong vault password shows a real error, not a fabricated success", async ({
  page,
}) => {
  await page.goto("/onboarding");
  await page.getByTestId("vault-password-input").fill("the real password");
  await page.getByTestId("vault-create-button").click();
  await expect(page.getByTestId("signed-in-indicator")).toBeVisible();

  // The session survives a reload; sign out to get the unlock form back.
  await page.reload();
  await expect(page.getByTestId("signed-in-indicator")).toBeVisible();
  await page.getByTestId("sign-out-button").click();
  await expect(page.getByTestId("vault-unlock-button")).toBeVisible();
  await page.getByTestId("vault-password-input").fill("not the real password");
  await page.getByTestId("vault-unlock-button").click();

  await expect(page.getByTestId("error-banner")).toBeVisible();
  await expect(page.getByTestId("signed-in-indicator")).toHaveCount(0);
});

test("profile: shows the five dimensions, USDT currency and the A0 limitation notice", async ({
  page,
}) => {
  await page.goto("/profile/track-1");

  await expect(page.getByTestId("currency")).toContainText("USDT");
  await expect(page.getByTestId("a0-notice")).toBeVisible();
  await expect(page.getByTestId("dimension-origin")).toContainText("A0");
  await expect(page.getByTestId("dimension-coverage")).toContainText(
    "POLICY_COMPLETE",
  );
  await expect(page.getByTestId("dimension-calculation")).toContainText(
    "ZK_VERIFIED",
  );
  await expect(page.getByTestId("dimension-registry")).toContainText(
    "FINALIZED",
  );
  await expect(page.getByTestId("dimension-availability")).toContainText(
    "AVAILABLE",
  );
});

test("profile: a missing track shows a real failure state, not fabricated data", async ({
  page,
}) => {
  await page.goto("/profile/does-not-exist");
  await expect(page.getByTestId("error-banner")).toBeVisible();
});

test("public profile settings: signed out asks to sign in; signed in without an account says to connect one", async ({
  page,
}) => {
  await page.goto("/disclose");
  await expect(page.getByRole("heading", { name: "Sign in first" })).toBeVisible();

  await addVirtualAuthenticator(page);
  await page.goto("/onboarding");
  await page.getByTestId("passkey-register-button").click();
  await expect(page.getByTestId("signed-in-indicator")).toBeVisible();

  await page.goto("/disclose");
  await expect(page.getByText("Connect an account to have a track record.")).toBeVisible();
  // The public address is there to copy from the start, and says what it exposes.
  await expect(page.getByTestId("public-address")).toBeVisible();
  await expect(page.getByTestId("public-link-status")).toContainText("percentages only");
});
