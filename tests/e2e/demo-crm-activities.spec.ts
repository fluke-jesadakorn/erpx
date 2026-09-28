import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; roles?: string[] };
type Credentials = { password: string; users: DemoUser[] };
type DemoState = { fixture?: { crmLeadId?: string } };

async function signInRole(context: BrowserContext, page: Page, credentials: Credentials, role: string, path: string) {
  const user = credentials.users.find((item) => item.roles?.includes(role) || item.role === role);
  expect(user, `${role} demo account`).toBeTruthy();
  const headers = { Origin: (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102") };
  const login = await context.request.post("/api/auth/sign-in/email", { headers, data: { email: user!.email, password: credentials.password } });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${role} active assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, { headers, data: { assignmentId: assignment.id, revision: active.revision } });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto(path, { waitUntil: "commit" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
}

test("isolated demo records CRM lead activities through Sales ownership and Work", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(120_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const leadId = state.fixture?.crmLeadId;
  expect(leadId).toBeTruthy();
  const operatorContext = await browser.newContext(testInfo.project.use);
  const managerContext = await browser.newContext(testInfo.project.use);
  const operatorPage = await operatorContext.newPage();
  const managerPage = await managerContext.newPage();
  const path = `/holdings/simulation/crm/${leadId}`;
  try {
    await signInRole(operatorContext, operatorPage, credentials, "sales_operator", path);
    await expect(operatorPage.getByRole("heading", { name: "Lead activities", exact: true })).toBeVisible();
    await operatorPage.getByLabel("Subject", { exact: true }).fill("Confirm installation window");
    await operatorPage.getByLabel("Next action details", { exact: true }).fill("Email the customer with the approved installation window.");
    await operatorPage.getByRole("combobox", { name: "Responsible salesperson", exact: true }).selectOption({ label: "Support agent" });
    await operatorPage.getByLabel("Due date", { exact: true }).fill("2026-09-26");
    await Promise.all([
      operatorPage.waitForResponse((response) => response.url().includes("/api/v1/holdings/simulation/crm/activities") && response.request().method() === "POST"),
      operatorPage.getByRole("button", { name: "Schedule activity", exact: true }).click(),
    ]);
    const activity = operatorPage.locator("article.card").filter({ hasText: "Confirm installation window" });
    await expect(activity).toBeVisible();
    await expect(activity.getByText(/open · Owner/)).toBeVisible();
    await signInRole(managerContext, managerPage, credentials, "sales_manager", path);
    const managerActivity = managerPage.locator("article.card").filter({ hasText: "Confirm installation window" });
    await expect(managerActivity).toBeVisible();
    await expect(managerActivity.getByText(/open · Owner/)).toBeVisible();
    await operatorPage.getByText("Record activity decision", { exact: true }).click();
    await activity.getByRole("combobox", { name: "Action", exact: true }).selectOption("complete");
    await activity.getByRole("textbox", { name: "Outcome and reason", exact: true }).fill("Customer confirmed the installation window.");
    await Promise.all([
      operatorPage.waitForResponse((response) => response.url().includes("/api/v1/holdings/simulation/crm/activities") && response.request().method() === "POST"),
      activity.getByRole("button", { name: "Save activity decision", exact: true }).click(),
    ]);
    await expect(activity.getByText(/completed · Owner/)).toBeVisible();
    await testInfo.attach("crm-activity-result", { contentType: "application/json", body: JSON.stringify({ run, leadId, roles: ["sales_operator", "sales_manager"], created: true, reloadedForManager: true, completed: true }, null, 2) });
  } finally {
    await Promise.allSettled([operatorContext.close(), managerContext.close()]);
  }
});
