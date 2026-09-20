import { expect, test } from "@playwright/test";

test.beforeEach(async ({ context }) => {
  const response = await context.request.post("/api/auth/sign-in/email", {
    data: {
      email: process.env.ERPX_E2E_ADMIN_EMAIL ?? "jesadakorn.kirtnu@gmail.com",
      password: process.env.ERPX_E2E_ADMIN_PASSWORD ?? "12345678",
    },
  });
  expect(response.status(), await response.text()).toBe(200);
});

test("People settings policy guidance spans the workspace grid", async ({ page }) => {
  await page.goto("/holdings/acme/people/settings");
  const policy = page.locator("aside").filter({ hasText: "Policy model" });
  await expect(policy).toBeVisible({ timeout: 30_000 });

  const box = await policy.boundingBox();
  expect(box).not.toBeNull();
  expect(box!.width).toBeGreaterThan(page.viewportSize()!.width * 0.7);
  expect(box!.height).toBeLessThan(160);
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1),
  ).toBe(true);
});
