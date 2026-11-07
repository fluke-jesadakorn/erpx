import { captureCaseScreenshot } from "../../support/helpers/case-screenshot";
import { browserProjects } from "../../../scripts/testing/browser-results";
import {
  test,
  expect as baseExpect,
  type BrowserContext,
  type Page,
} from "@playwright/test";
import { readFileSync, existsSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { selectFixtureAssignment } from "../../support/browser/select-fixture-assignment";
import type { WorkSummary } from "@erpx/server-work_projection/public/work-summary";
const file = process.env.ERPX_E2E_FIXTURE ?? ".scratch/work-cards/fixture.json";
type Account = {
  email: string;
  userId: string;
  assignmentId: string;
  company: string;
  branch: string;
  department: string;
};
const fixture: { password: string; accounts: Record<string, Account> } =
  existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : null;
test.skip(!fixture, "Run the isolated Work Cards fixture script first.");
test.setTimeout(120000);
const expect = baseExpect.configure({ timeout: 20000 }),
  base = "/holdings/acme",
  api = `/api/v1${base}`;
const origin = process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102";
async function login(context: BrowserContext, who = "owner") {
  const actor = fixture.accounts[who], loginPage = await context.newPage();
  try {
    await loginPage.goto("/login");
    await loginPage.getByRole("textbox", { name: /Work email/ }).fill(actor.email);
    await loginPage.getByLabel(/^Password(?:\s*\*)?$/).fill(fixture.password);
    const signedIn = loginPage.waitForResponse(response => new URL(response.url()).pathname === "/api/auth/sign-in/email" && response.request().method() === "POST");
    await loginPage.getByRole("button", { name: "Sign in", exact: true }).click();
    const response = await signedIn;
    expect(response.ok(), `Sign-in returned HTTP ${response.status()}`).toBeTruthy();
    await selectFixtureAssignment(context, actor.company, actor.assignmentId);
  } finally { await loginPage.close(); }
}
async function contextHeaders(context: BrowserContext) {
  const response = await context.request.get(`${api}/assignments/active`);
  expect(response.ok(), await response.text()).toBe(true);
  const { data } = await response.json() as { data: { valid: boolean; companyId: string; assignmentId: string | null; revision: number } };
  expect(data.valid).toBe(true); expect(data.assignmentId).toBeTruthy();
  return { Origin: origin, "x-erpx-context-company": data.companyId, "x-erpx-context-assignment": data.assignmentId!, "x-erpx-context-revision": String(data.revision) };
}
async function preferences(context: BrowserContext) {
  const response = await context.request.get(`${api}/work/card-preferences`);
  expect(response.ok(), await response.text()).toBeTruthy();
  return (await response.json()).data as {
    pinnedIds: string[] | null;
    version: number;
  };
}
async function setPreferences(
  context: BrowserContext,
  pinnedIds: string[] | null,
) {
  const current = await preferences(context);
  const response = await context.request.post(`${api}/work/card-preferences`, {
    headers: { ...await contextHeaders(context), "Idempotency-Key": randomUUID() },
    data: { pinnedIds, expectedVersion: current.version },
  });
  expect(response.ok(), await response.text()).toBeTruthy();
  return (await response.json()).data;
}
async function showShelf(page: Page) {
  await page.getByRole("button", { name: "App launcher", exact: true }).filter({ visible: true }).click();
  const launcher = page.getByRole("dialog", { name: "App launcher", exact: true });
  await expect(launcher).toBeVisible();
  const shelf = shortcutShelf(page);
  await expect(shelf).toBeVisible();
  await expect(shelf.getByRole("button", { name: "Customize", exact: true })).toHaveCount(0);
  return shelf;
}
function shortcutShelf(page: Page) {
  return page.getByRole("dialog", { name: "App launcher", exact: true })
    .getByRole("region", { name: "My shortcuts", exact: true });
}
async function closeShelf(page: Page) {
  const launcher = page.getByRole("dialog", { name: "App launcher", exact: true });
  await launcher.getByRole("button", { name: "Close dialog", exact: true }).click();
  await expect(launcher).toBeHidden();
}
async function closeDraft(page: Page, name: string) {
  const drawer = page.getByRole("dialog", { name, exact: true });
  await drawer.getByRole("button", { name: "Close dialog", exact: true }).click();
  const confirmation = page.getByRole("dialog", { name: "Discard changes?", exact: true });
  await expect(confirmation).toBeVisible();
  await confirmation.getByRole("button", { name: "Close and keep local draft", exact: true }).click();
  await expect(drawer).toBeHidden();
}
test("members without profiles use cards to create drafts and preserve filters, refresh and Back", async ({
  page,
  context,
}) => {
  await login(context, "missing");
  await setPreferences(context, null);
  await page.goto(
    `${base}/work?queue=waiting&type=service_request&responsibility=requested&q=printer`,
  );
  const shortcuts = await showShelf(page);
  await expect(
    shortcuts.getByRole("article", { name: "Expense claims", exact: true }),
  ).toBeVisible();
  await expect(
    shortcuts.getByRole("article", { name: "Tasks", exact: true }),
  ).toHaveCount(0);
  const workUrl = page.url();
  const openService = async () => {
    await shortcutShelf(page).getByRole("article", { name: "Service requests", exact: true })
      .getByRole("link", { name: /New service request/ }).click();
    await expect(page.getByRole("dialog", { name: "App launcher", exact: true })).toBeHidden();
    const drawer = page.getByRole("dialog", { name: "Service request", exact: true });
    await expect(drawer).toBeVisible();
    await expect(page).toHaveURL(workUrl);
    return drawer;
  };
  let drawer = await openService();
  const title = `Recover ${randomUUID()}`;
  await drawer.getByRole("textbox", { name: "Title", exact: true }).fill(title);
  await closeDraft(page, "Service request");
  await page.reload();
  await showShelf(page);
  drawer = await openService();
  await expect(drawer.getByRole("textbox", { name: "Title", exact: true })).toHaveValue(title);
  await drawer.getByRole("button", { name: "Close dialog", exact: true }).click();
  await expect(drawer).toBeHidden();
  await showShelf(page);
  await shortcutShelf(page)
    .getByRole("article", { name: "Expense claims", exact: true })
    .locator(`a[href="${base}/work?type=expense&queue=all#work-results"]`)
    .click();
  await expect(page).toHaveURL(/type=expense&queue=all#work-results$/);
  expect(new URL(page.url()).searchParams.has("q")).toBe(false);
  await expect(page.getByRole("dialog", { name: "App launcher", exact: true })).toBeHidden();
  await page.goBack();
  await expect(page).toHaveURL(workUrl);
  await showShelf(page);
  drawer = await openService();
  await expect(drawer.getByRole("textbox", { name: "Title", exact: true })).toHaveValue(title);
  await drawer.getByRole("button", { name: "Close dialog", exact: true }).click();
  await expect(drawer).toBeHidden();
});
test("pin order syncs between devices and conflicts preserve the proposed arrangement", async ({
  page,
  context,
  browser,
}, info) => {
  await login(context);
  await setPreferences(context, ["expenses", "services"]);
  await page.goto(`${base}/work`);
  await showShelf(page);
  await expect(shortcutShelf(page).getByRole("article")).toHaveCount(2);
  expect(await shortcutShelf(page).getByRole("article").evaluateAll(nodes => nodes.map(n => n.getAttribute("aria-label")))).toEqual(["Expense claims", "Service requests"]);
  await expect(page.getByRole("dialog", { name: "Customize shortcuts", exact: true })).toHaveCount(0);
  const proposed = ["services", "expenses"], previous = await preferences(context);
  await setPreferences(context, proposed);
  const device = await browser.newContext(info.project.use);
  try {
    const other = await device.newPage();
    await login(device);
    await other.goto(`${base}/work`);
    await showShelf(other);
    await expect(shortcutShelf(other).getByRole("article")).toHaveCount(2);
    const names = await shortcutShelf(other)
      .getByRole("article")
      .evaluateAll((nodes) => nodes.map((n) => n.getAttribute("aria-label")));
    expect(names).toEqual(["Service requests", "Expense claims"]);
    await setPreferences(device, ["expenses"]);
    const stale = await context.request.post(`${api}/work/card-preferences`, {
      headers: { ...await contextHeaders(context), "Idempotency-Key": randomUUID() },
      data: { pinnedIds: proposed, expectedVersion: previous.version },
    });
    expect(stale.status()).toBe(409);
    expect((await preferences(context)).pinnedIds).toEqual(["expenses"]);
    await closeShelf(page);
    await page.reload();
    await showShelf(page);
    await expect(shortcutShelf(page).getByRole("article")).toHaveCount(1);
    await expect(shortcutShelf(page).getByRole("article", { name: "Expense claims", exact: true })).toBeVisible();
    // An existing empty selection remains empty and still offers All actions.
    await setPreferences(device, []);
    await closeShelf(page);
    await page.reload();
    await showShelf(page);
    await expect(shortcutShelf(page).getByRole("article")).toHaveCount(0);
    await expect(shortcutShelf(page)).toContainText("Open All actions");
    await shortcutShelf(page).getByRole("button", { name: "All actions", exact: true }).click();
    await expect(page.getByRole("dialog", { name: "All actions", exact: true }).getByRole("link", { name: "Expense claims", exact: true })).toBeVisible();
  } finally { await device.close(); }
});
test("preference APIs reject forged authority and beta only offers supported actions", async ({
  page,
  context,
  browser,
}, info) => {
  const anonymous = await browser.newContext(info.project.use);
  try {
    expect(
      (await anonymous.request.get(`${api}/work/card-preferences`)).status(),
    ).toBe(401);
  } finally { await anonymous.close(); }
  await login(context, "beta-missing");
  await setPreferences(context, null);
  const current = await preferences(context);
  for (const data of [
    { pinnedIds: ["admin"], expectedVersion: current.version },
    {
      pinnedIds: [],
      expectedVersion: current.version,
      userId: fixture.accounts.owner.userId,
    },
  ])
    expect(
      (
        await context.request.post(`${api}/work/card-preferences`, {
          headers: { ...await contextHeaders(context), "Idempotency-Key": randomUUID() },
          data,
        })
      ).status(),
    ).toBe(422);
  expect(
    (
      await context.request.post(`${api}/work/card-preferences`, {
        headers: { ...await contextHeaders(context), "Idempotency-Key": randomUUID() },
        data: { pinnedIds: ["counts"], expectedVersion: current.version },
      })
    ).status(),
  ).toBe(403);
  expect(
    (
      await context.request.post(`${api}/work/card-preferences`, {
        headers: {
          ...await contextHeaders(context),
          "Idempotency-Key": randomUUID(),
          Origin: "https://untrusted.example",
        },
        data: { pinnedIds: [], expectedVersion: current.version },
      })
    ).status(),
  ).toBe(403);
  await page.goto(`${base}/work`);
  await showShelf(page);
  await shortcutShelf(page).getByRole("button", { name: "All actions", exact: true }).click();
  const allActions = page.getByRole("dialog", { name: "All actions" });
  await expect(
    allActions.getByRole("link", { name: /Start from a conversation/ }),
  ).toHaveAttribute("href", `${base}/home`);
  await expect(
    allActions.getByRole("link", { name: "Stock counts", exact: true }),
  ).toHaveCount(0);
  await expect(
    allActions.getByRole("link", { name: "Warehouse transfers", exact: true }),
  ).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(allActions).toBeHidden();
  await closeShelf(page);
  await page.getByRole("button", { name: "New", exact: true }).click();
  await expect(
    page
      .getByRole("dialog")
      .getByRole("link", { name: "Expense claim", exact: true }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
});
test("focus refresh updates preferences and incomplete summaries keep other work usable", async ({
  page,
  context,
}) => {
  await login(context, "member");
  await setPreferences(context, ["expenses", "services"]);
  await page.goto(`${base}/work`);
  await showShelf(page);
  await expect(
    page.getByRole("dialog", { name: "App launcher", exact: true }).getByRole("region", { name: "My shortcuts", exact: true }).getByRole("button", { name: "Customize", exact: true }),
  ).toHaveCount(0);
  await setPreferences(context, ["services"]);
  await expect
    .poll(async () => {
      await page.evaluate(() => window.dispatchEvent(new Event("focus")));
      return shortcutShelf(page)
        .getByRole("article", { name: "Expense claims", exact: true })
        .count();
    })
    .toBe(0);
  await expect(
    shortcutShelf(page).getByRole("article", { name: "Expense claims", exact: true }),
  ).toHaveCount(0);
  // Read the real summary before intercepting. Closing the drawer cancels its
  // request, so the interceptor must not wait on another network request before
  // fulfilling a route that may already have been cancelled.
  const response = await context.request.get(`${api}/work/summary`, {
    headers: await contextHeaders(context),
  });
  expect(response.ok(), await response.text()).toBeTruthy();
  const body = (await response.json()) as { data: WorkSummary };
  body.data.preferencesUnavailable = true;
  body.data.preferences = { pinnedIds: null, version: body.data.preferences.version + 1 };
  body.data.cards = body.data.cards.map((c) =>
    c.id === "expenses"
      ? { ...c, incomplete: true, toDo: null, waiting: null }
      : c,
  );
  await page.route("**/work/summary", async (route) => {
    await route.fulfill({ response, json: body });
  });
  await expect
    .poll(async () => {
      await page.evaluate(() => window.dispatchEvent(new Event("focus")));
      const card = shortcutShelf(page).getByRole("article", {
        name: "Expense claims",
        exact: true,
      });
      return (await card.count()) ? card.textContent() : "";
    })
    .toContain("Summary incomplete");
  await expect(
    page.getByRole("dialog", { name: "App launcher", exact: true }).getByRole("region", { name: "My shortcuts", exact: true }).getByRole("button", { name: "Retry" }),
  ).toBeVisible();
  await expect(
    page.getByRole("dialog", { name: "App launcher", exact: true }).getByRole("region", { name: "My shortcuts", exact: true }).getByRole("button", { name: "Customize", exact: true }),
  ).toHaveCount(0);
  await shortcutShelf(page).getByRole("button", { name: "All actions", exact: true }).click();
  const allActions = page.getByRole("dialog", { name: "All actions", exact: true });
  await expect(allActions.getByRole("link", { name: "Service requests", exact: true })).toBeVisible();
  await allActions.getByRole("button", { name: "Close dialog", exact: true }).click();
  await expect(allActions).toBeHidden();
  await closeShelf(page);
  await expect(
    page.getByRole("region", { name: "Work results" }),
  ).toBeVisible();
  await page.unrouteAll({ behavior: "wait" });
});
test("an expense saved from a card appears once in the matching list and summary", async ({
  page,
  context,
}) => {
  await login(context, "missing");
  await setPreferences(context, ["expenses"]);
  await page.goto(`${base}/work?queue=waiting&type=service_request&q=receipt`);
  await showShelf(page);
  const workUrl = page.url();
  await shortcutShelf(page)
    .getByRole("article", { name: "Expense claims", exact: true })
    .getByRole("link", { name: /New expense claim/ })
    .click();
  const drawer = page.getByRole("dialog", { name: "Expense claim", exact: true });
  await expect(drawer).toBeVisible();
  await expect(page.getByRole("dialog", { name: "App launcher", exact: true })).toBeHidden();
  await expect(page).toHaveURL(workUrl);
  const merchant = `Card receipt ${randomUUID()}`;
  await drawer.getByRole("textbox", { name: "Merchant", exact: true }).fill(merchant);
  const saved = page.waitForResponse(
    (r) => r.url().endsWith("/expenses") && r.request().method() === "POST",
  );
  await drawer.getByRole("button", { name: "Save draft", exact: true }).click();
  const response = await saved;
  expect(response.ok(), await response.text()).toBeTruthy();
  const id = (await response.json()).data.id;
  await expect(page).toHaveURL(new RegExp(`/expenses/${id}\\?return=`));
  await page.goBack();
  await expect(page).toHaveURL(workUrl);
  await showShelf(page);
  await expect(page).toHaveURL(/q=receipt/);
  await shortcutShelf(page)
    .getByRole("article", { name: "Expense claims", exact: true })
    .locator(`a[href="${base}/work?type=expense&queue=all#work-results"]`)
    .click();
  await expect(page).toHaveURL(/type=expense&queue=all#work-results$/);
  await expect(page.getByRole("dialog", { name: "App launcher", exact: true })).toBeHidden();
  const summary = (
    await (await context.request.get(`${api}/work/summary`)).json()
  ).data as WorkSummary;
  const expenses = summary.items.filter((item) => item.type === "expense");
  expect(expenses.filter((item) => item.id === id)).toHaveLength(1);
  const card = summary.cards.find((item) => item.id === "expenses")!;
  expect(card.incomplete).toBe(false);
  expect(card.toDo).toBe(
    expenses.filter((item) => item.queue === "todo").length,
  );
  await expect(
    page
      .getByRole("region", { name: "Work results" })
      .getByRole("link")
      .filter({ hasText: merchant }),
  ).toHaveCount(1);
  await expect(page.getByRole("region", { name: "Work results" }).locator('[data-slot="record-list-item"]').filter({ has: page.locator(`a[href^="${base}/expenses/${id}?"]`).filter({ hasText: merchant }) })).toHaveAttribute("data-tone", "amber");
});
test("team and inventory cards follow company, branch and department scope", async ({
  page,
  context,
}) => {
  await login(context, "owner");
  const actor = fixture.accounts.owner;
  const team = await context.request.post(`${api}/service-teams`, {
    headers: { ...await contextHeaders(context), "Idempotency-Key": randomUUID() },
    data: {
      name: `Card team ${randomUUID()}`,
      members: [{ userId: actor.userId, responsibility: "lead" }],
      coverage: [
        {
          category: "engineering",
          scopeKind: "department",
          scopeNodeId: actor.department,
        },
      ],
    },
  });
  // Repeated runs reuse the existing coverage instead of creating duplicate routing.
  expect([200, 409]).toContain(team.status());
  await setPreferences(context, ["counts", "team", "expenses"]);
  await page.goto(
    `${base}/branches/${actor.branch}/departments/${actor.department}/work`,
  );
  await showShelf(page);
  await expect(
    shortcutShelf(page).getByRole("article", { name: "Team queue", exact: true }),
  ).toBeVisible();
  await expect(
    shortcutShelf(page).getByRole("article", { name: "Stock counts", exact: true }),
  ).toHaveCount(0);
  expect((await preferences(context)).pinnedIds).toContain("counts");
  await closeShelf(page);
  await page.goto(`${base}/work`);
  await showShelf(page);
  await expect(
    shortcutShelf(page).getByRole("article", { name: "Stock counts", exact: true }),
  ).toBeVisible();
});
test("cards fit 390, 768 and 1440px in both themes and retain keyboard focus", browserProjects(["desktop"], "This case explicitly sweeps narrow and wide viewports and hardware keyboard focus once; other card business cases remain required on mobile."), async ({
  page,
  context,
}) => {

  await login(context);
  const summaryResponse = await context.request.get(`${api}/work/summary`);
  expect(summaryResponse.ok(), await summaryResponse.text()).toBe(true);
  const summary = (await summaryResponse.json()).data as WorkSummary;
  expect(summary.cards.length).toBeGreaterThanOrEqual(6);
  await setPreferences(context, summary.cards.slice(0, 6).map(card => card.id));

  for (const width of [390, 768, 1440])
    for (const theme of ["light", "dark"]) {
      await context.addCookies([
        {
          name: "erpx_appearance",
          value: theme,
          url: process.env.ERPX_E2E_BASE_URL ?? "http://localhost:3102",
        },
      ]);
      await page.setViewportSize({ width, height: 940 });
      await page.goto(`${base}/work`);
      await showShelf(page);
      await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
      const region = shortcutShelf(page);
      await expect(
        region.getByRole("button", { name: "Customize", exact: true }),
      ).toHaveCount(0);
      await expect(region.getByRole("article", { includeHidden: true })).toHaveCount(6);
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      ).toBe(true);
      if (width === 390) {
        await expect(region.locator("article:visible")).toHaveCount(4);
        await region
          .getByRole("button", { name: /Show all/ })
          .click();
        await expect(region.locator("article:visible")).toHaveCount(6);
      }
      const small = await region
        .locator("button:visible,a:visible")
        .evaluateAll((nodes) =>
          nodes
            .filter((n) => n.getBoundingClientRect().height < 43.5)
            .map((n) => n.textContent),
        );
      expect(small).toEqual([]);
      await page.evaluate(() => {
        (document.activeElement as HTMLElement)?.blur();
        window.scrollTo(0, 0);
      });
      await page.mouse.move(0, 0);
      await captureCaseScreenshot(page, `work-${width}-${theme}.png`, { fullPage: true });
      const trigger = region.getByRole("button", {
        name: "All actions",
        exact: true,
      });
      await trigger.focus();
      await page.keyboard.press("Enter");
      const dialog = page.getByRole("dialog", { name: "All actions", exact: true });
      await expect(dialog).toBeVisible();
      for (let i = 0; i < 20; i++) {
        await page.keyboard.press("Tab");
        await expect.poll(() => dialog.evaluate((el) => el.contains(document.activeElement)), { timeout: 1000 }).toBe(true);
      }
      await captureCaseScreenshot(page, `all-actions-${width}-${theme}.png`, { fullPage: true });
      await page.keyboard.press("Escape");
      await expect(trigger).toBeFocused();
      await closeShelf(page);
    }
});
