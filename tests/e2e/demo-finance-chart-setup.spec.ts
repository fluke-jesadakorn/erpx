import {createEngineeringBrowserContext} from './helpers/engineering-browser-evidence';
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
  await page.goto(`${path}${path.includes("?") ? "&" : "?"}refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
}

test("isolated demo renders real chart and account setup controls", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  expect(credentials.users.find((user) => user.role === "finance_approver"), "finance reviewer demo account").toBeTruthy();
  const context = await createEngineeringBrowserContext(browser,testInfo);
  try {
    const page = await context.newPage();
    await signInFinance(context, page, credentials, "/holdings/simulation/finance/setup");
    const chartsResponse = await page.request.get("/api/v1/holdings/simulation/finance/charts");
    expect(chartsResponse.ok(), await chartsResponse.text()).toBe(true);
    const chartsBody = await chartsResponse.json() as { data: Array<{ id: string }> };
    expect(chartsBody.data.length).toBeGreaterThan(0);
    const chartId = chartsBody.data[0].id;
    const accountsResponse = await page.request.get(`/api/v1/holdings/simulation/finance/charts/${encodeURIComponent(chartId)}/accounts`);
    expect(accountsResponse.ok(), await accountsResponse.text()).toBe(true);
    const accountsBody = await accountsResponse.json() as { data: Array<{ code: string }> };
    expect(accountsBody.data.length).toBeGreaterThan(0);

    await page.goto(`/holdings/simulation/finance/setup?step=chart&chart=${encodeURIComponent(chartId)}&refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
    await expect(page.getByRole("heading", { name: "Charts of accounts", exact: true })).toBeVisible();
    const content = page.locator('#workspace-content');
    await expect(content.getByRole("heading", { name: /^Accounts · / })).toBeVisible();
    await content.getByText("Create a chart", { exact: true }).click();
    const createChartDialog = page.getByRole("dialog", { name: "Create a chart", exact: true });
    await expect(createChartDialog).toBeVisible();
    await expect(createChartDialog.getByRole("button", { name: "Create chart", exact: true })).toBeVisible();
    await expect(createChartDialog.getByRole("button", { name: "Create chart", exact: true })).toBeVisible();
    await createChartDialog.getByRole("button", { name: "Close dialog", exact: true }).click();
    await expect(createChartDialog).toBeHidden();
    await content.getByText("Add an account", { exact: true }).click();
    const addAccountDialog = page.getByRole("dialog", { name: "Add an account", exact: true });
    await expect(addAccountDialog).toBeVisible();
    await expect(addAccountDialog.getByRole("button", { name: "Add account", exact: true })).toBeVisible();
    await addAccountDialog.getByRole("button", { name: "Close dialog", exact: true }).click();
    await expect(addAccountDialog).toBeHidden();
    await content.getByText("Import account list", { exact: true }).click();
    const importDialog = page.getByRole("dialog", { name: "Import account list", exact: true });
    await expect(importDialog).toBeVisible();
    await expect(importDialog.getByLabel("CSV account list")).toHaveValue(/code,name,category,currency,controlKind/);
    await expect(importDialog.getByText("Preview: 2 valid row(s) ready to import.", { exact: true })).toBeVisible();
    await expect(importDialog.getByRole("button", { name: "Import validated accounts", exact: true })).toBeVisible();
    await importDialog.getByRole("button", { name: "Close dialog", exact: true }).click();
    await expect(importDialog).toBeHidden();
    await expect(page.getByRole("table")).toBeVisible();

    await content.getByText("Add an account", { exact: true }).click();
    await expect(addAccountDialog).toBeVisible();
    const addAccountResponse = page.waitForResponse(response => response.request().method() === "POST" && response.url().includes(`/finance/charts/${encodeURIComponent(chartId)}/accounts`));
    const addAccountForm = addAccountDialog.locator('form').filter({ has: page.getByRole("button", { name: "Add account", exact: true }) });
    await addAccountForm.getByLabel("Code", { exact: true }).fill("9901");
    await addAccountForm.getByRole("textbox", { name: "Name", exact: true }).fill("Demo browser account");
    await addAccountForm.getByRole("combobox", { name: "Control role", exact: true }).selectOption("bank");
    await addAccountForm.getByRole("button", { name: "Add account", exact: true }).click();
    expect((await addAccountResponse).status()).toBe(201);
    await expect(addAccountForm.getByRole("button", { name: "Add account", exact: true })).toBeEnabled();
    await addAccountDialog.getByRole("button", { name: "Close dialog", exact: true }).click();
    await expect(addAccountDialog).toBeHidden();
    await expect(page.locator('#workspace-content').getByText("9901", { exact: true })).toBeVisible();
    const accountRow = content.getByRole("row").filter({ has: page.getByRole("cell", { name: "9901", exact: true }) });
    await accountRow.getByText("Change status", { exact: true }).click();
    const deactivateResponse = page.waitForResponse(response => response.request().method() === "POST" && response.url().includes("/accounts/") && response.url().endsWith("/status"));
    await accountRow.getByLabel("Reason for 9901").fill("Retire the unused demo account before close.");
    await accountRow.getByRole("button", { name: "Deactivate", exact: true }).click();
    expect((await deactivateResponse).status()).toBe(200);
    await expect(accountRow.getByText("inactive", { exact: true })).toBeVisible();
    if ((await accountRow.locator('details').getAttribute('open')) === null) {
      await accountRow.getByText("Change status", { exact: true }).click();
    }
    const activateResponse = page.waitForResponse(response => response.request().method() === "POST" && response.url().includes("/accounts/") && response.url().endsWith("/status"));
    await accountRow.getByLabel("Reason for 9901").fill("Restore the account for the next demo period.");
    await accountRow.getByRole("button", { name: "Activate", exact: true }).click();
    expect((await activateResponse).status()).toBe(200);
    await expect(accountRow.getByText("active", { exact: true })).toBeVisible();

    await content.getByText("Import account list", { exact: true }).click();
    await expect(importDialog).toBeVisible();
    const importResponse = page.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith("/finance/charts/import"));
    await importDialog.getByLabel("CSV account list").fill('code,name,category,currency,controlKind\n9902,"Demo, imported account",expense,USD,none');
    await expect(importDialog.getByText("Preview: 1 valid row(s) ready to import.", { exact: true })).toBeVisible();
    await importDialog.getByRole("button", { name: "Import validated accounts", exact: true }).click();
    expect((await importResponse).status()).toBe(201);
    await expect(importDialog.getByRole("button", { name: "Import validated accounts", exact: true })).toBeEnabled();
    await importDialog.getByRole("button", { name: "Close dialog", exact: true }).click();
    await expect(importDialog).toBeHidden();
    await expect(page.locator('#workspace-content').getByText("9902", { exact: true })).toBeVisible();
    await expect(page.locator('#workspace-content').getByText("Demo, imported account", { exact: true })).toBeVisible();

    // The new bank control has no transactions or bank statements. Retire this
    // unused account through its native command after proving reactivation so
    // subsequent period-close journeys require only the opening bank sources.
    if ((await accountRow.locator('details').getAttribute('open')) === null) {
      await accountRow.getByText("Change status", { exact: true }).click();
    }
    await accountRow.getByLabel("Reason for 9901").fill("Retire this unused zero-activity demonstration bank account after verifying its status lifecycle.");
    const retireResponse = page.waitForResponse(response => response.request().method() === "POST" && response.url().includes("/accounts/") && response.url().endsWith("/status"));
    await accountRow.getByRole("button", { name: "Deactivate", exact: true }).click();
    expect((await retireResponse).status()).toBe(200);
    await page.reload({ waitUntil: "domcontentloaded" });
    await expect(accountRow.getByText("inactive", { exact: true })).toBeVisible();
    const persisted = await page.request.get(`/api/v1/holdings/simulation/finance/charts/${encodeURIComponent(chartId)}/accounts`);
    expect(persisted.ok()).toBe(true);
    const persistedAccounts = (await persisted.json()).data as Array<{code:string;status:string;version:number;name:string}>;
    expect(persistedAccounts.find(account => account.code === "9901")).toMatchObject({status:"inactive",version:4});
    expect(persistedAccounts.find(account => account.code === "9902")).toMatchObject({name:"Demo, imported account",status:"active"});
  } finally {
    await context.close();
  }
});
