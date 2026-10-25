import { captureInvestorCheckpoint } from "../helpers/investor-capture";
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pinnedBunBinary } from "../../scripts/testing/runtime-tooling";
import { frontWorkspacePath, signInFrontRole, type SimulatedFrontCredentials } from "../helpers/front-simulation-session";

type DemoUser = { email: string; role: string; userId: string };
type Credentials = { password: string; users: DemoUser[] };
type DemoState = { fixture: { hospitality: { companyId: string; propertyId: string; purchaseRequestId: string } } };

async function openRequestAi(page: Page) {
  const disclosure = page.locator("#workspace-content details").filter({ has: page.locator("summary").getByText("Ask AI about this request", { exact: true }) });
  await disclosure.locator(":scope > summary").click();
  await disclosure.locator("summary").getByText("AI preparation", { exact: true }).click();
}

async function signIn(context: BrowserContext, page: Page, credentials: Credentials, role: string) {
  const user = credentials.users.find((item) => item.role === role && item.email.endsWith("@riverside-lantern.example"));
  expect(user, `${role} Riverside account`).toBeTruthy();
  const origin = process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100";
  const login = await context.request.post("/api/auth/sign-in/email", { headers: { Origin: origin }, data: { email: user!.email, password: credentials.password } });
  expect(login.ok(), `${role} login`).toBe(true);
  const endpoint = "/api/v1/holdings/riverside-lantern/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${role} active assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, { headers: { Origin: origin }, data: { assignmentId: assignment.id, revision: active.revision } });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto("/holdings/riverside-lantern/home", { waitUntil: "domcontentloaded" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
}

test("H002 AI drafts from authorized versions, cites hotel evidence, then falls back manually", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  expect(run, "The Thai Hotel mock runner must supply an exact prepared simulation run.").toMatch(/^[a-f0-9]{12}$/);
  if (!run) throw new Error("EXACT_BROWSER_SIMULATION_RUN_REQUIRED");
  test.setTimeout(240_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const { companyId, propertyId, purchaseRequestId } = state.fixture.hospitality;
  const employee = await browser.newContext(testInfo.project.use);
  const assistant = await browser.newContext(testInfo.project.use);
  const assistantApi = await browser.newContext(testInfo.project.use);
  const inspector = await browser.newContext(testInfo.project.use);
  const pmsAuditor = await browser.newContext(testInfo.project.use);
  const administrator = await browser.newContext(testInfo.project.use);
  [employee, assistant, assistantApi, inspector, pmsAuditor, administrator].forEach((context) => context.setDefaultTimeout(15_000));
  try {
    const employeePage = await employee.newPage();
    await signIn(employee, employeePage, credentials, "employee");
    const requestPath = `/holdings/riverside-lantern/requests/${purchaseRequestId}`;
    await employeePage.goto(requestPath, { waitUntil: "domcontentloaded" });
    await openRequestAi(employeePage);
    await expect(employeePage.getByRole("button", { name: "Save draft", exact: true })).toBeEnabled();
    await expect(employeePage.getByRole("button", { name: "Prepare with AI", exact: true })).toBeEnabled();
    const prepareResponse = employeePage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(`/requests/${purchaseRequestId}/ai`));
    await employeePage.getByRole("button", { name: "Prepare with AI", exact: true }).click();
    expect((await prepareResponse).ok()).toBe(true);
    await expect(employeePage.getByText("Source: this saved request, version 1.", { exact: true })).toBeVisible();
    await expect(employeePage.getByText("Review draft: Riverside kitchen rice restock", { exact: true })).toBeVisible();
    await employeePage.locator("summary").getByText("Customize request title (optional)", { exact: true }).click();
    const title = employeePage.getByLabel("Request title", { exact: true });
    await expect(title).toHaveValue("Riverside kitchen rice restock");
    await employeePage.getByRole("button", { name: "Use suggested title", exact: true }).click();
    await expect(title).toHaveValue("Review draft: Riverside kitchen rice restock");
    await captureInvestorCheckpoint(employeePage, "ai-draft", employeePage.getByText("Source: this saved request, version 1.", { exact: true }));
    const saveRequest = employeePage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(`/requests/${purchaseRequestId}`));
    await employeePage.getByRole("button", { name: "Save draft", exact: true }).click();
    expect((await saveRequest).ok()).toBe(true);
    await employeePage.reload({ waitUntil: "domcontentloaded" });
    await expect(employeePage.getByRole("heading", { name: "Review draft: Riverside kitchen rice restock", exact: true })).toBeVisible();
    await openRequestAi(employeePage);
    await expect(employeePage.getByRole("button", { name: "Prepare with AI", exact: true })).toBeVisible();
    await expect(employeePage.getByRole("button", { name: "Use suggested title", exact: true })).toHaveCount(0);
    const request = (await (await employee.request.get(`/api/v1/holdings/riverside-lantern/requests/${purchaseRequestId}`)).json()).data.request;
    expect(request).toMatchObject({ title: "Review draft: Riverside kitchen rice restock", version: 2, state: "draft" });
    const status = (await (await employee.request.get(`/api/v1/holdings/riverside-lantern/requests/${purchaseRequestId}/ai`)).json()).data;
    expect(status).toMatchObject({ state: "stale", result: null, sourceVersion: 2 });

    const assistantPage = await assistant.newPage();
    const auditPath = frontWorkspacePath(companyId, `night-audit?property=${propertyId}`);
    await signIn(assistant, assistantPage, credentials, "pms_night_auditor");
    await expect(assistantPage.getByRole("heading", { name: "Today", level: 1, exact: true })).toBeVisible();
    await assistantPage.getByRole("button", { name: "Open work assistant" }).click();
    const panel = assistantPage.getByRole("complementary", { name: "ERPX work assistant" });
    await expect(panel.getByText("This page · Today", { exact: true })).toBeVisible();
    await panel.getByLabel("Message to work assistant").fill("Summarize this workspace and suggest one safe next step.");
    const panelResponse = assistantPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/api/v1/holdings/riverside-lantern/assistant"));
    await panel.getByRole("button", { name: "Send to assistant" }).click();
    expect((await panelResponse).ok()).toBe(true);
    await expect(panel.getByText(/Simulation-only summary/)).toBeVisible();
    await captureInvestorCheckpoint(assistantPage, "ai-assistant", panel);

    const pmsPage = await pmsAuditor.newPage();
    await signInFrontRole(pmsAuditor, pmsPage, credentials as SimulatedFrontCredentials, "holding_owner", "pms", companyId, auditPath);
    const manualDayClose = pmsPage.locator("details").filter({ hasText: "Start day close" }).first();
    await manualDayClose.locator("summary").click();
    await expect(manualDayClose.locator("form")).toBeVisible();
    await pmsPage.reload({ waitUntil: "domcontentloaded" });
    await expect(pmsPage.locator("details").filter({ hasText: "Start day close" }).first().locator("summary")).toBeVisible();
    const assistantApiPage = await assistantApi.newPage();
    await signIn(assistantApi, assistantApiPage, credentials, "pms_night_auditor");
    const workContext = await assistantApiPage.locator("[data-work-revision]").evaluate((node) => {
      const element = node as HTMLElement;
      return {
        revision: element.dataset.workRevision ?? "",
        company: element.dataset.workCompany ?? "",
        assignment: element.dataset.workAssignment ?? "",
      };
    });
    expect(workContext.revision).not.toBe("");
    expect(workContext.company).not.toBe("");
    expect(workContext.assignment).not.toBe("");
    const auditAssistantUrl = `/api/v1/holdings/riverside-lantern/assistant?${new URLSearchParams({ path: `/pms/night-audit?property=${propertyId}`, mode: "page" })}`;
    const assistantResponse = await assistantApi.request.post("/api/v1/holdings/riverside-lantern/assistant", {
      headers: {
        Origin: process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100",
        "Idempotency-Key": `hotel-ai-${run}`,
        "x-erpx-context-revision": workContext.revision,
        "x-erpx-context-company": workContext.company,
        "x-erpx-context-assignment": workContext.assignment,
      },
      data: {
        path: `/pms/night-audit?property=${propertyId}`,
        mode: "page",
        question: "Summarize the current hotel day and what the team should check before close.",
        contextPaths: [],
      },
    });
    expect(assistantResponse.ok(), await assistantResponse.text()).toBe(true);
    expect((await assistantResponse.json()).data.id).toBe(`hotel-ai-${run}`);
    const firstView = (await (await assistantApi.request.get(auditAssistantUrl)).json()).data;
    expect(firstView.turns).toHaveLength(1);
    expect(firstView.turns[0].answer.answer).toContain("Simulation-only summary");
    const hotelCitation = firstView.turns[0].sources.find((source: { title: string }) => source.title.includes("hotel evidence"));
    expect(hotelCitation).toBeTruthy();
    expect(firstView.turns[0].answer.sourceIds).toContain(hotelCitation.id);

    // A new authorized housekeeping record changes the source snapshot and hides the earlier summary.
    const inspectorPage = await inspector.newPage();
    const roomsPath = frontWorkspacePath(companyId, `housekeeping?property=${propertyId}`);
    await signInFrontRole(inspector, inspectorPage, credentials as SimulatedFrontCredentials, "pms_supervisor", "pms", companyId, roomsPath);
    await inspectorPage.locator("summary").filter({ hasText: /^Create housekeeping task$/ }).click();
    const taskSummary = inspectorPage.locator("summary").filter({ hasText: /^Create housekeeping task · Room 204$/ });
    await expect(taskSummary).toHaveCount(1);
    await taskSummary.click();
    const taskDetails = taskSummary.locator("..");
    await expect(taskDetails.getByRole("button", { name: "Create housekeeping task · Room 204", exact: true })).toHaveAttribute("data-client-managed-ready", "true");
    await taskDetails.locator('select[name="type"]').selectOption("stayover");
    const businessDate = (await (await inspector.request.get(`/api/v1/holdings/riverside-lantern/pms?property=${propertyId}`)).json()).data.snapshot.property.businessDate;
    await taskDetails.locator('input[name="date"]').fill(businessDate);
    const taskResponse = inspectorPage.waitForResponse((response) => response.request().method() === "POST" && response.url().includes(`/api/v1/front/riverside-lantern/${companyId}/pms`));
    await taskDetails.getByRole("button", { name: /Create housekeeping task/ }).click();
    expect((await taskResponse).ok(), await (await taskResponse).text()).toBe(true);
    const changedView = (await (await assistantApi.request.get(auditAssistantUrl)).json()).data;
    expect(changedView.hiddenTurns).toBe(1);
    expect(changedView.turns).toHaveLength(0);

    const adminPage = await administrator.newPage();
    await signIn(administrator, adminPage, credentials, "holding_owner");
    await adminPage.goto("/holdings/riverside-lantern/settings?section=ai", { waitUntil: "domcontentloaded" });
    await expect(adminPage.getByRole("heading", { name: "Company defaults", exact: true })).toBeVisible();
    const aiDefaults = adminPage.locator("form").filter({ has: adminPage.locator('select[name="mode"]') });
    await expect(aiDefaults.getByRole("button", { name: "Save defaults", exact: true })).toHaveAttribute("data-client-managed-ready", "true");
    await aiDefaults.locator('select[name="mode"]').selectOption("disabled");
    await aiDefaults.getByRole("button", { name: "Save defaults", exact: true }).click();
    await expect(adminPage.getByText("Settings saved.", { exact: true })).toBeVisible();
    const unavailable = (await (await assistantApi.request.get(auditAssistantUrl)).json()).data;
    expect(unavailable.unavailable).toContain("AI is disabled");
    const failedAssistant = await assistantApi.request.post("/api/v1/holdings/riverside-lantern/assistant", {
      headers: {
        Origin: process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100",
        "Idempotency-Key": `ai-disabled-${run}`,
        "x-erpx-context-revision": workContext.revision,
        "x-erpx-context-company": workContext.company,
        "x-erpx-context-assignment": workContext.assignment,
      },
      data: { path: `/pms/night-audit?property=${propertyId}`, mode: "page", question: "Prepare a hotel day-close summary from this source." },
    });
    expect(failedAssistant.status()).toBe(503);
    expect((await failedAssistant.json()).error.code).toBe("AI_DISABLED");

    // AI is unavailable, so the night auditor completes the same day close by hand.
    await pmsPage.reload({ waitUntil: "domcontentloaded" });
    const manualFallbackClose = pmsPage.locator("details").filter({ hasText: "Start day close" }).first();
    await manualFallbackClose.locator("summary").click();
    await expect(manualFallbackClose.getByRole("button", { name: "Start day close", exact: true })).toHaveAttribute("data-client-managed-ready", "true");
    const pmsCommand = `/api/v1/front/riverside-lantern/${companyId}/pms`;
    const startClose = pmsPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(pmsCommand));
    await manualFallbackClose.getByRole("button", { name: "Start day close", exact: true }).click();
    const closeResult = await startClose;
    expect(closeResult.ok(), await closeResult.text()).toBe(true);
    const beforeDrain = (await (await pmsAuditor.request.get(`/api/v1/holdings/riverside-lantern/pms?property=${propertyId}`)).json()).data;
    expect(beforeDrain.snapshot.property.state).toBe("closing");
    const completedNightsBefore = beforeDrain.snapshot.records.filter((record: { kind: string; data: { state?: string } }) => record.kind === "night" && record.data.state === "completed").length;
    const completed = JSON.parse(execFileSync(pinnedBunBinary(resolve(".")), ["--env-file=/dev/null", "scripts/local/run-server-script.ts", "scripts/fixtures/hospitality-drain-demo-job.ts", run], { cwd: resolve("."), timeout: 30_000 }).toString()) as { job: { state: string; error_code: string | null }; property: { state: string; business_date: string } };
    expect(completed).toMatchObject({ job: { state: "completed", error_code: null }, property: { state: "open" } });
    expect(completed.property.business_date).not.toBe(beforeDrain.snapshot.property.businessDate);
    const completedAudit = (await (await pmsAuditor.request.get(`/api/v1/holdings/riverside-lantern/pms?property=${propertyId}`)).json()).data;
    const completedNightsAfter = completedAudit.snapshot.records.filter((record: { kind: string; data: { state?: string } }) => record.kind === "night" && record.data.state === "completed").length;
    expect(completedNightsAfter).toBe(completedNightsBefore + 1);
    const replayedDrain = JSON.parse(execFileSync(pinnedBunBinary(resolve(".")), ["--env-file=/dev/null", "scripts/local/run-server-script.ts", "scripts/fixtures/hospitality-drain-demo-job.ts", run], { cwd: resolve("."), timeout: 30_000 }).toString()) as typeof completed;
    expect(replayedDrain).toEqual(completed);
    const replayedAudit = (await (await pmsAuditor.request.get(`/api/v1/holdings/riverside-lantern/pms?property=${propertyId}`)).json()).data;
    expect(replayedAudit.snapshot.records.filter((record: { kind: string; data: { state?: string } }) => record.kind === "night" && record.data.state === "completed")).toHaveLength(completedNightsAfter);

    await employeePage.reload({ waitUntil: "domcontentloaded" });
    await openRequestAi(employeePage);
    await expect(employeePage.getByText("AI is disabled for this capability. Continue manually.", { exact: true })).toBeVisible();
    const description = employeePage.locator("#workspace-content textarea").first();
    await expect(employeePage.getByRole("button", { name: "Save draft", exact: true })).toBeEnabled();
    await captureInvestorCheckpoint(employeePage, "ai-manual", employeePage.getByText("AI is disabled for this capability. Continue manually.", { exact: true }));
    await description.fill("Manual synthetic fallback: the kitchen team can continue from the saved rice requirement while AI is unavailable.");
    const manualSave = employeePage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(`/requests/${purchaseRequestId}`));
    await employeePage.getByRole("button", { name: "Save draft", exact: true }).click();
    expect((await manualSave).ok()).toBe(true);
    const manuallySaved = (await (await employee.request.get(`/api/v1/holdings/riverside-lantern/requests/${purchaseRequestId}`)).json()).data.request;
    expect(manuallySaved.description).toContain("Manual synthetic fallback");
  } finally {
    await Promise.all([employee.close(), assistant.close(), assistantApi.close(), inspector.close(), pmsAuditor.close(), administrator.close()]);
  }
});
