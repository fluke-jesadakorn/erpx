import { captureCaseScreenshot } from "../../support/helpers/case-screenshot";
import { expect, test, type Page } from "@playwright/test";
import type { WorkSummary } from "../../../src/server/modules/work_projection/public/work-summary";
import { projectWorkPage } from "../../../src/lib/work_projection/work-page";
import { workAttention } from "../../../src/lib/work_projection/work-attention";
import { selectFixtureAssignment } from "../../support/browser/select-fixture-assignment";
import type { WorkItem } from "../../../src/lib/work_projection/work-item";

const base = "/holdings/acme";
const api = `/api/v1${base}/work/summary`;
function contrast(foreground: string, background: string) {
  const luminance = (hex: string) => {
    const raw = hex.trim().slice(1);
    const expanded = raw.length === 3 ? Array.from(raw).map(c => c + c).join("") : raw;
    const rgb = [0, 2, 4].map(offset => parseInt(expanded.slice(offset, offset + 2), 16) / 255)
      .map(c => c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
    return rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
  };
  const a = luminance(foreground), b = luminance(background);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}
test.beforeEach(async ({ context, baseURL }) => {
  test.skip(!["localhost", "127.0.0.1"].includes(new URL(baseURL!).hostname), "Uses the documented local development roster.");
  const response = await context.request.post("/api/auth/sign-in/email", {
    data: { email: "mali@erpx.dev", password: "Mali-ERPX-2026!" },
  });
  expect(response.status()).toBe(200);
  await selectFixtureAssignment(context, "acme-co", "seed_usr_mali");
});

async function refreshAfterTabReturn(page: Page, endpoint = api) {
  await expect(page.getByRole("button", { name: "Search workspace", exact: true })).toBeEnabled();
  if (new URL(page.url()).pathname === `${base}/work`) await expect(page.getByRole("button", { name: "New", exact: true })).toBeEnabled();
  const response = page.waitForResponse(r => new URL(r.url()).pathname === endpoint && r.request().method() === "GET");
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  return await response;
}
async function openTodayTools(page: Page) {
  const toggle = page.locator("#today-tools-toggle");
  const compact = await toggle.evaluate(el => getComputedStyle(el.parentElement!).display !== "none");
  if (compact) {
    await expect(toggle).toBeVisible(); await toggle.click();
    const plan = page.getByRole("dialog", { name: "Plan my day", exact: true });
    await expect(plan).toBeVisible();
    await plan.getByRole("button", { name: "Close dialog", exact: true }).click();
    await expect(plan).toBeHidden();
  }
  else await expect(toggle).toBeHidden();
  await expect(page.getByRole("complementary", { name: "Today tools", exact: true })).toBeVisible();
}

for (const size of [{ width: 1440, height: 900 }, { width: 1024, height: 768 }, { width: 390, height: 844 }]) {
  for (const theme of ["light", "dark"]) {
    test(`three working surfaces at ${size.width} in ${theme}`, async ({ page, context, baseURL }) => {
      test.setTimeout(180_000);
      await context.addCookies([{ name: "erpx_appearance", value: theme, url: baseURL! }]);
      await page.setViewportSize(size);

      for (const route of ["home", "work", "inbox"]) {
        await page.goto(`${base}/${route}`);
        await expect(page.getByRole("button", { name: "Search workspace", exact: true })).toBeEnabled({ timeout: 40_000 });
        await expect(page.locator("#workspace-content")).toBeVisible();
        const sharedShelf = page.getByRole("region", { name: "My shortcuts", exact: true });
        await expect(sharedShelf.getByRole("button", { name: "Customize", exact: true })).toHaveCount(0);
        if (size.width > 650) {
          await expect(sharedShelf).toBeVisible();
          const shelfBox = await sharedShelf.boundingBox();
          const contentBox = await page.locator("#workspace-content").boundingBox();
          expect(shelfBox!.x + shelfBox!.width).toBeLessThanOrEqual(contentBox!.x + 1);
        } else {
          await expect(sharedShelf).toBeHidden();
          await expect(page.getByRole("navigation", { name: "Mobile navigation", exact: true }).getByRole("button", { name: "Start work", exact: true })).toBeVisible();
        }
        await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
        expect(await page.locator("body").evaluate(el => getComputedStyle(el).backgroundColor)).toBe(theme === "light" ? "rgb(242, 245, 248)" : "rgb(21, 26, 33)");
        const tokens = await page.locator("html").evaluate(el => Object.fromEntries(
          ["ink", "ink-strong", "muted", "card", "paper", "brand", "on-brand", "amber", "amber-soft", "red", "red-soft", "green", "green-soft"].map(name => [name, getComputedStyle(el).getPropertyValue(`--ui-${name}`).trim()]),
        ));
        for (const [foreground, background] of [["ink", "card"], ["ink-strong", "paper"], ["muted", "card"], ["muted", "paper"], ["brand", "card"], ["on-brand", "brand"], ["amber", "amber-soft"], ["red", "red-soft"], ["green", "green-soft"]]) {
          expect(contrast(tokens[foreground], tokens[background]), `${foreground} on ${background}`).toBeGreaterThanOrEqual(4.5);
        }
        if (route === "home") {
          await expect(page.getByRole("navigation", { name: "Staff self-service" })).toHaveCount(0);
          await openTodayTools(page);
          await expect(page.getByRole("region", { name: "Needs you", exact: true }).getByRole("heading")).toBeVisible();
          await expect(page.getByText("Share an update", { exact: true })).toBeVisible();
          if (size.width === 1440) {
            const feed = await page.getByRole("region", { name: "Team updates", exact: true }).boundingBox();
            expect(feed!.y).toBeLessThan(size.height - 100);
          }
        }
        if (route === "work") {
          if (size.width > 650) {
            const shortcuts = page.getByRole("region", { name: "My shortcuts", exact: true });
            expect(await shortcuts.locator("a:visible,button:visible").evaluateAll(nodes => nodes.filter(n => n.getBoundingClientRect().height < 43.5).map(n => n.textContent))).toEqual([]);
            await expect(page.getByRole("dialog", { name: "Customize shortcuts", exact: true })).toHaveCount(0);
          } else {
            const start = page.getByRole("navigation", { name: "Mobile navigation", exact: true }).getByRole("button", { name: "Start work", exact: true });
            await start.click();
            const dialog = page.getByRole("dialog", { name: "Start work", exact: true });
            await expect(dialog).toBeVisible();
            expect(await dialog.getByRole("link").evaluateAll(nodes => nodes.filter(n => n.getBoundingClientRect().height < 43.5).map(n => n.textContent))).toEqual([]);
            await page.keyboard.press("Escape"); await expect(start).toBeFocused();
          }
        }
        if (route === "inbox") {
          await expect(page.getByRole("button", { name: "New chat", exact: true })).toBeEnabled();
          if (size.width > 760) await expect(page.getByRole("button", { name: "Start a chat", exact: true })).toBeHidden();
          await expect(page.getByRole("heading", { name: "Chats", exact: true })).toBeVisible();
        }
        await page.evaluate(() => window.scrollTo(0, 0));
        await captureCaseScreenshot(page, `${route}-${size.width}-${theme}.png`);
      }
    });
  }
}

test("shared attention shows full totals, survives filters, recovers from failure and clears revoked data", async ({ page, context }) => {
  test.setTimeout(120_000);
  const response = await context.request.get(api);
  expect(response.ok()).toBe(true);
  const summary = (await response.json()).data as WorkSummary;
  const rows: WorkItem[] = Array.from({ length: 5 }, (_, index) => ({
    id: `design-${index}`, type: "expense", title: `Design verification ${index} · งานที่ต้องดำเนินการ`, detail: "Review the details", nextAction: "Review the details", href: `/expenses/design-${index}`, status: "pending", queue: index === 0 ? "waiting" : "todo", blocker: index === 0 ? "Assign a responsible teammate" : null, updatedAt: 100 - index,
  }));
  // All five rows are actionable; a blocker remains an action rather than
  // being misclassified as work waiting on somebody else.
  rows[0].queue = "todo";
  let mode: "ok" | "failure" | "revoked" = "ok";
  await page.route(`**${api}**`, route => {
    const snapshot: WorkSummary = { ...summary, items: rows, unavailableSources: [], capabilities: { ...summary.capabilities, unavailable: [] } };
    const query = new URL(route.request().url()).searchParams;
    return route.fulfill({
      status: mode === "ok" ? 200 : mode === "failure" ? 503 : 403,
      json: mode === "ok" ? { data: query.get("paged") === "1" ? projectWorkPage(snapshot, { view: query.get("view") ?? "all", queue: query.get("queue") ?? "all", type: query.get("type") ?? "all", q: query.get("q") ?? "", responsibility: query.get("responsibility") ?? "all" }) : snapshot } : { error: { message: "Design verification unavailable" } },
    });
  });
  const navigationApi = `/api/v1${base}/navigation`;
  const navigation = await context.request.get(`${navigationApi}?include=attention`);
  expect(navigation.ok()).toBe(true);
  const navigationSnapshot = (await navigation.json()).data;
  await page.route(`**${navigationApi}**`, route => {
    if (route.request().method() !== "GET") return route.continue();
    return route.fulfill({ status: mode === "ok" ? 200 : mode === "failure" ? 503 : 403,
      json: mode === "ok" ? { data: { ...navigationSnapshot, work: rows.length,
        attention: workAttention(rows), snapshotAt: Date.now() } } : { error: { message: "Design verification unavailable" } } });
  });
  const readHome = page.waitForResponse(r => new URL(r.url()).pathname === navigationApi && r.request().method() === "POST");
  await page.goto(`${base}/home`);
  expect((await readHome).ok()).toBe(true);
  expect((await refreshAfterTabReturn(page, navigationApi)).ok()).toBe(true);
  await openTodayTools(page);
  const attention = page.getByRole("region", { name: "Needs you", exact: true });
  await expect(attention.getByRole("heading")).toHaveText("Needs you 5");
  await expect(attention.locator("li")).toHaveCount(2);
  await expect(attention.locator("li").first()).toContainText("Assign a responsible teammate");
  mode = "failure"; expect((await refreshAfterTabReturn(page, navigationApi)).status()).toBe(503);
  await expect(attention.getByRole("status")).toBeVisible(); await expect(attention.locator("li")).toHaveCount(2);
  mode = "ok";
  const recovered = page.waitForResponse(r => new URL(r.url()).pathname === navigationApi && r.status() === 200);
  await attention.getByRole("button", { name: "Retry work", exact: true }).click(); await recovered;
  await expect(attention.getByRole("status")).toHaveCount(0);
  mode = "revoked"; expect((await refreshAfterTabReturn(page, navigationApi)).status()).toBe(403);
  await expect(attention.locator("li")).toHaveCount(0); await expect(attention).not.toContainText("You’re all caught up");

  mode = "ok"; await page.goto(`${base}/work?view=all`);
  await expect(page.getByRole("heading", { name: "My work", exact: true })).toBeVisible();
  expect((await refreshAfterTabReturn(page)).ok()).toBe(true);
  const results = page.getByRole("region", { name: "Work results", exact: true });
  const total = page.locator('nav[aria-label="Primary"]:visible,nav[aria-label="Mobile navigation"]:visible').getByRole("link", { name: /^My work/ }).locator('[title="My actions; – means incomplete"]');
  await expect(total).toHaveText("5");
  await expect(results.getByRole("article")).toHaveCount(5);
  await expect(results.getByRole("article").first()).toContainText("Assign a responsible teammate");
  mode = "failure"; expect((await refreshAfterTabReturn(page)).status()).toBe(503);
  await expect(page.getByRole("status").filter({ hasText: "Design verification unavailable" })).toBeVisible();
  await expect(results.getByRole("article")).toHaveCount(5);
  mode = "ok"; expect((await refreshAfterTabReturn(page)).ok()).toBe(true);
  await expect(page.getByRole("status").filter({ hasText: "Design verification unavailable" })).toHaveCount(0);
  // Filtering the list does not change the complete action count in navigation.
  await page.getByLabel("Search work", { exact: true }).fill("unmatched");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(results).toContainText("No matching work");
  await expect(page.getByLabel("Search work", { exact: true })).toHaveValue("unmatched");
  expect((await refreshAfterTabReturn(page)).ok()).toBe(true);
  await expect(results).toContainText("No matching work");
  await expect(total).toHaveText("5");
  mode = "revoked"; expect((await refreshAfterTabReturn(page)).status()).toBe(403);
  await expect(results.getByRole("article")).toHaveCount(0);
  await expect(total).toHaveText("–");
  await expect(results).not.toContainText("You’re all caught up");
  await expect(page.getByRole("status").filter({ hasText: "Design verification unavailable" })).toBeVisible();
});

test("work rows show a real current owner and next handoff, and preserve the return filters", async ({ page, context }) => {
  const response = await context.request.get(api);
  expect(response.ok()).toBe(true);
  const summary = (await response.json()).data as WorkSummary;
  const person = { userId: "design-owner", name: "Nisa Chen · ผู้รับผิดชอบงานฝ่ายบัญชี", teamId: "finance", teamName: "Finance" };
  const current = { key: "review", action: "Review claim", role: "reviewer", teamName: "Finance", primary: person, candidates: [], status: "current" as const, blocker: null };
  const next = { ...current, key: "pay", action: "Settle claim", teamName: "Treasury", primary: null, status: "upcoming" as const };
  const row: WorkItem = {
    id: "design-flow", type: "expense", title: "Travel expenses · ตรวจเอกสารค่าใช้จ่าย", detail: "Review claim", href: "/expenses/design-flow", status: "pending", queue: "todo", updatedAt: 100,
    flow: { kind: "expense", recordId: "design-flow", recordVersion: 1, version: 1, defaultsRevision: 1, state: "pending", current, next, steps: [current, next], canAct: true, canManage: false, blocker: null, history: [] },
  };
  const snapshot: WorkSummary = { ...summary, items: [row], unavailableSources: [], capabilities: { ...summary.capabilities, unavailable: [] } };
  await page.route(`**${api}**`, route => route.fulfill({ json: { data: projectWorkPage(snapshot, { view: "legacy", queue: "todo", type: "expense", q: "Travel", responsibility: "all" }) } }));
  await page.goto(`${base}/work?queue=todo&type=expense&q=Travel`);
  await expect(page.getByRole("heading", { name: "My work", exact: true })).toBeVisible();
  expect((await refreshAfterTabReturn(page)).ok()).toBe(true);
  const results = page.getByRole("region", { name: "Work results", exact: true });
  await expect(results.getByText(person.name, { exact: true })).toBeVisible();
  await expect(results.locator('[aria-current="step"]')).toHaveText("Review claim");
  await expect(results).toContainText("Next: Settle claim");
  await expect(results).toContainText("Treasury");
  await expect(results.getByRole("link")).toHaveAttribute("href", `${base}/expenses/design-flow?return=queue%3Dtodo%26type%3Dexpense%26q%3DTravel`);
  for (const width of [1440, 390]) {
    await page.setViewportSize({ width, height: 900 });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  }
});

test("System follows both color schemes and respects reduced motion", async ({ page, context, baseURL }) => {
  await context.addCookies([{ name: "erpx_appearance", value: "system", url: baseURL! }]);
  await page.emulateMedia({ colorScheme: "dark", reducedMotion: "reduce" });
  await page.goto(`${base}/inbox`);
  await expect(page.locator("html")).toHaveAttribute("data-theme", "system");
  expect(await page.locator("body").evaluate(el => getComputedStyle(el).backgroundColor)).toBe("rgb(21, 26, 33)");
  expect(await page.getByRole("button", { name: /^Messages/ }).evaluate(el => getComputedStyle(el).transitionDuration)).toBe("1e-05s");
  await page.emulateMedia({ colorScheme: "light" });
  expect(await page.locator("body").evaluate(el => getComputedStyle(el).backgroundColor)).toBe("rgb(242, 245, 248)");
});
