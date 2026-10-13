import { test, expect } from "@playwright/test";

test.setTimeout(120_000);

test("signed-out workspace link explains sign-in and preserves its destination", async ({ page }) => {
  const target = "/holdings/acme/branches/bangkok/departments/operations/home";
  await page.goto(target);
  await expect(page).toHaveURL(/\/login\?reason=sign_in/);
  await expect(page.getByRole("status")).toContainText("session has expired");
  expect(new URL(page.url()).searchParams.get("next")).toBe(target);
});

test("scoped account gets a reason, working recovery and unchanged API restrictions", async ({ page, context, baseURL }, info) => {
  const login = await context.request.post("/api/auth/sign-in/email", { data: { email: "demo.bangkok.operations.staff@erpx.dev", password: "Elena-ERPX-2026!" } });
  expect(login.ok()).toBe(true);
  const holding = await page.goto("/holdings/acme/home");
  expect(holding?.status()).toBe(403);
  await expect(page).toHaveURL(/\/holdings\/acme\/home$/);
  await expect(page.getByRole("heading", { name: "Your role cannot open this view" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Account menu for Elena Suwan, demo.bangkok.operations.staff@erpx.dev", exact: true })).toBeVisible();
  const unselected = await context.request.get("/api/v1/holdings/acme/people/organization");
  expect(unselected.status()).toBe(409);
  await expect(unselected.json()).resolves.toEqual({ error: { code: "COMPANY_REQUIRED", message: "Select an active Company to continue." } });
  await page.getByRole("button", { name: "Switch workspace", exact: true }).click();
  const workspaces = page.getByRole("dialog", { name: "Switch workspace", exact: true });
  const [recovered] = await Promise.all([
    page.waitForResponse(response => response.request().isNavigationRequest() && new URL(response.url()).pathname === "/holdings/acme/branches/bangkok/departments/operations/home"),
    workspaces.locator('a[href="/holdings/acme/branches/bangkok/departments/operations/home"]').click(),
  ]);
  expect(recovered.status()).toBe(200);
  await expect(page).toHaveURL(/\/branches\/bangkok\/departments\/operations\/home$/);
  await expect(page.getByRole("heading", { name: "Today", exact: true, level: 1 })).toBeVisible();
  await expect(page.getByRole("button", { name: "Account menu for Elena Suwan, demo.bangkok.operations.staff@erpx.dev", exact: true })).toBeVisible();
  if (info.project.name === "desktop")
    await expect(page.getByText("Good to see you, Elena. Catch up with your team.", { exact: true })).toBeVisible();
  const organization = await context.request.get("/api/v1/holdings/acme/branches/bangkok/departments/operations/people/organization");
  expect(organization.status()).toBe(403);
  const platform = await page.goto("/platform/home");
  expect(platform?.status()).toBe(403);
  const platformAlias = platform?.request().redirectedFrom();
  expect(platformAlias?.url()).toBe(new URL("/platform/home", baseURL!).href);
  expect((await platformAlias?.response())?.status()).toBe(307);
  await expect(page).toHaveURL(new URL("/holdings/erpx/manage-erpx", baseURL!).href);
  await expect(page.getByRole("heading", { name: "Your role cannot open this view" })).toBeVisible();
  const settings = await page.goto("/holdings/acme/branches/bangkok/departments/operations/settings?section=people");
  expect(settings?.status()).toBe(200);
  await expect(page).toHaveURL(new URL("/holdings/acme/branches/bangkok/departments/operations/people?panel=settings", baseURL!).href);
  await expect(page.getByRole("heading", { name: "People", exact: true, level: 1 })).toBeVisible();
  await expect(page.getByText("Read-only organization explorer", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "+ Add", exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "HR settings", exact: true })).toHaveCount(0);
  const readOnlyOrganization = await context.request.get("/api/v1/holdings/acme/branches/bangkok/departments/operations/people/organization");
  expect(readOnlyOrganization.status()).toBe(403);
  const unknownBranch = await page.goto("/holdings/acme/branches/unknown-branch/home");
  expect(unknownBranch?.status()).toBe(403);
  await expect(page).toHaveURL(/\/holdings\/acme\/branches\/unknown-branch\/home$/);
  await expect(page.getByRole("heading", { name: "Your role cannot open this view" })).toBeVisible();
  await page.goto("/access?from=%2Fholdings%2Facme%2Fbranches%2Fbangkok%2Fdepartments%2Foperations%2Fhome&reason=expired");
  // A forged reason cannot claim that a live assignment expired.
  await expect(page.getByRole("heading", { name: "Your workspace is available" })).toBeVisible();
  await context.addCookies([{ name: "erpx_company_h001", value: "missing-company", url: baseURL! }]);
  const missingCompany = await page.goto("/holdings/acme/home");
  expect(missingCompany?.status()).toBe(403);
  await expect(page).toHaveURL(/\/holdings\/acme\/home$/);
  await expect(page.getByRole("heading", { name: "Your role cannot open this view" })).toBeVisible();
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
