import { captureInvestorCheckpoint } from "../../support/helpers/investor-capture";
import {createEngineeringBrowserContext} from "../../support/browser/engineering-browser-evidence";
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; roles?: string[]; label: string };
type Credentials = { password: string; users: DemoUser[] };

async function signInFinance(context: BrowserContext, page: Page, credentials: Credentials, path: string) {
  const login = await context.request.post("/api/auth/sign-in/email", {
    headers: { Origin: (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102") },
    data: { email: "finance@simulation.test", password: credentials.password },
  });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === "finance_approver");
  expect(assignment, "finance approver assignment").toBeTruthy();
  const selected = await context.request.post(endpoint, {
    headers: { Origin: (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102") },
    data: { assignmentId: assignment.id, revision: active.revision },
  });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto(`${path}?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
}

test("isolated demo traces finance report totals to the selected ledger", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  expect(credentials.users.find((user) => user.role === "finance_approver"), "finance reviewer demo account").toBeTruthy();
  const context = await createEngineeringBrowserContext(browser,testInfo);
  try {
    const page = await context.newPage();
    await signInFinance(context, page, credentials, "/holdings/simulation/finance/reports");
    const chartsResponse = await page.request.get("/api/v1/holdings/simulation/finance/charts");
    expect(chartsResponse.ok(), await chartsResponse.text()).toBe(true);
    const chartsBody = await chartsResponse.json() as { data: Array<{ id: string; base_currency: string }> };
    const chart = chartsBody.data[0];
    expect(chart).toBeTruthy();

    const statementsResponse = await page.request.get(`/api/v1/holdings/simulation/finance/statements?chartId=${encodeURIComponent(chart.id)}`);
    const trialBalanceResponse = await page.request.get(`/api/v1/holdings/simulation/finance/trial-balance?chartId=${encodeURIComponent(chart.id)}`);
    expect(statementsResponse.ok(), await statementsResponse.text()).toBe(true);
    expect(trialBalanceResponse.ok(), await trialBalanceResponse.text()).toBe(true);
    const statementsBody = await statementsResponse.json() as { data: { balanceSheet: { assets: string }; trialBalance: Array<{ id: string }> } };
    const trialBalanceBody = await trialBalanceResponse.json() as { data: Array<{ id: string; category: string; debit: string; credit: string }> };
    expect(statementsBody.data.trialBalance.length).toBe(trialBalanceBody.data.length);

    await page.goto(`/holdings/simulation/finance/reports?chart=${encodeURIComponent(chart.id)}&refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
    await expect(page.getByRole("heading", { name: "Reports and statements", exact: true })).toBeVisible();
    await expect(page.locator('#workspace-content').getByText(`${chart.base_currency} ${statementsBody.data.balanceSheet.assets}`, { exact: true }).first()).toBeVisible();
    const traceLinks = page.getByRole("link", { name: "Trace journal entries →", exact: true });
    await expect(traceLinks).toHaveCount(5);
    await captureInvestorCheckpoint(page, "finance-statements", testInfo.project.name === "mobile" ? traceLinks.first() : page.getByRole("heading", { name: "Reports and statements", exact: true }));
    const assetTrace = traceLinks.first();
    const assetHref = new URL((await assetTrace.getAttribute('href'))!, page.url());
    expect(assetHref.pathname).toBe('/holdings/simulation/record-details/finance_report/assets');
    expect(assetHref.searchParams.get('chartId')).toBe(chart.id);
    await assetTrace.click();
    await expect(page).toHaveURL(url => url.searchParams.get('detailKind') === 'finance_report' && url.searchParams.get('detailId') === 'assets');
    const drawer = page.getByTestId('record-detail-drawer');
    await expect(drawer.getByRole('heading', { name: 'assets', level: 1, exact: true })).toBeVisible();
    const assetAccount = trialBalanceBody.data.find(row => row.category === 'asset' && (Number(row.debit) > 0 || Number(row.credit) > 0));
    expect(assetAccount, 'a posted asset account in the selected ledger').toBeTruthy();
    const assetSources = drawer.getByRole('region', { name: 'Source records', exact: true });
    await expect(assetSources).toHaveCount(1);
    const assetRow = assetSources.getByRole('row').filter({ has: page.locator(`a[href^="/holdings/simulation/record-details/trial_balance/${encodeURIComponent(assetAccount!.id)}?"]`) });
    await expect(assetRow).toHaveCount(1);
    const accountSource = assetRow.getByRole('cell').nth(0).getByRole('link');
    await expect(accountSource).toHaveCount(1);
    expect(new URL((await accountSource.getAttribute('href'))!, page.url()).searchParams.get('chartId')).toBe(chart.id);
    await accountSource.click();
    await expect(page).toHaveURL(url => url.searchParams.get('detailKind') === 'trial_balance' && url.searchParams.get('detailId') === assetAccount!.id);
    await expect(drawer.getByRole('heading', { name: 'Source records', exact: true })).toBeVisible();
    await expect(drawer.locator('a[href*="/finance/journals/"]').first()).toBeVisible();
    await captureInvestorCheckpoint(page, "finance-sources", drawer);
    await drawer.getByRole('button', { name: 'Close dialog', exact: true }).click();
    await expect(drawer).toBeHidden();
    await expect(page.getByRole("heading", { name: "Trial balance", exact: true })).toBeVisible();
    await expect(page.getByRole("link", { name: "Open Accounting →", exact: true })).toHaveAttribute("href", `/holdings/simulation/finance/accounting?chart=${encodeURIComponent(chart.id)}#journal-entries`);
    await page.getByRole('link', { name: 'Open Accounting →', exact: true }).click();
    await expect(page).toHaveURL(url => url.pathname === '/holdings/simulation/finance/accounting' && url.searchParams.get('chart') === chart.id && url.hash === '#journal-entries');
    await expect(page.locator('#workspace-content').getByRole('heading', { name: 'Journal entries', exact: true })).toBeVisible();
  } finally {
    await context.close();
  }
});
