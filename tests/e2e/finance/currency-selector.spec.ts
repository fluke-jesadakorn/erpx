import { captureCaseScreenshot } from "../../support/helpers/case-screenshot";
import { expect, test } from "@playwright/test";
import { browserFrontOrigin } from "../../../scripts/testing/browser-fixture-metadata";
import { selectFixtureAssignment } from "../../support/browser/select-fixture-assignment";

test("signed-in ERP currency picker is searchable on desktop and mobile", async ({ page, context, baseURL }) => {
  test.setTimeout(180_000);
  const seededDev = ["127.0.0.1", "localhost", "dev-erpx.jesadakorn.com"].includes(new URL(baseURL!).hostname);
  const email = process.env.ERPX_E2E_EMAIL ?? (seededDev ? "mali@erpx.dev" : undefined);
  const password = process.env.ERPX_E2E_PASSWORD ?? (seededDev ? "Mali-ERPX-2026!" : undefined);
  test.skip(!email || !password, "Set an authorized ERPX browser test account for the target host.");

  await page.goto("/login", { waitUntil: "domcontentloaded" });
  await page.getByLabel("Work email").fill(email!);
  await page.getByLabel(/^Password(?:\s*\*)?$/).fill(password!);
  const signIn = page.getByRole("button", { name: "Sign in", exact: true });
  await expect(signIn).toBeEnabled({ timeout: 60_000 });
  const signInResponse = page.waitForResponse((response) => response.url().includes("/api/auth/sign-in/email") && response.request().method() === "POST", { timeout: 30_000 });
  await signIn.click();
  expect((await signInResponse).status()).toBe(200);
  await expect(page).toHaveURL(/\/holdings\/[^/]+\/home$/, { timeout: 30_000 });
  await selectFixtureAssignment(context, "acme-co", "asg_mali_owner");
  const home = await page.goto("/holdings/acme/home", { waitUntil: "domcontentloaded" });
  expect(home?.status()).toBe(200);
  await expect(page.getByRole("heading", { name: "Today", exact: true, level: 1 })).toBeVisible();
  const picker = page.getByRole("button", { name: "Choose display currency" });
  await expect(picker).toBeVisible();
  await expect(picker).toBeEnabled({ timeout: 60_000 });
  await picker.click();
  await expect(picker).toHaveAttribute("aria-expanded", "true");
  const search = page.getByRole("combobox", { name: "Display currency", exact: true });
  await expect(search).toBeVisible();
  await search.fill("KWD");
  await expect(page.getByRole("option", { name: /KWD/ })).toBeVisible();
  await expect(page.getByRole("option", { name: /USD/ })).toHaveCount(0);
  await captureCaseScreenshot(page, `currency-selector-${test.info().project.name}.png`);
  // Close the options popup before opening the existing Company rate drawer.
  await search.press("Escape");
  const manage = page.getByRole("button", { name: "Add or revise manual rate", exact: true });
  await expect(manage).toBeVisible();
  await manage.click();
  const dialog = page.getByRole("dialog", { name: "Confirm manual exchange rate" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("combobox", { name: "Currency", exact: true }).selectOption("KWD");
  await dialog.getByRole("textbox", { name: "1 USD =", exact: true }).fill("0.307125");
  await expect(dialog).toContainText("After · USD 100.00");
  await expect(dialog).toContainText("KWD 30.713");
  await captureCaseScreenshot(page, `currency-rate-${test.info().project.name}.png`);
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await picker.click();
  await search.fill("JPY");
  await page.getByRole("option", { name: /JPY/ }).click();
  await expect(picker).toContainText("JPY");
  await page.reload({ waitUntil: "commit" });
  await expect(picker).toContainText("JPY");
  await expect(picker).toBeEnabled();
  await picker.click();
  await search.fill("USD");
  await page.getByRole("option", { name: /USD/ }).click();
  await expect(picker).toContainText("USD");
  await page.goto("/holdings/acme/commerce?view=sales", { waitUntil: "commit" });
  await expect(page).toHaveURL(url => url.pathname === "/holdings/acme/sales" && url.searchParams.get("view") === "documents" && url.searchParams.get("type") === "sales", { timeout: 30_000 });
  const salesUrl = new URL(page.url());
  expect(salesUrl.pathname).toBe("/holdings/acme/sales");
  expect(salesUrl.searchParams.get("view")).toBe("documents");
  expect(salesUrl.searchParams.get("type")).toBe("sales");
  await expect(page.getByRole("button", { name: "Choose display currency" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Sales", exact: true, level: 1 })).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Sales document type" }).getByRole("link", { name: "Sales orders", exact: true })).toHaveAttribute("aria-current", "page");
  await captureCaseScreenshot(page, `currency-commerce-${test.info().project.name}.png`);
  await page.goto("/holdings/acme/expenses", { waitUntil: "commit" });
  await expect(page.getByRole("button", { name: "Choose display currency" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Employee expenses" })).toBeVisible();
  await captureCaseScreenshot(page, `currency-expenses-${test.info().project.name}.png`);
});

test("signed-in POS and PMS Front show the shared currency picker", async ({ page, baseURL }) => {
  test.setTimeout(120_000);
  const local = ["127.0.0.1", "localhost"].includes(new URL(baseURL!).hostname);
  test.skip(!local && (!process.env.ERPX_E2E_FRONT_POS_ORIGIN || !process.env.ERPX_E2E_FRONT_PMS_ORIGIN), "Configure authorized Front origins, account and location for a remote host.");
  for (const app of ["pos", "pms"] as const) {
    const origin = browserFrontOrigin(baseURL!, app, process.env[`ERPX_E2E_FRONT_${app.toUpperCase()}_ORIGIN`]);
    await page.goto(`${origin}/acme/acme-co/holding/`, { waitUntil: "domcontentloaded" });
    if (new URL(page.url()).pathname === "/login") {
      await page.getByRole("textbox", { name: /Work email/ }).fill(process.env.ERPX_E2E_EMAIL ?? "mali@erpx.dev");
      await page.getByLabel(/^Password(?:\s*\*)?$/).fill(process.env.ERPX_E2E_PASSWORD ?? "Mali-ERPX-2026!");
      const signIn = page.getByRole("button", { name: "Sign in", exact: true });
      await expect(signIn).toBeEnabled();
      const signInResponse = page.waitForResponse((response) => response.url().includes("/api/auth/sign-in/email") && response.request().method() === "POST", { timeout: 30_000 });
      await signIn.click();
      expect((await signInResponse).status()).toBe(200);
    }
    const more = page.getByRole("banner").locator("summary").filter({ hasText: /^\s*(?:More|เพิ่มเติม)(?:\s*···)?\s*$/ });
    await expect(more).toBeVisible({ timeout: 30_000 });
    await more.click();
    await expect(more.locator("..")).toHaveJSProperty("open", true);
    const picker = page.getByRole("button", { name: "Choose display currency" });
    await expect(picker).toBeVisible({ timeout: 30_000 });
    await expect(picker).toBeEnabled({ timeout: 30_000 });
    await picker.click();
    await expect(page.getByRole("listbox", { name: "Display currency" })).toBeVisible();
    await captureCaseScreenshot(page, `currency-front-${app}-${test.info().project.name}.png`);
  }
});
