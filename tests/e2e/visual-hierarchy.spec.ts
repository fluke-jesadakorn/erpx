import { expect, test } from "@playwright/test";
import type { WorkSummary } from "../../src/server/work-summary";
import type { WorkItem } from "../../src/lib/work-item";

test.beforeEach(async ({ context, baseURL }) => {
  test.skip(!["localhost", "127.0.0.1"].includes(new URL(baseURL!).hostname), "Uses the local development roster.");
  const response = await context.request.post("/api/auth/sign-in/email", {
    data: { email: "mali@erpx.dev", password: "Mali-ERPX-2026!" },
  });
  expect(response.status()).toBe(200);
});

for (const theme of ["light", "dark"] as const) {
  for (const width of [390, 1024, 1440]) {
    test(`Today priorities and feed at ${width}px in ${theme}`, async ({ page, context, baseURL }) => {
      await context.addCookies([{ name: "erpx_appearance", value: theme, url: baseURL! }]);
      await page.setViewportSize({ width, height: 900 });
      await page.goto("/holdings/acme/home");
      await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
      const priority = page.locator("#home-priorities");
      const feed = page.getByRole("region", { name: "Team updates", exact: true });
      await expect(priority.getByRole("region", { name: "Needs you" })).toBeVisible();
      await expect(feed).toBeVisible();
      await expect.poll(async () => priority.locator("li").count()).toBeGreaterThan(0);
      expect(await priority.evaluate(node => Boolean(node.compareDocumentPosition(document.querySelector('[aria-label="Team updates"]')) & Node.DOCUMENT_POSITION_FOLLOWING))).toBe(true);
      await expect(page.getByRole("region", { name: "Needs you" })).toHaveCount(1);
      await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      const first = priority.locator("li").first();
      await expect(first.locator("a")).toBeVisible();
      await expect(first).toContainText(/Due|Overdue|Blocked|Request|Prepare|Review|Submit|Update|Open/i);
    });
  }
}

test("priority cards distinguish blockers, overdue work and undated work", async ({ page, context }) => {
  const response = await context.request.get("/api/v1/holdings/acme/work/summary");
  expect(response.ok()).toBe(true);
  const summary = (await response.json()).data as WorkSummary;
  const dueAt = Date.now() - 86_400_000;
  const items: WorkItem[] = [
    { id: "visual-overdue", type: "expense", title: "Overdue expense review", detail: "Review expense", nextAction: "Review expense", href: "/expenses/visual-overdue", status: "pending", queue: "todo", dueAt, updatedAt: 3 },
    { id: "visual-blocked", type: "expense", title: "Blocked expense review", detail: "Review blocked claim", nextAction: "Review blocked claim", blocker: "Waiting for receipt", href: "/expenses/visual-blocked", status: "pending", queue: "todo", updatedAt: 2 },
    { id: "visual-undated", type: "expense", title: "Undated expense review", detail: "Review undated claim", nextAction: "Review undated claim", href: "/expenses/visual-undated", status: "pending", queue: "todo", updatedAt: 1 },
  ];
  await page.route("**/api/v1/holdings/acme/work/summary", route => route.fulfill({ json: { data: { ...summary, items, unavailableSources: [], capabilities: { ...summary.capabilities, unavailable: [] } } } }));
  await page.goto("/holdings/acme/home");
  const priority = page.locator("#home-priorities");
  await expect(priority.locator("li")).toHaveCount(3);
  await expect(priority.getByText("Overdue ·", { exact: false })).toBeVisible();
  await expect(priority.getByText("Blocked · Waiting for receipt")).toBeVisible();
  await expect(priority.getByText("Undated expense review")).toBeVisible();
  await expect(priority.locator("li").filter({ hasText: "Undated expense review" })).not.toContainText("Due ·");
});

test("keyboard entry reaches the main content", async ({ page }) => {
  await page.goto("/holdings/acme/home");
  await page.keyboard.press("Tab");
  const skip = page.getByRole("link", { name: "Skip to content" });
  await expect(skip).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.locator("#workspace-content")).toBeFocused();
});
