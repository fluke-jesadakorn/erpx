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
  const context = await browser.newContext(testInfo.project.use);
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

    await page.goto(`/holdings/simulation/finance/setup?chart=${encodeURIComponent(chartId)}&refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
    await expect(page.getByRole("heading", { name: "Charts of accounts", exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { name: /Add account to/ })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Import account list", exact: true })).toBeVisible();
    await expect(page.getByLabel("CSV account list")).toHaveValue(/code,name,category,currency,controlKind/);
    await expect(page.getByText("Preview: 2 valid row(s) ready to import.", { exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: "Create chart", exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: "Add account", exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: "Import validated accounts", exact: true })).toBeVisible();
    await expect(page.getByRole("table")).toBeVisible();

    const addAccountResponse = page.waitForResponse(response => response.request().method() === "POST" && response.url().includes(`/finance/charts/${encodeURIComponent(chartId)}/accounts`));
    await page.locator('input[name="code"]').fill("9901");
    await page.getByRole("textbox", { name: "Name", exact: true }).fill("Demo browser account");
    await page.locator('select[name="controlKind"]').selectOption("bank");
    await page.getByRole("button", { name: "Add account", exact: true }).click();
    expect((await addAccountResponse).status()).toBe(201);
    await expect(page.getByText("9901", { exact: true })).toBeVisible();
    const accountRow = page.getByRole("row").filter({ hasText: "9901" });
    const deactivateResponse = page.waitForResponse(response => response.request().method() === "POST" && response.url().includes("/accounts/") && response.url().endsWith("/status"));
    await accountRow.getByLabel("Reason for 9901").fill("Retire the unused demo account before close.");
    await accountRow.getByRole("button", { name: "Deactivate", exact: true }).click();
    expect((await deactivateResponse).status()).toBe(200);
    await expect(accountRow.getByText("inactive", { exact: true })).toBeVisible();
    const activateResponse = page.waitForResponse(response => response.request().method() === "POST" && response.url().includes("/accounts/") && response.url().endsWith("/status"));
    await accountRow.getByLabel("Reason for 9901").fill("Restore the account for the next demo period.");
    await accountRow.getByRole("button", { name: "Activate", exact: true }).click();
    expect((await activateResponse).status()).toBe(200);
    await expect(accountRow.getByText("active", { exact: true })).toBeVisible();

    const importResponse = page.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith("/finance/charts/import"));
    await page.getByLabel("CSV account list").fill('code,name,category,currency,controlKind\n9902,"Demo, imported account",expense,USD,none');
    await expect(page.getByText("Preview: 1 valid row(s) ready to import.", { exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Import validated accounts", exact: true }).click();
    expect((await importResponse).status()).toBe(201);
    await expect(page.getByText("9902", { exact: true })).toBeVisible();
    await expect(page.getByText("Demo, imported account", { exact: true })).toBeVisible();
  } finally {
    await context.close();
  }
});
