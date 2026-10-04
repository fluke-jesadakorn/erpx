import { expect, test } from "@playwright/test";

const base = "/holdings/acme";
test.setTimeout(120_000);
test.beforeEach(async ({ context }) => {
  const response = await context.request.post("/api/auth/sign-in/email", {
    headers: { Origin: process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100" },
    data: {
      email: process.env.ERPX_E2E_ADMIN_EMAIL ?? "jesadakorn.kirtnu@gmail.com",
      password: process.env.ERPX_E2E_ADMIN_PASSWORD ?? "12345678",
    },
  });
  expect(response.status(), await response.text()).toBe(200);
});

async function fitsViewport(page: import("@playwright/test").Page) {
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
}

test("HR settings uses one flat topic list and edits inside the People detail panel", async ({ page }, info) => {
  await page.goto(`${base}/people/settings`);
  await expect(page).toHaveURL(/\/people\?.*panel=settings/);
  const panel = page.getByTestId("people-detail"), sections = panel.getByRole("navigation", { name: "HR settings topics" });
  await expect(panel).toHaveCount(1, { timeout: 30_000 });
  await expect(sections.getByRole("button")).toHaveCount(6);
  await expect(sections.getByRole("button", { name: /Custom attributes/ })).toHaveCount(0);
  await expect(panel.getByRole("button", { name: "Create policy", exact: true })).toHaveCount(0);
  await fitsViewport(page);

  await sections.getByRole("button", { name: /^Leave & overtime/ }).click();
  await expect(page).toHaveURL(/section=leave/, { timeout: 60_000 });
  await expect(panel.getByRole("button", { name: "Create policy", exact: true })).toBeVisible();
  await panel.locator("summary").filter({ hasText: /^Create leave type/ }).click();
  await expect(panel.getByLabel("Code", { exact: true })).toBeVisible();
  // The mobile detail panel is itself a dialog; settings stay inside that panel.
  await expect(page.getByRole("dialog")).toHaveCount(info.project.name === "mobile" ? 1 : 0);
  await panel.getByRole("button", { name: "All HR settings", exact: true }).click();
  await expect(sections).toBeVisible();
  await fitsViewport(page);

  await sections.getByRole("button", { name: /^Organization templates/ }).click();
  await expect(page).toHaveURL(/section=templates/, { timeout: 60_000 });
  await expect(panel.getByRole("heading", { name: "Organization templates", exact: true })).toBeVisible();
  await expect(panel.locator("summary").filter({ hasText: /^Create organization template/ })).toBeVisible();
  await expect(panel.getByRole("button", { name: "Create leave type" })).toHaveCount(0);
  await panel.getByRole("button", { name: "All HR settings", exact: true }).click();
  await sections.getByRole("button", { name: /^Lifecycle handoff/ }).click();
  await expect(panel.getByRole("combobox", { name: "IT owner", exact: true })).toBeVisible();
  await expect(panel.getByRole("button", { name: "Save lifecycle handoff", exact: true })).toBeVisible();
  await fitsViewport(page);

  await page.reload();
  await expect(panel.getByRole("combobox", { name: "IT owner", exact: true })).toBeVisible();
  await panel.getByRole("button", { name: "Close details", exact: true }).click();
  await expect(panel).toHaveCount(0);
  await expect(page.getByTestId("people-organization-rail")).toBeAttached();
});

test("leave policy wizard preserves draft and submits one reviewed command", async ({ page }) => {
  await page.goto(`${base}/people?panel=settings&section=leave`);
  await page.getByRole("button", { name: "Create policy", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Basics" })).toBeVisible();
  await page.getByLabel("Policy key").fill("browser-review-policy");
  await page.getByLabel("Version").fill("1");
  await page.getByLabel("Policy name").fill("Browser review policy");
  await page.getByLabel("Effective from").fill("2026-10-01");
  await page.getByLabel("Timezone").fill("Asia/Bangkok");
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Working days" })).toBeVisible();
  await page.getByLabel("Saturday").check();
  await page.getByRole("button", { name: "Back" }).click();
  await expect(page.getByLabel("Policy name")).toHaveValue("Browser review policy");
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByLabel("Saturday")).toBeChecked();
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Leave rules" })).toBeVisible();
  await page.getByLabel("Leave type").selectOption({ index: 1 });
  await page.getByLabel("Grant units").fill("12");
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByRole("heading", { name: "Review" })).toBeVisible();
  let calls = 0;
  let posted: Record<string, unknown> | null = null;
  await page.route("**/leave/admin", async route => {
    calls++;
    posted = route.request().postDataJSON();
    await route.fulfill({ status: calls === 1 ? 422 : 200, contentType: "application/json", body: calls === 1 ? JSON.stringify({ error: { message: "Review API error" } }) : JSON.stringify({ data: { id: "reviewed" } }) });
  });
  await page.getByRole("button", { name: "Create policy version" }).click();
  await expect(page.getByRole("alert").filter({ hasText: "Review API error" })).toBeVisible();
  await expect(page.getByText("Browser review policy", { exact: false }).first()).toBeVisible();
  await page.getByRole("button", { name: "Back" }).click();
  await expect(page.getByLabel("Grant units")).toHaveValue("12");
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: "Create policy version" }).click();
  await expect(page).toHaveURL(/section=leave/);
  expect(calls).toBe(2);
  expect(posted).toMatchObject({ resource: "policy", policyKey: "browser-review-policy", name: "Browser review policy", workweek: [1, 2, 3, 4, 5, 6] });
  await fitsViewport(page);
});

test("custom attributes live in Technology and old direct links redirect", async ({ page }) => {
  await page.goto(`${base}/people/settings?section=attributes`, { waitUntil: "domcontentloaded" });
  await expect(page).toHaveURL(/\/technology\?view=attributes$/, { timeout: 60_000 });
  await expect(page.getByRole("link", { name: "Custom attributes", exact: true }).first()).toBeVisible();
  await expect(page.getByRole("heading", { name: "Custom attributes", exact: true })).toBeVisible();
  await fitsViewport(page);
});

test("pay groups and holiday calendars use existing commands from the People settings panel", async ({ page }) => {
  const posted: { path: string; body: Record<string, unknown> }[] = [];
  await page.route(/\/api\/v1\/.*\/(?:people\/pay-groups|leave\/admin)$/, async route => {
    if (route.request().method() !== "POST") return route.continue();
    posted.push({ path: new URL(route.request().url()).pathname, body: route.request().postDataJSON() });
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ data: { id: "browser-settings-command" } }) });
  });
  await page.goto(`${base}/people?panel=settings&section=pay-groups`);
  const panel = page.getByTestId("people-detail");
  await panel.locator("summary").filter({ hasText: /^Configure pay group/ }).click();
  await panel.getByLabel("Name", { exact: true }).fill("Browser monthly payroll");
  await panel.getByRole("button", { name: "Save pay group policy", exact: true }).click();
  await expect.poll(() => posted.length).toBe(1);
  expect(posted[0]).toMatchObject({ path: `/api/v1${base}/people/pay-groups`, body: { name: "Browser monthly payroll", countryCode: "TH", currency: "THB", frequency: "monthly", timezone: "Asia/Bangkok", overtimeMultiplierBps: 15000, prorationMethod: "calendar_days" } });
  await panel.getByRole("button", { name: "All HR settings", exact: true }).click();
  await panel.getByRole("navigation", { name: "HR settings topics" }).getByRole("button", { name: /^Holiday calendars/ }).click();
  await panel.locator("summary").filter({ hasText: /^Create holiday calendar/ }).click();
  await panel.getByLabel("Calendar name", { exact: true }).fill("Browser holiday calendar");
  await panel.getByRole("button", { name: "Create calendar", exact: true }).click();
  await expect.poll(() => posted.length).toBe(2);
  expect(posted[1]).toMatchObject({ path: `/api/v1${base}/leave/admin`, body: { resource: "calendar", name: "Browser holiday calendar", timezone: "Asia/Bangkok" } });
  await fitsViewport(page);
  await page.goto(`${base}/people/payroll/settings`);
  await expect(page.getByRole("heading", { name: "Pay groups", exact: true })).toHaveCount(0);
});
