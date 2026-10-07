import { expect, test, type Page } from "@playwright/test";
import type { WorkSummary } from "../../src/server/modules/work_projection/public/work-summary";
import { projectWorkPage } from "../../src/lib/work-page";
import { selectFixtureAssignment } from "./helpers/select-fixture-assignment";
import type { WorkItem } from "../../src/lib/work-item";

test.beforeEach(async ({ context, baseURL }) => {
  test.skip(!["localhost", "127.0.0.1"].includes(new URL(baseURL!).hostname), "Uses the local development roster.");
  const response = await context.request.post("/api/auth/sign-in/email", {
    data: { email: "mali@erpx.dev", password: "Mali-ERPX-2026!" },
  });
  expect(response.status()).toBe(200);
  await selectFixtureAssignment(context, "acme-co", "seed_usr_mali");
});

async function openTodayTools(page: Page) {
  const toggle = page.locator("#today-tools-toggle");
  const compact = await toggle.evaluate(el => getComputedStyle(el.parentElement!).display !== "none");
  if (compact) { await expect(toggle).toBeVisible(); await toggle.click(); }
  else await expect(toggle).toBeHidden();
  await expect(page.getByRole("complementary", { name: "Today tools", exact: true })).toBeVisible();
}
async function refreshWork(page: Page) {
  const response = page.waitForResponse(r => new URL(r.url()).pathname === "/api/v1/holdings/acme/work/summary" && r.request().method() === "GET");
  const other = await page.context().newPage();
  try { await other.goto("about:blank"); await page.bringToFront(); expect((await response).ok()).toBe(true); }
  finally { await other.close(); }
}

for (const theme of ["light", "dark"] as const) {
  for (const width of [390, 1024, 1440]) {
    test(`Today priorities and feed at ${width}px in ${theme}`, async ({ page, context, baseURL }) => {
      await context.addCookies([{ name: "erpx_appearance", value: theme, url: baseURL! }]);
      await page.setViewportSize({ width, height: 900 });
      await page.goto("/holdings/acme/home");
      await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
      await openTodayTools(page);
      const priority = page.getByRole("region", { name: "Needs you", exact: true });
      const feed = page.getByRole("region", { name: "Team updates", exact: true });
      await expect(priority).toBeVisible();
      await expect(feed).toBeVisible();
      await expect.poll(async () => priority.locator("li").count()).toBeGreaterThan(0);
      const priorityBox = await priority.boundingBox(), feedBox = await feed.boundingBox();
      expect(priorityBox).not.toBeNull(); expect(feedBox).not.toBeNull();
      if (width <= 1024) expect(priorityBox!.y).toBeLessThan(feedBox!.y);
      else expect(priorityBox!.x).toBeGreaterThan(feedBox!.x);
      expect(await priority.locator("li").count()).toBeLessThanOrEqual(2);
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
  await page.route("**/api/v1/holdings/acme/work/summary**", route => {
    const snapshot: WorkSummary = { ...summary, items, unavailableSources: [], capabilities: { ...summary.capabilities, unavailable: [] } };
    const paged = new URL(route.request().url()).searchParams.get("paged") === "1";
    return route.fulfill({ json: { data: paged ? projectWorkPage(snapshot, { view: "actions", queue: "todo", type: "all", q: "", responsibility: "all" }) : snapshot } });
  });
  await page.goto("/holdings/acme/home");
  await openTodayTools(page);
  const priority = page.getByRole("region", { name: "Needs you", exact: true });
  await expect(priority.getByRole("heading")).toHaveText("Needs you 3");
  await expect(priority.locator("li")).toHaveCount(2);
  await expect(priority.getByText("Blocked · Waiting for receipt", { exact: true })).toBeVisible();
  // Today keeps two compact previews. My work retains the full dated and
  // undated records, including the real overdue timestamp and blocker.
  await priority.getByRole("link", { name: "View all work", exact: true }).click();
  await expect(page.getByRole("heading", { name: "My work", exact: true })).toBeVisible();
  await refreshWork(page);
  const results = page.getByRole("region", { name: "Work results", exact: true });
  await expect(results.getByRole("article")).toHaveCount(3);
  const overdue = results.getByRole("article").filter({ hasText: "Overdue expense review" });
  await expect(overdue).toContainText("Due");
  const timestamp = await overdue.locator("time").getAttribute("datetime");
  expect(new Date(timestamp!).getTime()).toBe(dueAt);
  expect(new Date(timestamp!).getTime()).toBeLessThan(Date.now());
  await expect(results.getByRole("article").filter({ hasText: "Blocked expense review" })).toContainText("Needs attention: Waiting for receipt");
  const undated = results.getByRole("article").filter({ hasText: "Undated expense review" });
  await expect(undated).toBeVisible(); await expect(undated.locator("time")).toHaveCount(0);
  await expect(undated).not.toContainText("Due ");
});

test("keyboard entry reaches the main content", async ({ page }) => {
  await page.goto("/holdings/acme/home");
  await page.keyboard.press("Tab");
  const skip = page.getByRole("link", { name: "Skip to content" });
  await expect(skip).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.locator("#workspace-content")).toBeFocused();
});
