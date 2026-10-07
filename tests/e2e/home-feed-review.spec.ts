import { expect, test } from "@playwright/test";
import { selectRoleForSetup } from "../helpers/context-switching-browser";

test("Home feed controls and saved widget layout remain usable", async ({ page }) => {
  const email = process.env.ERPX_REVIEW_EMAIL;
  const password = process.env.ERPX_REVIEW_PASSWORD;
  test.skip(!email || !password, "Set development review credentials to run this browser check.");

  await page.goto("/login?next=%2Fholdings%2Facme%2Fhome");
  await page.getByRole("textbox", { name: /Work email/ }).fill(email!);
  await page.getByLabel(/^Password(?:\s*\*)?$/).fill(password!);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL(/\/holdings\/acme(?:\/branches\/[^/]+)?\/(?:home|work-role)(?:\?.*)?$/);
  await selectRoleForSetup(page.context(), "holding_owner");
  await page.goto("/holdings/acme/home");
  await expect(page.getByRole("heading", { name: "Today", exact: true })).toBeVisible();
  await expect(page.getByRole("region", { name: "Team updates", exact: true })).toBeVisible();

  const composer = page.locator("details.beta-composer");
  await composer.locator("summary").first().click();
  await expect(composer).toHaveAttribute("open", "");
  const editor = page.getByRole("dialog", { name: "Share an update", exact: true });
  await expect(editor).toBeVisible();
  await expect(editor.getByLabel("Post message", { exact: true })).toBeVisible();
  await expect(editor.getByRole("button", { name: "Share update", exact: true })).toBeDisabled();
  await editor.getByRole("button", { name: "Close dialog", exact: true }).click();
  await expect(editor).toBeHidden();

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

  // Home has a fixed feed. Saved layout previews remain supported on Overtime.
  await expect(page.getByRole("button", { name: "Customize layout", exact: true })).toHaveCount(0);
  await page.goto("/holdings/acme/overtime");
  const widget = page.locator('[data-feature-widget="overtime-requests-history"]');
  await expect(widget).toBeVisible();
  await page.getByRole("button", { name: "Customize layout" }).click();
  await expect(page.getByRole("button", { name: "Save", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Move Requests & history", exact: true })).toBeVisible();
  await page.getByRole("combobox", { name: "Requests & history width", exact: true }).selectOption("6");
  await expect(widget).toHaveAttribute("style", /grid-column: span 6/);
  await page.getByRole("button", { name: "Hide Requests & history", exact: true }).click();
  await expect(widget).toContainText("Hidden in your saved layout");
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(page.getByRole("button", { name: "Customize layout" })).toBeVisible();
  await expect(widget).toBeVisible();
  await page.goto("/holdings/acme/home");

  await page.getByRole("article").first().locator('.beta-person a[href*="/posts/"]').click();
  await expect(page.getByRole("heading", { name: "Team conversation" })).toBeVisible();
  await expect(page.getByRole("article")).toHaveCount(1);
});
