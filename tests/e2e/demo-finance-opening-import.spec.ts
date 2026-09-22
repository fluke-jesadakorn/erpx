import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type Credentials = { password: string; users: Array<{ email: string; role: string; userId: string }> };
type Fixture = { browserOpeningChartId: string; browserOpeningCashId: string; browserOpeningEquityId: string; periodId: string };

async function signInAs(context: BrowserContext, page: Page, credentials: Credentials, role: "finance_preparer" | "finance_approver" | "holding_owner", path: string) {
  const user = credentials.users.find(item => item.role === role);
  expect(user, `${role} demo account`).toBeTruthy();
  const login = await context.request.post("/api/auth/sign-in/email", { data: { email: user!.email, password: credentials.password } });
  expect(login.ok()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  let active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${role} assignment`).toBeTruthy();
  if (active.assignmentId !== assignment.id) {
    const selected = await context.request.post(endpoint, { data: { assignmentId: assignment.id, revision: active.revision } });
    expect(selected.ok()).toBe(true);
    active = (await (await context.request.get(endpoint)).json()).data;
  }
  expect(active.assignmentId).toBe(assignment.id);
  await page.goto(path);
  if (path.includes("/finance/setup")) {
    await expect(page.getByRole("heading", { name: "Finance setup" })).toBeVisible();
  } else {
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  }
}

test("isolated demo imports and independently posts opening balances", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(90_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as { fixture: Fixture };
  const fixture = state.fixture;
  const query = `?chart=${encodeURIComponent(fixture.browserOpeningChartId)}`;
  const preparer = await browser.newContext(testInfo.project.use);
  const approver = await browser.newContext(testInfo.project.use);
  try {
    const preparePage = await preparer.newPage();
    await signInAs(preparer, preparePage, credentials, "finance_preparer", `/holdings/simulation/finance/setup${query}`);
    await preparePage.getByRole("button", { name: "Import opening balances" }).click();
    await preparePage.getByLabel("Opening period").selectOption(fixture.periodId);
    await preparePage.getByLabel("Opening balance date").fill("2026-09-01");
    await preparePage.getByLabel("Account 1").selectOption(fixture.browserOpeningCashId);
    await preparePage.getByLabel("Debit 1").fill("100");
    await preparePage.getByLabel("Credit 1").fill("0");
    await preparePage.getByLabel("Account 2").selectOption(fixture.browserOpeningEquityId);
    await preparePage.getByLabel("Debit 2").fill("0");
    await preparePage.getByLabel("Credit 2").fill("100");
    await preparePage.getByRole("button", { name: "Preview reconciliation" }).click();
    await expect(preparePage.getByRole("heading", { name: "Opening reconciliation" })).toBeVisible();
    await expect(preparePage.getByText("THB 100.00", { exact: false }).first()).toBeVisible();
    await Promise.all([
      preparePage.waitForURL(/\/finance\/journals\//),
      preparePage.getByRole("button", { name: "Create reconciled opening journal" }).click(),
    ]);
    const journalPath = new URL(preparePage.url()).pathname;
    await expect(preparePage.getByText("draft", { exact: true }).first()).toBeVisible();

    const journalId = journalPath.split("/").at(-1)!;
    const approvePage = await approver.newPage();
    await signInAs(approver, approvePage, credentials, "finance_approver", journalPath);
    await expect(approvePage.getByRole("button", { name: "Approve and post journal" })).toBeVisible();
    await approvePage.getByRole("button", { name: "Approve and post journal" }).click();
    await expect(approvePage.getByText("posted", { exact: true }).first()).toBeVisible();
    const journalResponse = await approver.request.get("/api/v1/holdings/simulation/finance/journals?status=posted");
    expect(journalResponse.ok()).toBe(true);
    const postedJournals = (await journalResponse.json()).data as Array<{ id: string; status: string; source_type: string }>;
    expect(postedJournals.find(journal => journal.id === journalId)).toMatchObject({ status: "posted", source_type: "opening_import" });
  } finally {
    await Promise.all([preparer.close(), approver.close()]);
  }
});
