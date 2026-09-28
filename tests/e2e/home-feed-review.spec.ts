import { expect, test } from "@playwright/test";

test("Home feed controls and saved widget layout remain usable", async ({ page }) => {
  const email = process.env.ERPX_REVIEW_EMAIL;
  const password = process.env.ERPX_REVIEW_PASSWORD;
  test.skip(!email || !password, "Set development review credentials to run this browser check.");

  await page.goto("/login?next=%2Fholdings%2Facme%2Fhome");
  await page.getByRole("textbox", { name: /Work email/ }).fill(email!);
  await page.getByLabel("Password").fill(password!);
  await page.getByRole("button", { name: "Sign in" }).click();
  await page.waitForURL("**/holdings/acme/home");
  await expect(page.getByRole("heading", { name: "Feed", exact: true })).toBeVisible();

  const composer = page.locator("details.beta-composer");
  await composer.locator("summary").first().click();
  await expect(composer).toHaveAttribute("open", "");
  await expect(composer.getByLabel("Post message", { exact: true })).toBeVisible();
  await expect(composer.getByRole("button", { name: "Share update" })).toBeDisabled();

  await page.getByRole("button", { name: "Following", exact: true }).click();
  await expect(page.getByRole("button", { name: "Following", exact: true })).toHaveAttribute("aria-pressed", "true");
  await page.getByRole("button", { name: "Latest updates", exact: true }).click();
  await expect(page.getByRole("button", { name: "Latest updates", exact: true })).toHaveAttribute("aria-pressed", "true");
  await page.getByRole("combobox", { name: "Filter branch" }).selectOption({ label: "Bangkok Operations" });
  await expect(page.getByRole("combobox", { name: "Filter branch" })).toHaveJSProperty("selectedIndex", 1);
  await page.getByRole("combobox", { name: "Filter branch" }).selectOption("");

  const firstPost = page.getByRole("article").first();
  await firstPost.getByRole("button", { name: /Comment/ }).click();
  await expect(firstPost.getByLabel("Write a comment")).toBeVisible();
  await firstPost.locator("details.post-more > summary").click();
  await expect(firstPost.getByRole("button", { name: "Summarize" })).toBeVisible();

  await page.getByRole("button", { name: "Customize layout" }).click();
  await expect(page.getByRole("button", { name: "Save", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Move Feed up" })).toBeVisible();
  await page.getByRole("combobox", { name: "Feed width" }).selectOption("6");
  await expect(page.getByRole("combobox", { name: "Feed width" })).toHaveValue("6");
  await page.getByRole("button", { name: "Hide Feed" }).click();
  await expect(page.locator('[data-feature-widget="home-feed"]')).toContainText("Hidden in your saved layout");
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(page.getByRole("button", { name: "Customize layout" })).toBeVisible();
  await expect(page.locator('[data-feature-widget="home-feed"]')).toBeVisible();

  await page.getByRole("article").first().locator(".beta-person a").click();
  await expect(page.getByRole("heading", { name: "Team conversation" })).toBeVisible();
  await expect(page.getByRole("article")).toHaveCount(1);
});
