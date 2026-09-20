import { expect, test } from "@playwright/test";

test("the disclosure explainer changes the visible statement without publishing a claim", async ({
  page,
}) => {
  const publications: string[] = [];
  page.on("request", (request) => {
    if (request.url().includes("/claims/disclose"))
      publications.push(request.url());
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Drawdown", exact: true }).click();
  await expect(page.getByTestId("claim-preview-statement")).toContainText(
    "15%",
  );
  await page.getByRole("button", { name: "What stays private" }).click();
  await expect(page.getByText("The details remain yours.")).toBeVisible();
  await expect(
    page.getByText("Exchange credentials", { exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "What you share", exact: true })
    .click();
  await expect(page.getByTestId("claim-preview-statement")).toContainText(
    "15%",
  );
  await expect(
    page.getByText("Illustrative preview · No proof generated.", {
      exact: false,
    }),
  ).toBeVisible();
  expect(publications).toEqual([]);
});

test("disclosure walkthrough supports manual steps, and thresholds", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Sharpe", exact: true }).click();
  await page.getByRole("button", { name: "Threshold 1.5" }).click();
  await page
    .getByRole("button", { name: "Keep details private", exact: true })
    .click();
  await expect(page.getByTestId("claim-preview-receipt")).toHaveCount(0);
  await page.waitForTimeout(2000);
  await expect(page.getByTestId("claim-preview-receipt")).toHaveCount(0);
  await page
    .getByRole("button", { name: "Preview disclosure", exact: true })
    .click();
  await expect(page.getByTestId("claim-preview-statement")).toContainText(
    "1.5",
  );
});

test("disclosure preview remains usable on mobile with reduced motion", async ({
  page,
}) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.setViewportSize({ width: 360, height: 800 });
  await page.goto("/");
  await page.getByRole("button", { name: "Drawdown", exact: true }).click();
  await page.getByRole("button", { name: "Threshold 8 percent" }).click();
  await expect(page.getByTestId("claim-preview-statement")).toContainText("8%");
  await page.getByRole("button", { name: "What stays private" }).click();
  await expect(page.getByTestId("claim-preview-receipt")).toHaveCount(0);
  await expect(page.getByText("128,430.82 USDT")).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page
    .getByRole("link", { name: "Create your own claim", exact: true })
    .click();
  await expect(page).toHaveURL(/\/disclose$/);
});

test("mobile navigation supports keyboard dismissal and real destinations", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/");
  const toggle = page.getByRole("button", { name: "Open navigation" });
  await toggle.click();
  await expect(
    page.getByRole("navigation", { name: "Mobile navigation" }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(toggle).toBeFocused();
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  await toggle.click();
  await page
    .getByRole("navigation", { name: "Mobile navigation" })
    .getByRole("link", { name: "Explorer", exact: true })
    .click();
  await expect(page).toHaveURL(/\/explorer$/);
  await expect(
    page.getByRole("heading", { name: "Look into the evidence." }),
  ).toBeVisible();
});

test("explorer opens a person's public profile from an address or a link, and still accepts track links", async ({
  page,
}) => {
  const address = "0x0000000000000000000000000000000000000001";
  await page.goto("/explorer");
  await page.getByTestId("track-search-input").fill(address);
  await page.getByTestId("track-search-button").click();
  await expect(page).toHaveURL(new RegExp(`/p/${address}$`));
  // Nobody has published anything under this address, and the page says so
  // plainly instead of showing an empty or invented profile.
  await expect(page.getByTestId("profile-missing")).toBeVisible();

  await page.goto("/explorer");
  await page.getByTestId("track-search-input").fill(`https://example.test/p/${address}`);
  await page.getByTestId("track-search-button").click();
  await expect(page).toHaveURL(new RegExp(`/p/${address}$`));

  await page.goto("/explorer");
  await page
    .getByTestId("track-search-input")
    .fill("https://example.test/profile/track-1");
  await page.getByTestId("track-search-button").click();
  await expect(page).toHaveURL(/\/profile\/track-1$/);
  await expect(page.getByTestId("currency")).toContainText("USDT");
  await expect(page.getByTestId("a0-notice")).toBeVisible();

  await page.goto("/explorer");
  await page.getByTestId("track-search-input").fill("not a valid thing!");
  await page.getByTestId("track-search-button").click();
  await expect(page.getByRole("main").getByRole("alert")).toContainText("Enter an address");
});

test("reduced motion and narrow layouts preserve usable content", async ({
  page,
}) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.setViewportSize({ width: 360, height: 800 });
  for (const path of ["/", "/disclose", "/docs"]) {
    await page.goto(path);
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth,
      ),
    ).toBe(true);
  }
});
