import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; userId: string };
type Credentials = { password: string; users: DemoUser[] };
type DemoState = { fixture: { hospitality: { payrollEngagementIds: Record<string, string>; payrollPersonIds: Record<string, string>; leaveTypeId: string; rosterWeekId: string } } };

async function signIn(context: BrowserContext, page: Page, credentials: Credentials, role: string) {
  const user = credentials.users.find((item) => item.role === role && item.email.endsWith("@riverside-lantern.example"));
  expect(user, `${role} Riverside account`).toBeTruthy();
  const origin = process.env.ERPX_E2E_BASE_URL ?? (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102");
  const login = await context.request.post("/api/auth/sign-in/email", { headers: { Origin: origin }, data: { email: user!.email, password: credentials.password } });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/riverside-lantern/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${role} active assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, { headers: { Origin: origin }, data: { assignmentId: assignment.id, revision: active.revision } });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto("/holdings/riverside-lantern/home", { waitUntil: "domcontentloaded" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
}

test("employee overtime is verified by a manager and independently handed off by HR", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  expect(run, "The Thai Hotel mock runner must supply an exact prepared simulation run.").toMatch(/^[a-f0-9]{12}$/);
  test.setTimeout(180_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  expect(state.fixture.hospitality.payrollEngagementIds.employee).toBeTruthy();
  expect(state.fixture.hospitality.leaveTypeId).toBeTruthy();
  expect(state.fixture.hospitality.rosterWeekId).toBeTruthy();
  const employeeContext = await browser.newContext(testInfo.project.use);
  const managerContext = await browser.newContext(testInfo.project.use);
  const hrContext = await browser.newContext(testInfo.project.use);
  const unauthorizedContext = await browser.newContext(testInfo.project.use);
  [employeeContext, managerContext, hrContext, unauthorizedContext].forEach((context) => context.setDefaultTimeout(15_000));
  try {
    const employeePage = await employeeContext.newPage();
    const managerPage = await managerContext.newPage();
    const hrPage = await hrContext.newPage();
    const unauthorizedPage = await unauthorizedContext.newPage();
    await signIn(employeeContext, employeePage, credentials, "employee");
    await employeePage.goto("/holdings/riverside-lantern/overtime/new", { waitUntil: "domcontentloaded" });
    await employeePage.waitForTimeout(750);
    await employeePage.getByLabel("Request path").selectOption("retrospective");
    await expect(employeePage.getByLabel("Actual start", { exact: true })).toBeVisible();
    await employeePage.getByLabel("Actual start").fill("2026-09-10T22:00");
    await employeePage.getByLabel("Actual end").fill("2026-09-11T02:00");
    await employeePage.getByLabel("Unpaid break (minutes)").fill("30");
    await employeePage.getByLabel("Reason", { exact: true }).fill("Fictional restaurant close and guest safety coverage.");
    await employeePage.getByLabel("Late-submission explanation").fill("Synthetic acceptance scenario for retrospective hours.");
    await employeePage.getByRole("button", { name: "Review calculated hours" }).click();
    await expect(employeePage.getByText("3.5 hours", { exact: true })).toBeVisible();
    await employeePage.getByRole("button", { name: "Save draft to continue" }).click();
    await expect(employeePage).toHaveURL(/\/overtime\/ot-/);
    const overtimeUrl = employeePage.url();
    const overtimeId = new URL(overtimeUrl).pathname.split("/").pop();
    expect(overtimeId).toBeTruthy();
    await employeePage.reload({ waitUntil: "domcontentloaded" });
    await expect(employeePage.getByLabel("Actual start")).toHaveValue("2026-09-10T22:00");
    await expect(employeePage.getByLabel("Actual end")).toHaveValue("2026-09-11T02:00");
    await employeePage.getByRole("button", { name: "Review calculated hours" }).click();
    await expect(employeePage.getByText("3.5 hours", { exact: true })).toBeVisible();
    const submitResponse = employeePage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(`/overtime-requests/${overtimeId}`));
    await employeePage.getByRole("button", { name: "Submit claim", exact: true }).click();
    expect((await submitResponse).status()).toBe(200);
    await expect(employeePage.getByRole("heading", { name: "Manager verification" })).toBeVisible();

    await signIn(unauthorizedContext, unauthorizedPage, credentials, "purchasing_operator");
    const hiddenRequest = await unauthorizedContext.request.get(`/api/v1/holdings/riverside-lantern/overtime-requests/${overtimeId}`);
    expect(hiddenRequest.status()).toBe(404);

    await signIn(managerContext, managerPage, credentials, "people_manager");
    await managerPage.goto(overtimeUrl, { waitUntil: "domcontentloaded" });
    await expect(managerPage.getByRole("button", { name: "Verify actual hours", exact: true })).toBeVisible();
    const verifyResponse = managerPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(`/overtime-requests/${overtimeId}`));
    await managerPage.getByRole("button", { name: "Verify actual hours", exact: true }).click();
    expect((await verifyResponse).status()).toBe(200);
    await expect(managerPage.getByRole("heading", { name: "HR approval" })).toBeVisible();
    await managerPage.reload({ waitUntil: "domcontentloaded" });
    await expect(managerPage.getByRole("heading", { name: "HR approval" })).toBeVisible();

    await signIn(hrContext, hrPage, credentials, "hr_admin");
    await hrPage.goto(overtimeUrl, { waitUntil: "domcontentloaded" });
    await expect(hrPage.getByRole("button", { name: "Approve hours & hand off", exact: true })).toBeVisible();
    const approveResponse = hrPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(`/overtime-requests/${overtimeId}`));
    await hrPage.getByRole("button", { name: "Approve hours & hand off", exact: true }).click();
    expect((await approveResponse).status()).toBe(200);
    await expect(hrPage.locator("#workspace-content").getByText("Approved hours have been handed off to payroll preview.").first()).toBeVisible();
    await hrPage.reload({ waitUntil: "domcontentloaded" });
    await expect(hrPage.locator("#workspace-content").getByText("Approved hours have been handed off to payroll preview.").first()).toBeVisible();
    const approved = await hrContext.request.get(`/api/v1/holdings/riverside-lantern/overtime-requests/${overtimeId}`);
    expect(approved.ok()).toBe(true);
    expect((await approved.json()).data).toMatchObject({ state: "approved", handoff: { minutes: 210 } });

    const employeeId = state.fixture.hospitality.payrollPersonIds.employee;
    const peopleApi = `/api/v1/holdings/riverside-lantern/people/people/${employeeId}`;
    await employeePage.goto(`/holdings/riverside-lantern/people/${employeeId}?tab=pay`, { waitUntil: "domcontentloaded" });
    await employeePage.getByLabel("Work date", { exact: true }).fill("2026-09-22");
    await employeePage.getByLabel("Regular hours", { exact: true }).fill("8");
    await employeePage.getByLabel("Overtime hours", { exact: true }).fill("1");
    await employeePage.getByLabel("Work note", { exact: true }).fill("Fictional scheduled front desk coverage from the published roster.");
    const timeSubmit = employeePage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/people/time-entries"));
    await employeePage.getByRole("button", { name: "Submit time for approval", exact: true }).click();
    expect((await timeSubmit).status()).toBe(201);
    let personDetail = (await (await employeeContext.request.get(peopleApi)).json()).data;
    const submittedTime = personDetail.time.find((item: { work_date: string }) => item.work_date === "2026-09-22");
    expect(submittedTime).toMatchObject({ regular_hours: 8, overtime_hours: 1, status: "submitted" });
    await employeePage.reload({ waitUntil: "domcontentloaded" });
    await expect(employeePage.locator("#workspace-content").getByText("8h + 1h overtime · submitted", { exact: true })).toBeVisible();

    // Time-entry administration is exposed to HR/time reviewers. The manager
    // has already completed the overtime and leave manager steps above.
    await hrPage.goto(`/holdings/riverside-lantern/people/${employeeId}?tab=pay`, { waitUntil: "domcontentloaded" });
    const timeCard = hrPage.locator("article").filter({ hasText: "2026-09-22" });
    const timeApproval = hrPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(`/people/time-entries/${submittedTime.id}/approve`));
    await timeCard.getByRole("button", { name: "Approve time", exact: true }).click();
    expect((await timeApproval).ok()).toBe(true);
    await hrPage.reload({ waitUntil: "domcontentloaded" });
    personDetail = (await (await hrContext.request.get(peopleApi)).json()).data;
    expect(personDetail.time.find((item: { id: string }) => item.id === submittedTime.id)).toMatchObject({ status: "approved", approved_by: expect.any(String) });

    await employeePage.goto("/holdings/riverside-lantern/leave/new", { waitUntil: "domcontentloaded" });
    await expect(employeePage.getByRole("heading", { name: "Request leave", exact: true })).toBeVisible();
    await employeePage.getByRole("combobox").first().selectOption(state.fixture.hospitality.leaveTypeId);
    await employeePage.getByLabel("First day", { exact: true }).first().fill("2026-09-29");
    await employeePage.getByLabel("Last day", { exact: true }).first().fill("2026-09-29");
    await employeePage.getByLabel(/Handover note/).first().fill("Fictional leave handover for rostered front desk coverage.");
    await employeePage.getByRole("button", { name: "Review request", exact: true }).click();
    await expect(employeePage.getByRole("heading", { name: "Check the calculation", exact: true })).toBeVisible();
    await expect(employeePage.getByText("1 working day", { exact: true })).toBeVisible();
    await employeePage.getByText("Working dates and portions · 1", { exact: true }).click();
    await expect(employeePage.getByText(/2026-09-29 · full · roster/)).toBeVisible();
    await employeePage.getByRole("button", { name: "Submit leave request", exact: true }).click();
    await expect(employeePage).toHaveURL(/\/tasks\/people_payroll\/leave-/);
    const leaveId = new URL(employeePage.url()).pathname.split("/").at(-1)!;
    let leaveDetail = (await (await employeeContext.request.get(`/api/v1/holdings/riverside-lantern/leave-requests/${leaveId}`)).json()).data;
    expect(leaveDetail).toMatchObject({ state: "pending_approval", calculated_units: 1 });

    await managerPage.goto(`/holdings/riverside-lantern/tasks/people_payroll/${leaveId}`, { waitUntil: "domcontentloaded" });
    await expect(managerPage.getByRole("button", { name: "Approve", exact: true })).toBeVisible();
    const managerLeaveApproval = managerPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(`/leave-requests/${leaveId}/approve`));
    await managerPage.getByRole("button", { name: "Approve", exact: true }).click();
    expect((await managerLeaveApproval).ok()).toBe(true);
    leaveDetail = (await (await managerContext.request.get(`/api/v1/holdings/riverside-lantern/leave-requests/${leaveId}`)).json()).data;
    expect(leaveDetail.state).toBe("pending_approval");
    await managerPage.reload({ waitUntil: "domcontentloaded" });
    await expect(managerPage.getByRole("button", { name: "Approve", exact: true })).toHaveCount(0);

    await hrPage.goto(`/holdings/riverside-lantern/tasks/people_payroll/${leaveId}`, { waitUntil: "domcontentloaded" });
    const hrLeaveApproval = hrPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith(`/leave-requests/${leaveId}/approve`));
    await hrPage.getByRole("button", { name: "Approve", exact: true }).click();
    expect((await hrLeaveApproval).ok()).toBe(true);
    await hrPage.reload({ waitUntil: "domcontentloaded" });
    leaveDetail = (await (await hrContext.request.get(`/api/v1/holdings/riverside-lantern/leave-requests/${leaveId}`)).json()).data;
    expect(leaveDetail).toMatchObject({ state: "approved", version: 3 });
    await expect(hrPage.getByRole("heading", { name: "Synthetic annual leave", exact: true })).toBeVisible();
    await expect(hrPage.getByText("Your leave is approved.", { exact: true })).toBeVisible();
  } finally {
    await Promise.all([employeeContext.close(), managerContext.close(), hrContext.close(), unauthorizedContext.close()]);
  }
});
