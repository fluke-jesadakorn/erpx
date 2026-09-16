import { test, expect } from "@playwright/test";

test("signed-out workspace link explains sign-in and preserves its destination", async ({ page }) => {
  const target = "/holdings/acme/branches/bangkok/departments/operations/home";
  await page.goto(target);
  await expect(page).toHaveURL(/\/login\?reason=sign_in/);
  await expect(page.getByRole("status")).toContainText("session has expired");
  expect(new URL(page.url()).searchParams.get("next")).toBe(target);
});

test("scoped account gets a reason, working recovery and unchanged API restrictions", async ({ page, context, baseURL }) => {
  const login = await context.request.post("/api/auth/sign-in/email", { data: { email: "demo.bangkok.operations.staff@erpx.dev", password: "Elena-ERPX-2026!" } });
  expect(login.ok()).toBe(true);
  await page.goto("/holdings/acme/home");
  await expect(page).toHaveURL(/\/access\?/);
  await expect(page.getByRole("heading", { name: "This link opens a different workspace" })).toBeVisible();
  await expect(page.getByText("demo.bangkok.operations.staff@erpx.dev", { exact: true })).toBeVisible();
  await expect(page.getByText("What you can do", { exact: true })).toBeVisible();
  expect((await context.request.get("/api/v1/holdings/acme/people/organization")).status()).toBe(403);
  await page.getByRole("link", { name: "Open my workspace", exact: true }).click();
  await expect(page).toHaveURL(/\/branches\/bangkok\/departments\/operations\/home$/);
  await expect(page.getByRole("heading", { name: "Hello, Elena" })).toBeVisible();
  await page.goto("/platform/home");
  await expect(page.getByRole("heading", { name: "Platform administration requires separate access" })).toBeVisible();
  await page.goto("/holdings/acme/branches/bangkok/departments/operations/settings?section=people");
  await expect(page.getByRole("heading", { name: "Your role cannot open this view" })).toBeVisible();
  await page.goto("/holdings/acme/branches/unknown-branch/home");
  await expect(page.getByRole("heading", { name: "We couldn’t find an available workspace for this link" })).toBeVisible();
  await page.goto("/access?from=%2Fholdings%2Facme%2Fbranches%2Fbangkok%2Fdepartments%2Foperations%2Fhome&reason=expired");
  // A forged reason cannot claim that a live assignment expired.
  await expect(page.getByRole("heading", { name: "Your workspace is available" })).toBeVisible();
  await context.addCookies([{ name: "erpx_company_h001", value: "missing-company", url: baseURL! }]);
  await page.goto("/holdings/acme/home");
  await expect(page.getByRole("heading", { name: "Your selected company is unavailable" })).toBeVisible();
});

test("missing pages fit mobile and desktop, support offline recovery and safe navigation", async ({ page, context }) => {
  await page.goto("/a-page-that-does-not-exist");
  await expect(page.getByRole("heading", { name: "We couldn’t find this page" })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
  const link = page.getByRole("link", { name: "Open my workspace", exact: true });
  const size = await link.boundingBox(); expect(size!.height).toBeGreaterThanOrEqual(44);
  await link.focus(); await expect(link).toBeFocused();
  await context.setOffline(true);
  await expect(page.getByRole("heading", { name: "You appear to be offline" })).toBeVisible();
  await context.setOffline(false);
  await expect(page.getByRole("heading", { name: "We couldn’t find this page" })).toBeVisible();
  await page.goto("/login?reason=sign_in&next=https%3A%2F%2Fevil.example");
  await expect(page.getByRole("heading", { name: "Welcome back" })).toBeVisible();
  expect(new URL(page.url()).hostname).not.toBe("evil.example");
});
