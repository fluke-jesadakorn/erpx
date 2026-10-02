import { expect, test } from "@playwright/test";

test("signed-in ERP currency picker is searchable on desktop and mobile", async ({ page, baseURL }) => {
  test.setTimeout(180_000);
  const seededDev = ["127.0.0.1", "localhost", "dev-erpx.jesadakorn.com"].includes(new URL(baseURL!).hostname);
  const email = process.env.ERPX_E2E_EMAIL ?? (seededDev ? "mali@erpx.dev" : undefined);
  const password = process.env.ERPX_E2E_PASSWORD ?? (seededDev ? "Mali-ERPX-2026!" : undefined);
  test.skip(!email || !password, "Set an authorized ERPX browser test account for the target host.");

  await page.goto("/login", { waitUntil: "domcontentloaded" });
  await page.getByLabel("Work email").fill(email!);
  await page.getByLabel("Password", { exact: true }).fill(password!);
  const signIn = page.getByRole("button", { name: "Sign in", exact: true });
  await expect(signIn).toBeEnabled({ timeout: 60_000 });
  const signInResponse = page.waitForResponse((response) => response.url().includes("/api/auth/sign-in/email") && response.request().method() === "POST", { timeout: 30_000 });
  await signIn.click();
  expect((await signInResponse).status()).toBe(200);
  await expect(page).toHaveURL(/\/(?:holdings\/[^/]+\/home|work-role\?.*)$/, { timeout: 30_000 });
  if (page.url().includes("/work-role")) {
    await page.getByRole("combobox", { name: "Role for actions & approvals" }).fill("people manager");
    await page.getByRole("listbox").getByRole("option").click();
  }
  await expect(page).toHaveURL(/\/holdings\/[^/]+\/home$/);
  const picker = page.getByRole("button", { name: "Choose display currency" });
  await expect(picker).toBeVisible();
  await expect(picker).toBeEnabled({ timeout: 60_000 });
  await picker.click();
  await expect(picker).toHaveAttribute("aria-expanded", "true");
  await expect(page.getByRole("listbox", { name: "Display currency" })).toBeVisible();
  await page.getByRole("textbox", { name: "Search currency" }).fill("KWD");
  await expect(page.getByRole("option", { name: /KWD/ })).toBeVisible();
  await expect(page.getByRole("option", { name: /USD/ })).toHaveCount(0);
  await page.screenshot({ path: `.scratch/dev/currency-selector-${test.info().project.name}.png` });
  const manage = page.getByRole("button", { name: "Add or revise manual rate" });
  if (await manage.isVisible()) {
    await manage.click();
    const dialog = page.getByRole("dialog", { name: "Confirm manual exchange rate" });
    await expect(dialog).toBeVisible();
    await dialog.getByLabel("Currency").selectOption("KWD");
    await dialog.getByLabel("1 USD =").fill("0.307125");
    await expect(dialog).toContainText("After · USD 100.00");
    await expect(dialog).toContainText("KWD 30.713");
    await page.screenshot({ path: `.scratch/dev/currency-rate-${test.info().project.name}.png` });
    await dialog.getByRole("button", { name: "Cancel" }).click();
  }
  if (test.info().project.name === "desktop") {
    await picker.click();
    await page.getByRole("textbox", { name: "Search currency" }).fill("JPY");
    await page.getByRole("option", { name: /JPY/ }).click();
    await expect(picker).toContainText("JPY");
    await page.reload({ waitUntil: "commit" });
    await expect(page.getByRole("button", { name: "Choose display currency" })).toContainText("JPY");
    await page.getByRole("button", { name: "Choose display currency" }).click();
    await page.getByRole("textbox", { name: "Search currency" }).fill("USD");
    await page.getByRole("option", { name: /USD/ }).click();
    await expect(page.getByRole("button", { name: "Choose display currency" })).toContainText("USD");
  }
  await page.goto("/holdings/acme/commerce?view=sales", { waitUntil: "commit" });
  await expect(page.getByRole("button", { name: "Choose display currency" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Sales orders", level: 1 })).toBeVisible();
  await page.screenshot({ path: `.scratch/dev/currency-commerce-${test.info().project.name}.png` });
  await page.goto("/holdings/acme/expenses", { waitUntil: "commit" });
  await expect(page.getByRole("button", { name: "Choose display currency" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Employee expenses" })).toBeVisible();
  await page.screenshot({ path: `.scratch/dev/currency-expenses-${test.info().project.name}.png` });
});

test("signed-in POS and PMS Front show the shared currency picker", async ({ page, baseURL }) => {
  test.setTimeout(120_000);
  const local = ["127.0.0.1", "localhost"].includes(new URL(baseURL!).hostname);
  test.skip(!local, "Configure an authorized Front account and location for a remote host.");
  for (const app of ["pos", "pms"] as const) {
    await page.goto(`https://dev-${app}.jesadakorn.com/acme/acme-co/holding/`, { waitUntil: "domcontentloaded" });
    if (new URL(page.url()).pathname === "/login") {
      await page.getByRole("textbox", { name: /Work email/ }).fill("mali@erpx.dev");
      await page.getByLabel("Password", { exact: true }).fill("Mali-ERPX-2026!");
      const signIn = page.getByRole("button", { name: "Sign in", exact: true });
      await expect(signIn).toBeEnabled();
      const signInResponse = page.waitForResponse((response) => response.url().includes("/api/auth/sign-in/email") && response.request().method() === "POST", { timeout: 30_000 });
      await signIn.click();
      expect((await signInResponse).status()).toBe(200);
    }
    const picker = page.getByRole("button", { name: "Choose display currency" });
    await expect(picker).toBeVisible({ timeout: 30_000 });
    await expect(picker).toBeEnabled({ timeout: 30_000 });
    await picker.click();
    await expect(page.getByRole("listbox", { name: "Display currency" })).toBeVisible();
    await page.screenshot({ path: `.scratch/dev/currency-front-${app}-${test.info().project.name}.png` });
  }
});
