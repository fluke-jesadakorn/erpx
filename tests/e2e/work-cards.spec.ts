import {
  test,
  expect as baseExpect,
  type BrowserContext,
  type Page,
} from "@playwright/test";
import { readFileSync, existsSync, mkdirSync } from "node:fs";
import { randomUUID } from "node:crypto";
import type { WorkSummary } from "../../src/server/work-summary";
const file = ".scratch/work-cards/fixture.json";
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
test.setTimeout(60000);
const expect = baseExpect.configure({ timeout: 20000 }),
  base = "/holdings/acme",
  api = `/api/v1${base}`;
async function login(context: BrowserContext, who = "owner") {
  const actor = fixture.accounts[who];
  const response = await context.request.post("/api/auth/sign-in/email", {
    data: { email: actor.email, password: fixture.password },
  });
  expect(response.ok(), await response.text()).toBeTruthy();
  const url = process.env.ERPX_E2E_BASE_URL ?? "http://localhost:3102";
  await context.addCookies([
    { name: "erpx_company_h001", value: actor.company, url },
    { name: "erpx_assignment_h001", value: actor.assignmentId, url },
  ]);
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
    headers: { "Idempotency-Key": randomUUID() },
    data: { pinnedIds, expectedVersion: current.version },
  });
  expect(response.ok(), await response.text()).toBeTruthy();
  return (await response.json()).data;
}
async function openEditor(page: Page) {
  await page.getByRole("button", { name: "Customize", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Customize shortcuts" });
  await expect(dialog).toBeVisible();
  return dialog;
}
async function saveEditor(page: Page) {
  const response = page.waitForResponse(
    (r) =>
      r.url().endsWith("/work/card-preferences") &&
      r.request().method() === "POST",
  );
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Save", exact: true })
    .click();
  return response;
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
  const shortcuts = page.getByRole("region", { name: "Your shortcuts" });
  await expect(
    shortcuts.getByRole("article", { name: "Expense claims", exact: true }),
  ).toBeVisible();
  await expect(
    shortcuts.getByRole("article", { name: "Tasks", exact: true }),
  ).toHaveCount(0);
  await shortcuts
    .getByRole("article", { name: "Service requests", exact: true })
    .getByRole("link", { name: "New service request", exact: true })
    .click();
  await expect(page).toHaveURL(/new=service_request/);
  await expect(shortcuts).toHaveCount(0);
  const title = `Recover ${randomUUID()}`;
  await page.getByLabel("Title", { exact: true }).fill(title);
  await page.reload();
  await expect(page.getByLabel("Title", { exact: true })).toHaveValue(title);
  await page.getByRole("link", { name: "Close new work" }).click();
  await expect(page).toHaveURL(/q=printer/);
  await page.goBack();
  await expect(page.getByLabel("Title", { exact: true })).toHaveValue(title);
  await page.getByRole("link", { name: "Close new work" }).click();
  await page
    .getByRole("article", { name: "Expense claims", exact: true })
    .getByRole("link", { name: /Expense claims/ })
    .click();
  await expect(page).toHaveURL(/type=expense&queue=all#work-results$/);
  expect(new URL(page.url()).searchParams.has("q")).toBe(false);
});
test("pin order syncs between devices and conflicts preserve the proposed arrangement", async ({
  page,
  context,
  browser,
}) => {
  await login(context);
  await setPreferences(context, ["expenses", "services"]);
  await page.goto(`${base}/work`);
  let dialog = await openEditor(page);
  await dialog
    .getByRole("button", { name: "Move Service requests up" })
    .focus();
  await page.keyboard.press("Enter");
  expect(
    await dialog.evaluate((el) => el.contains(document.activeElement)),
  ).toBe(true);
  expect((await saveEditor(page)).ok()).toBeTruthy();
  await expect(dialog).not.toBeVisible();
  expect((await preferences(context)).pinnedIds).toEqual([
    "services",
    "expenses",
  ]);
  const device = await browser.newContext(),
    other = await device.newPage();
  await login(device);
  await other.goto(`${base}/work`);
  await expect(other.getByRole("region", { name: "Your shortcuts" }).getByRole("article")).toHaveCount(2);
  const names = await other
    .getByRole("region", { name: "Your shortcuts" })
    .getByRole("article")
    .evaluateAll((nodes) => nodes.map((n) => n.getAttribute("aria-label")));
  expect(names).toEqual(["Service requests", "Expense claims"]);
  dialog = await openEditor(page);
  await dialog.getByRole("button", { name: "Unpin Expense claims" }).click();
  await setPreferences(device, ["expenses"]);
  expect((await saveEditor(page)).status()).toBe(409);
  await expect(dialog.getByRole("alert")).toContainText("changed");
  await expect(dialog.getByRole("listitem")).toHaveCount(1);
  await expect(dialog.getByRole("listitem")).toContainText("Service requests");
  await expect(
    dialog.getByRole("button", { name: "Save", exact: true }),
  ).toBeDisabled();
  await dialog.getByRole("checkbox").check();
  expect((await saveEditor(page)).ok()).toBeTruthy();
  expect((await preferences(device)).pinnedIds).toEqual(["services"]);
  dialog = await openEditor(page);
  await dialog.getByRole("button", { name: "Reset to default" }).click();
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  expect((await preferences(context)).pinnedIds).toEqual(["services"]);
  dialog = await openEditor(page);
  await dialog.getByRole("button", { name: "Reset to default" }).click();
  expect((await saveEditor(page)).ok()).toBeTruthy();
  expect((await preferences(device)).pinnedIds).toBeNull();
  await device.close();
});
test("preference APIs reject forged authority and beta only offers supported actions", async ({
  page,
  context,
  browser,
}) => {
  const anonymous = await browser.newContext();
  expect(
    (await anonymous.request.get(`${api}/work/card-preferences`)).status(),
  ).toBe(401);
  await anonymous.close();
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
          headers: { "Idempotency-Key": randomUUID() },
          data,
        })
      ).status(),
    ).toBe(422);
  expect(
    (
      await context.request.post(`${api}/work/card-preferences`, {
        headers: { "Idempotency-Key": randomUUID() },
        data: { pinnedIds: ["counts"], expectedVersion: current.version },
      })
    ).status(),
  ).toBe(403);
  expect(
    (
      await context.request.post(`${api}/work/card-preferences`, {
        headers: {
          "Idempotency-Key": randomUUID(),
          Origin: "https://untrusted.example",
        },
        data: { pinnedIds: [], expectedVersion: current.version },
      })
    ).status(),
  ).toBe(403);
  await page.goto(`${base}/work`);
  await page.getByRole("button", { name: "All actions", exact: true }).click();
  await expect(
    page
      .getByRole("article", { name: "Tasks", exact: true })
      .getByRole("link", { name: "Start from a conversation" }),
  ).toHaveAttribute("href", `${base}/home`);
  await expect(
    page.getByRole("article", { name: "Stock counts", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("article", { name: "Warehouse transfers", exact: true }),
  ).toHaveCount(0);
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
  await expect(
    page.getByRole("button", { name: "Customize", exact: true }),
  ).toBeEnabled();
  await setPreferences(context, ["services"]);
  await expect
    .poll(async () => {
      await page.evaluate(() => window.dispatchEvent(new Event("focus")));
      return page
        .getByRole("article", { name: "Expense claims", exact: true })
        .count();
    })
    .toBe(0);
  await expect(
    page.getByRole("article", { name: "Expense claims", exact: true }),
  ).toHaveCount(0);
  await page.route("**/work/summary", async (route) => {
    const response = await route.fetch(),
      body = (await response.json()) as { data: WorkSummary };
    body.data.preferencesUnavailable = true;
    body.data.preferences = { pinnedIds: null, version: 0 };
    body.data.unavailableSources = ["Expense claims"];
    body.data.cards = body.data.cards.map((c) =>
      c.id === "expenses"
        ? { ...c, incomplete: true, toDo: null, waiting: null }
        : c,
    );
    await route.fulfill({ response, json: body });
  });
  await expect
    .poll(async () => {
      await page.evaluate(() => window.dispatchEvent(new Event("focus")));
      const card = page.getByRole("article", {
        name: "Expense claims",
        exact: true,
      });
      return (await card.count()) ? card.textContent() : "";
    })
    .toContain("Summary incomplete");
  await expect(
    page.getByRole("article", { name: "Service requests", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Retry shortcuts" }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Customize", exact: true }),
  ).toBeDisabled();
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
  await page
    .getByRole("article", { name: "Expense claims", exact: true })
    .getByRole("link", { name: "New expense claim", exact: true })
    .click();
  const merchant = `Card receipt ${randomUUID()}`;
  await page.getByLabel("Merchant", { exact: true }).fill(merchant);
  const saved = page.waitForResponse(
    (r) => r.url().endsWith("/expenses") && r.request().method() === "POST",
  );
  await page.getByRole("button", { name: "Save draft", exact: true }).click();
  const response = await saved;
  expect(response.ok(), await response.text()).toBeTruthy();
  const id = (await response.json()).data.id;
  await expect(page).toHaveURL(new RegExp(`/expenses/${id}$`));
  await page.goBack();
  await expect(page).toHaveURL(/new=expense/);
  await page.getByRole("link", { name: "Close new work" }).click();
  await expect(page).toHaveURL(/q=receipt/);
  await page
    .getByRole("article", { name: "Expense claims", exact: true })
    .getByRole("link", { name: /Expense claims/ })
    .click();
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
});
test("team and inventory cards follow company, branch and department scope", async ({
  page,
  context,
}) => {
  await login(context, "owner");
  const actor = fixture.accounts.owner;
  const team = await context.request.post(`${api}/service-teams`, {
    headers: { "Idempotency-Key": randomUUID() },
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
  await expect(
    page.getByRole("article", { name: "Team queue", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("article", { name: "Stock counts", exact: true }),
  ).toHaveCount(0);
  const dialog = await openEditor(page);
  await expect(
    dialog.getByText("Kept for another scope or when available again."),
  ).toBeVisible();
  await dialog.getByRole("button", { name: "Pin Service requests" }).click();
  const saved = page.waitForResponse(
    (r) =>
      r.url().endsWith("/work/card-preferences") &&
      r.request().method() === "POST",
  );
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  expect((await saved).ok()).toBeTruthy();
  expect((await preferences(context)).pinnedIds).toContain("counts");
  await page.goto(`${base}/work`);
  await expect(
    page.getByRole("article", { name: "Stock counts", exact: true }),
  ).toBeVisible();
});
test("cards fit 390, 768 and 1440px in both themes and retain keyboard focus", async ({
  page,
  context,
}, info) => {
  test.skip(
    info.project.name !== "desktop",
    "The matrix already includes mobile widths.",
  );
  await login(context);
  await setPreferences(context, null);
  mkdirSync(".scratch/work-cards/screenshots", { recursive: true });
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
      await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
      const region = page.getByRole("region", { name: "Your shortcuts" });
      await expect(
        region.getByRole("button", { name: "Customize", exact: true }),
      ).toBeEnabled();
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      ).toBe(true);
      if (width === 390) {
        await expect(region.locator("article:visible")).toHaveCount(3);
        await region
          .getByRole("button", { name: /Show all .* shortcuts/ })
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
      await page.screenshot({
        path: `.scratch/work-cards/screenshots/work-${width}-${theme}.png`,
        fullPage: true,
      });
      const trigger = region.getByRole("button", {
        name: "Customize",
        exact: true,
      });
      await trigger.focus();
      await page.keyboard.press("Enter");
      const dialog = page.getByRole("dialog", { name: "Customize shortcuts" });
      await expect(dialog).toBeVisible();
      for (let i = 0; i < 20; i++) {
        await page.keyboard.press("Tab");
        expect(
          await dialog.evaluate((el) => el.contains(document.activeElement)),
        ).toBe(true);
      }
      await page.screenshot({
        path: `.scratch/work-cards/screenshots/customize-${width}-${theme}.png`,
        fullPage: true,
      });
      await page.keyboard.press("Escape");
      await expect(trigger).toBeFocused();
    }
});
