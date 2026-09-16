import { expect, test } from "@playwright/test";
import { mkdirSync } from "node:fs";
import type { WorkSummary } from "../../src/server/work-summary";
import type { WorkItem } from "../../src/lib/work-item";

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
});

for (const size of [{ width: 1440, height: 900 }, { width: 1024, height: 768 }, { width: 390, height: 844 }]) {
  for (const theme of ["light", "dark"]) {
    test(`three working surfaces at ${size.width} in ${theme}`, async ({ page, context, baseURL }) => {
      test.setTimeout(180_000);
      await context.addCookies([{ name: "erpx_appearance", value: theme, url: baseURL! }]);
      await page.setViewportSize(size);
      mkdirSync(".scratch/workplace-design", { recursive: true });
      for (const route of ["home", "work", "inbox"]) {
        await page.goto(`${base}/${route}`);
        await expect(page.getByRole("button", { name: "Search workspace", exact: true })).toBeEnabled({ timeout: 40_000 });
        await expect(page.locator("main")).toBeVisible();
        await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
        expect(await page.locator("body").evaluate(el => getComputedStyle(el).backgroundColor)).toBe(theme === "light" ? "rgb(245, 244, 240)" : "rgb(25, 27, 31)");
        const tokens = await page.locator("html").evaluate(el => Object.fromEntries(
          ["ink", "ink-strong", "muted", "card", "paper", "brand", "on-brand", "amber", "amber-soft", "red", "red-soft", "green", "green-soft"].map(name => [name, getComputedStyle(el).getPropertyValue(`--${name}`).trim()]),
        ));
        for (const [foreground, background] of [["ink", "card"], ["ink-strong", "paper"], ["muted", "card"], ["muted", "paper"], ["brand", "card"], ["on-brand", "brand"], ["amber", "amber-soft"], ["red", "red-soft"], ["green", "green-soft"]]) {
          expect(contrast(tokens[foreground], tokens[background]), `${foreground} on ${background}`).toBeGreaterThanOrEqual(4.5);
        }
        if (route === "home") {
          await expect(page.getByRole("region", { name: "Needs attention", exact: true })).toBeVisible();
          await expect(page.getByText("Share an update", { exact: true })).toBeVisible();
          if (size.width === 1440) {
            const feed = await page.getByRole("region", { name: "Team updates", exact: true }).boundingBox();
            const attention = await page.getByRole("region", { name: "Needs attention", exact: true }).boundingBox();
            expect(attention!.x).toBeGreaterThan(feed!.x + feed!.width);
            expect(feed!.y).toBeLessThan(size.height - 100);
          }
        }
        if (route === "work") {
          const shortcuts = page.getByRole("region", { name: "Your shortcuts", exact: true });
          await expect(shortcuts.getByRole("button", { name: "Customize", exact: true })).toBeEnabled();
          if (size.width === 1440) {
            const results = await page.getByRole("region", { name: "Work results", exact: true }).boundingBox();
            expect(results!.y).toBeLessThan(size.height - 100);
          }
          expect(await shortcuts.locator("a:visible,button:visible").evaluateAll(nodes => nodes.filter(n => n.getBoundingClientRect().height < 43.5).map(n => n.textContent))).toEqual([]);
          const customize = shortcuts.getByRole("button", { name: "Customize", exact: true });
          await customize.click();
          await expect(page.getByRole("dialog", { name: "Customize shortcuts", exact: true })).toBeVisible();
          await page.keyboard.press("Escape");
          await expect(customize).toBeFocused();
        }
        if (route === "inbox") {
          await expect(page.getByRole("button", { name: "New chat", exact: true })).toBeEnabled();
          if (size.width > 760) await expect(page.getByRole("button", { name: "Start a chat", exact: true })).toBeHidden();
          await page.getByRole("button", { name: "Updates", exact: true }).click();
          await expect(page.getByRole("region", { name: "Work updates", exact: true })).toBeVisible();
          await page.getByRole("button", { name: "Chats", exact: true }).click();
        }
        await page.evaluate(() => window.scrollTo(0, 0));
        await page.screenshot({ path: `.scratch/workplace-design/${route}-${size.width}-${theme}.png` });
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
  let mode: "ok" | "failure" | "revoked" = "ok";
  await page.route(`**${api}`, route => route.fulfill({
    status: mode === "ok" ? 200 : mode === "failure" ? 503 : 403,
    json: mode === "ok" ? { data: { ...summary, items: rows, unavailableSources: [], capabilities: { ...summary.capabilities, unavailable: [] } } } : { error: { message: "Design verification unavailable" } },
  }));
  for (const route of ["work", "home"]) {
    mode = "ok";
    await page.goto(`${base}/${route}${route === "work" ? "?queue=completed&q=unmatched" : ""}`);
    await expect(page.getByRole("button", { name: "Search workspace", exact: true })).toBeEnabled();
    const attention = page.getByRole("region", { name: "Needs attention", exact: true });
    // The shell can hydrate before the streamed Home client installs its listener.
    await expect.poll(async () => {
      await page.evaluate(() => window.dispatchEvent(new Event("focus")));
      return attention.getByRole("heading").textContent();
    }).toBe("Needs attention 5");
    await expect(attention.locator("li")).toHaveCount(3);
    await expect(attention.locator("li").first()).toContainText("Assign a responsible teammate");
    mode = "failure";
    await page.evaluate(() => window.dispatchEvent(new Event("focus")));
    await expect(attention.getByRole("status")).toBeVisible();
    await expect(attention.locator("li")).toHaveCount(3);
    mode = "ok";
    await attention.getByRole("button", { name: "Retry work", exact: true }).click();
    await expect(attention.getByRole("status")).toHaveCount(0);
    mode = "revoked";
    await page.evaluate(() => window.dispatchEvent(new Event("focus")));
    await expect(attention.locator("li")).toHaveCount(0);
    await expect(attention).not.toContainText("You’re all caught up");
  }
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
  await page.route(`**${api}`, route => route.fulfill({ json: { data: { ...summary, items: [row], unavailableSources: [] } } }));
  await page.goto(`${base}/work?queue=todo&type=expense&q=Travel`);
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
  expect(await page.locator("body").evaluate(el => getComputedStyle(el).backgroundColor)).toBe("rgb(25, 27, 31)");
  expect(await page.getByRole("button", { name: "Chats", exact: true }).evaluate(el => getComputedStyle(el).transitionDuration)).toBe("1e-05s");
  await page.emulateMedia({ colorScheme: "light" });
  expect(await page.locator("body").evaluate(el => getComputedStyle(el).backgroundColor)).toBe("rgb(245, 244, 240)");
});
