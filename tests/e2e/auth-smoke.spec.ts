import { expect, test } from "@playwright/test";

test("health endpoint is available and not cached", async ({ request }) => {
  const response = await request.get("/api/health");
  expect(response.status()).toBe(200);
  expect(await response.json()).toEqual({ status: "ok" });
  expect(response.headers()["cache-control"]).toContain("no-store");
});

for (const path of ["/", "/holdings/acme/home", "/holdings/acme/work"]) {
  test(`signed-out visitors to ${path} reach login`, async ({ page }) => {
    await page.goto(path);
    await expect(page).toHaveURL(/\/login(?:\?.*)?$/);
    await expect(page.getByRole("heading", { name: "Welcome back" })).toBeVisible();
    await expect(page.getByLabel("Work email")).toBeVisible();
    await expect(page.getByLabel("Password", { exact: true })).toBeVisible();
  });
}

test("invalid credentials show an error and allow another attempt", async ({ page }) => {
  await page.goto("/login");
  await page.getByLabel("Work email").fill("playwright-nobody@example.invalid");
  await page.getByLabel("Password", { exact: true }).fill("Invalid-password-123!");
  const responsePromise = page.waitForResponse((response) =>
    response.url().endsWith("/api/auth/sign-in/email") && response.request().method() === "POST",
  );
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  expect((await responsePromise).status()).toBe(401);
  await expect(page.locator("form").getByRole("alert")).toContainText(/invalid|incorrect/i);
  await expect(page).toHaveURL(/\/login$/);
  await expect(page.getByRole("button", { name: "Sign in", exact: true })).toBeEnabled();
});

for (const destination of [
  { link: "Forgot password?", path: "/forgot-password", heading: "Reset your password" },
  { link: "Verify email", path: "/verify-email", heading: "Verify your email" },
  { link: "Join the free beta", path: "/signup", heading: "A shared home for your team’s work" },
]) {
  test(`login navigates to ${destination.path}`, async ({ page }) => {
    await page.goto("/login");
    await page.getByRole("link", { name: destination.link, exact: true }).click();
    await expect(page).toHaveURL(new RegExp(`${destination.path}$`));
    await expect(page.getByRole("heading", { name: destination.heading, exact: true })).toBeVisible();
  });
}

test("seeded employee signs in through the UI and keeps their session after reload", async ({ page, baseURL }) => {
  const local = ["127.0.0.1", "localhost"].includes(new URL(baseURL!).hostname);
  const email = process.env.ERPX_E2E_EMAIL ?? (local ? "mali@erpx.dev" : undefined);
  const password = process.env.ERPX_E2E_PASSWORD ?? (local ? "Mali-ERPX-2026!" : undefined);
  test.skip(!email || !password, "Set ERPX_E2E_EMAIL and ERPX_E2E_PASSWORD for an authorized remote test account.");

  await page.goto("/login");
  await page.getByLabel("Work email").fill(email!);
  await page.getByLabel("Password", { exact: true }).fill(password!);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page).toHaveURL(/\/(?:holdings\/[^/]+\/home|work-role\?.*)$/);
  if(page.url().includes('/work-role')){
    await page.getByRole('combobox',{name:'Role for actions & approvals'}).fill('people manager');
    await page.getByRole('listbox').getByRole('option').click();
  }
  await expect(page).toHaveURL(/\/holdings\/[^/]+\/home$/);
  await expect(page.getByRole("main")).toBeVisible();
  const homeURL = page.url();
  await page.reload();
  await expect(page).toHaveURL(homeURL);
  await expect(page.getByRole("main")).toBeVisible();
  await expect(page.getByRole("button", { name: "Sign in", exact: true })).toHaveCount(0);
});
