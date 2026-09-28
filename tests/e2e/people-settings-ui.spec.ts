import { expect, test } from "@playwright/test";

const base = "/holdings/acme";
test.setTimeout(120_000);
test.beforeEach(async ({ context }) => {
  const response = await context.request.post("/api/auth/sign-in/email", {
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

test("People settings opens a short overview and keeps sections separate", async ({ page }) => {
  await page.goto(`${base}/people/settings`);
  const sections = page.getByRole("navigation", { name: "People settings sections" });
  await expect(sections.getByRole("link", { name: "Overview" })).toHaveAttribute("aria-current", "page");
  await expect(sections.getByRole("link", { name: "Custom attributes" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Create leave type" })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "HR → IT handoff" })).toHaveCount(0);
  await fitsViewport(page);

  await sections.getByRole("link", { name: "Leave & overtime" }).click();
  await expect(page).toHaveURL(/section=leave/, { timeout: 60_000 });
  await expect(page.getByRole("button", { name: "Create leave type" })).toBeVisible();
  await expect(page.locator("[data-assignment-row]")).toHaveCount(10);
  await expect(page.getByRole("heading", { name: "HR → IT handoff" })).toHaveCount(0);
  await page.getByRole("button", { name: "Create leave type" }).click();
  const dialog = page.getByRole("dialog", { name: "Create leave type" });
  await expect(dialog.getByLabel("Code")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).not.toBeVisible();
  await fitsViewport(page);

  await sections.getByRole("link", { name: "HR templates" }).click();
  await expect(page).toHaveURL(/section=templates/, { timeout: 60_000 });
  await expect(page.getByRole("heading", { name: "HR → IT handoff" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Create leave type" })).toHaveCount(0);
  await expect(page.getByRole("link", { name: "Advanced organization templates" })).toBeVisible();
  await fitsViewport(page);

  await page.goto(`${base}/people/settings?section=overview`);
  await expect(page.getByRole("navigation", { name: "People settings sections" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Create leave type" })).toHaveCount(0);
});

test("leave policy wizard preserves draft and submits one reviewed command", async ({ page }) => {
  await page.goto(`${base}/people/settings/leave-policy/new`);
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
  await expect(page.getByRole("heading", { name: "Fields that fit your organization" })).toBeVisible();
  await fitsViewport(page);
});
