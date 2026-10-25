import {createEngineeringBrowserContext} from "../../support/browser/engineering-browser-evidence";
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
  await expect(page.getByRole("heading", { name: "Lead activities", exact: true })).toBeVisible();
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
  const operatorContext = await createEngineeringBrowserContext(browser,testInfo);
  const managerContext = await createEngineeringBrowserContext(browser,testInfo);
  const operatorPage = await operatorContext.newPage();
  const managerPage = await managerContext.newPage();
  const path = `/holdings/simulation/customers-sales/sales/pipeline/${leadId}`;
  try {
    await signInRole(operatorContext, operatorPage, credentials, "sales_operator", path);
    const activityArea = operatorPage.locator("#crm-activities");
    await expect(activityArea.getByRole("heading", { name: "Lead activities", exact: true })).toBeVisible();
    await activityArea.getByRole("button", { name: "Schedule lead activity", exact: true }).click();
    const scheduleEditor = activityArea.locator("[data-record-editor]").filter({ hasText: "Schedule lead activity" });
    const scheduleForm = scheduleEditor.locator("form");
    await expect(scheduleEditor).toBeVisible();
    await scheduleForm.getByLabel("Subject", { exact: true }).fill("Confirm installation window");
    await scheduleForm.getByLabel("Next action details", { exact: true }).fill("Email the customer with the approved installation window.");
    await scheduleForm.getByRole("combobox", { name: "Responsible salesperson", exact: true }).selectOption({ label: "Support manager" });
    await scheduleForm.getByLabel("Due date", { exact: true }).fill("2026-09-26");
    await Promise.all([
      operatorPage.waitForResponse((response) => response.url().includes("/api/v1/holdings/simulation/crm/activities") && response.request().method() === "POST"),
      scheduleForm.getByRole("button", { name: "Schedule activity", exact: true }).click(),
    ]);
    const activity = activityArea.locator("article[data-slot=card]:visible").filter({ hasText: "Confirm installation window" });
    await expect(activity).toBeVisible();
    await expect(activity.getByText(/open · Owner Support manager/)).toBeVisible();
    await signInRole(managerContext, managerPage, credentials, "sales_manager", path);
    const managerActivity = managerPage.locator("#crm-activities article[data-slot=card]:visible").filter({ hasText: "Confirm installation window" });
    await expect(managerActivity).toBeVisible();
    await expect(managerActivity.getByText(/open · Owner Support manager/)).toBeVisible();
    const actionsPath = `/holdings/simulation/work/actions?type=crm_activity&q=${encodeURIComponent("Confirm installation window")}`;
    await managerPage.goto(actionsPath, { waitUntil: "commit" });
    const workResults = managerPage.getByRole("region", { name: "Work results", exact: true });
    const actionableWork = workResults.getByRole("link", { name: /CRM activity · Confirm installation window/ });
    await expect(actionableWork).toBeVisible();
    await managerPage.goto(path, { waitUntil: "commit" });
    const managerLeadActivity = managerPage.locator("#crm-activities article[data-slot=card]:visible").filter({ hasText: "Confirm installation window" });
    await managerLeadActivity.getByRole("button", { name: "Record activity decision", exact: true }).click();
    const decisionEditor = managerLeadActivity.locator("[data-record-editor]").filter({ hasText: "Record activity decision" });
    const decisionForm = decisionEditor.locator("form");
    await expect(decisionEditor).toBeVisible();
    await expect(decisionForm).toHaveCount(1);
    await decisionForm.getByRole("combobox", { name: "Action", exact: true }).selectOption("complete");
    await decisionForm.getByRole("textbox", { name: "Outcome and reason", exact: true }).fill("Customer confirmed the installation window.");
    await Promise.all([
      managerPage.waitForResponse((response) => response.url().includes("/api/v1/holdings/simulation/crm/activities") && response.request().method() === "POST"),
      decisionForm.getByRole("button", { name: "Save activity decision", exact: true }).click(),
    ]);
    await expect(decisionEditor).not.toBeVisible();
    await managerPage.reload();
    const completedActivity = managerPage.locator("#crm-activities article[data-slot=card]:visible").filter({ hasText: "Confirm installation window" });
    await expect(completedActivity.getByText(/completed · Owner Support manager/)).toBeVisible();
    await expect(completedActivity.getByText("Outcome: Customer confirmed the installation window.", { exact: true })).toBeVisible();
    await completedActivity.getByText("Activity history", { exact: true }).click();
    await expect(completedActivity.getByText(/Version 1 · create ·/)).toBeVisible();
    await expect(completedActivity.getByText(/Version 2 · complete ·/)).toBeVisible();
    await managerPage.goto(actionsPath, { waitUntil: "commit" });
    await expect(workResults.getByText("No matching work", { exact: true })).toBeVisible();
    await managerPage.goto(`/holdings/simulation/work/completed?type=crm_activity&q=${encodeURIComponent("Confirm installation window")}`, { waitUntil: "commit" });
    await expect(managerPage.getByRole("region", { name: "Work results", exact: true }).getByRole("link", { name: /CRM activity · Confirm installation window/ })).toBeVisible();
    await testInfo.attach("crm-activity-result", { contentType: "application/json", body: JSON.stringify({ run, leadId, roles: ["sales_operator", "sales_manager"], owner: "sales_manager", created: true, appearedInOwnerActions: true, reloadedForManager: true, completed: true, disappearedFromActions: true, retainedInCompletedWork: true, retainedOutcomeAndVersions: [1, 2] }, null, 2) });
  } finally {
    await Promise.allSettled([operatorContext.close(), managerContext.close()]);
  }
});
