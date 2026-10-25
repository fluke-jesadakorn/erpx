import { expect, test } from "@playwright/test";
import { matchingRoleOptions } from "../../support/helpers/context-switching-browser";

test("health endpoint is available and not cached", async ({ request }) => {
  const response = await request.get("/api/health");
  expect(response.status()).toBe(200);
  expect(await response.json()).toMatchObject({ status: "ok" });
  expect(response.headers()["cache-control"]).toContain("no-store");
});

for (const path of ["/", "/holdings/acme/home", "/holdings/acme/work"]) {
  test(`signed-out visitors to ${path} reach login`, async ({ page }) => {
    await page.goto(path);
    await expect(page).toHaveURL(/\/login(?:\?.*)?$/);
    await expect(page.getByRole("heading", { name: "Welcome back" })).toBeVisible();
    await expect(page.getByLabel("Work email")).toBeVisible();
    await expect(page.getByLabel(/^Password(?:\s*\*)?$/)).toBeVisible();
  });
}

test("invalid credentials show an error and allow another attempt", async ({ page }) => {
  await page.goto("/login");
  await page.getByLabel("Work email").fill("playwright-nobody@example.invalid");
  await page.getByLabel(/^Password(?:\s*\*)?$/).fill("Invalid-password-123!");
  const responsePromise = page.waitForResponse((response) =>
    response.url().endsWith("/api/auth/sign-in/email") && response.request().method() === "POST",
  );
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  expect((await responsePromise).status()).toBe(401);
  await expect(page.locator("form").getByRole("alert")).toContainText(/invalid|incorrect/i);
  await expect(page).toHaveURL(/\/login$/);
  await expect(page.getByRole("button", { name: "Sign in", exact: true })).toBeEnabled();
});

test("invalid reset links explain how to request a replacement", async ({ page }) => {
  await page.goto("/reset-password?error=INVALID_TOKEN");
  await expect(
    page.getByRole("heading", { name: "Choose a new password" }),
  ).toBeVisible();
  const recoveryAlert = page
    .getByRole("alert")
    .filter({ hasText: "Reset link unavailable" });
  await expect(recoveryAlert).toContainText("Reset link unavailable");
  await expect(recoveryAlert).toContainText(
    "invalid, expired, or has been replaced",
  );
  await expect(
    page.getByRole("link", { name: "Request a new reset link" }),
  ).toHaveAttribute("href", "/forgot-password");
  await expect(page.getByLabel(/^Password(?:\s*\*)?$/)).toHaveCount(0);
});

test("login opens password recovery", async ({ page }) => {
  await page.goto("/login");
  await page.getByRole("link", { name: "Forgot password?", exact: true }).click();
  await expect(page).toHaveURL(/\/forgot-password$/);
  await expect(page.getByRole("heading", { name: "Reset your password.", exact: true })).toBeVisible();
});

test("seeded employee signs in through the UI and keeps their session after reload", async ({ page, baseURL }) => {
  const local = ["127.0.0.1", "localhost"].includes(new URL(baseURL!).hostname);
  const email = process.env.ERPX_E2E_EMAIL ?? (local ? "mali@erpx.dev" : undefined);
  const password = process.env.ERPX_E2E_PASSWORD ?? (local ? "Mali-ERPX-2026!" : undefined);
  test.skip(!email || !password, "Set ERPX_E2E_EMAIL and ERPX_E2E_PASSWORD for an authorized remote test account.");

  await page.goto("/login");
  await page.getByLabel("Work email").fill(email!);
  await page.getByLabel(/^Password(?:\s*\*)?$/).fill(password!);
  await Promise.all([
    page.waitForURL(/\/(?:holdings\/[^/]+\/home|work-role\?.*)$/, { waitUntil: "domcontentloaded" }),
    page.getByRole("button", { name: "Sign in", exact: true }).click(),
  ]);
  await expect(page).toHaveURL(/\/(?:holdings\/[^/]+\/home|work-role\?.*)$/);
  if(page.url().includes('/work-role')){
    await expect(page.getByRole('heading',{name:'Choose your work role',exact:true})).toBeVisible();
    const roles = await matchingRoleOptions(page, 'people manager');
    await roles.click();
  }
  await expect(page).toHaveURL(/\/holdings\/[^/]+\/home$/);
  await expect(page.getByRole("main")).toBeVisible();
  const homeURL = page.url();
  await page.reload();
  await expect(page).toHaveURL(homeURL);
  await expect(page.getByRole("main")).toBeVisible();
  await expect(page.getByRole("button", { name: "Sign in", exact: true })).toHaveCount(0);
});
