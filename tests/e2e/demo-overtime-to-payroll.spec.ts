import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = {
  email: string;
  role: string;
  roles?: string[];
  userId: string;
};
type Credentials = { password: string; users: DemoUser[] };

async function signInRole(
  context: BrowserContext,
  page: Page,
  credentials: Credentials,
  role: string,
) {
  const user = credentials.users.find(
    (item) => item.roles?.includes(role) || item.role === role,
  );
  expect(user, `${role} demo account`).toBeTruthy();
  const login = await context.request.post("/api/auth/sign-in/email", {
    headers: { Origin: (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102") },
    data: { email: user!.email, password: credentials.password },
  });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find(
    (item: { role: string }) => item.role === role,
  );
  expect(assignment, `${role} active assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, {
    headers: { Origin: (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102") },
    data: { assignmentId: assignment.id, revision: active.revision },
  });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto("/holdings/simulation/home", { waitUntil: "domcontentloaded" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute(
    "data-work-stale",
    "false",
  );
}

test("isolated demo sends reviewed overtime from employee to payroll preview", async ({
  browser,
}, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(
    await readFile(resolve(directory, "credentials.json"), "utf8"),
  ) as Credentials;
  const employeeContext = await browser.newContext(testInfo.project.use);
  const managerContext = await browser.newContext(testInfo.project.use);
  const hrContext = await browser.newContext(testInfo.project.use);
  const payrollContext = await browser.newContext(testInfo.project.use);
  const unauthorizedContext = await browser.newContext(testInfo.project.use);
  try {
    const employeePage = await employeeContext.newPage();
    const managerPage = await managerContext.newPage();
    const hrPage = await hrContext.newPage();
    const payrollPage = await payrollContext.newPage();
    const unauthorizedPage = await unauthorizedContext.newPage();

    await signInRole(employeeContext, employeePage, credentials, "employee");
    await employeePage.goto("/holdings/simulation/overtime/new", {
      waitUntil: "domcontentloaded",
    });
    await employeePage.getByLabel("Request path").selectOption("retrospective");
    await employeePage.getByLabel("Actual start").fill("2026-09-10T22:00");
    await employeePage.getByLabel("Actual end").fill("2026-09-11T02:00");
    await employeePage.getByLabel("Unpaid break (minutes)").fill("30");
    await employeePage.getByLabel("Reason", { exact: true }).fill(
      "Fictional Demo support escalation coverage.",
    );
    await employeePage.getByLabel("Late-submission explanation").fill(
      "This acceptance scenario records a retrospective claim after the work date.",
    );
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
    await expect(employeePage.getByLabel("Unpaid break (minutes)")).toHaveValue("30");

    await employeePage.getByRole("button", { name: "Review calculated hours" }).click();
    await expect(employeePage.getByText("3.5 hours", { exact: true })).toBeVisible();
    const submitResponse = employeePage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith(`/overtime-requests/${overtimeId}`),
    );
    await employeePage.getByRole("button", { name: "Submit claim", exact: true }).click();
    expect((await submitResponse).status()).toBe(200);
    await expect(employeePage.getByRole("heading", { name: "Manager verification" })).toBeVisible();

    await signInRole(unauthorizedContext, unauthorizedPage, credentials, "purchasing_operator");
    const hiddenRequest = await unauthorizedContext.request.get(
      `/api/v1/holdings/simulation/overtime-requests/${overtimeId}`,
    );
    expect(hiddenRequest.status()).toBe(404);

    await signInRole(managerContext, managerPage, credentials, "people_manager");
    await managerPage.goto(overtimeUrl, { waitUntil: "domcontentloaded" });
    await expect(managerPage.getByRole("button", { name: "Verify actual hours", exact: true })).toBeVisible();
    const verifyResponse = managerPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith(`/overtime-requests/${overtimeId}`),
    );
    await managerPage.getByRole("button", { name: "Verify actual hours", exact: true }).click();
    expect((await verifyResponse).status()).toBe(200);
    await expect(managerPage.getByRole("heading", { name: "HR approval" })).toBeVisible();
    await managerPage.reload({ waitUntil: "domcontentloaded" });
    await expect(managerPage.getByRole("heading", { name: "HR approval" })).toBeVisible();

    await signInRole(hrContext, hrPage, credentials, "hr_admin");
    await hrPage.goto(overtimeUrl, { waitUntil: "domcontentloaded" });
    await expect(hrPage.getByRole("button", { name: "Approve hours & hand off", exact: true })).toBeVisible();
    const approveResponse = hrPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith(`/overtime-requests/${overtimeId}`),
    );
    await hrPage.getByRole("button", { name: "Approve hours & hand off", exact: true }).click();
    expect((await approveResponse).status()).toBe(200);
    const persistedOvertimeAfterApproval = await hrContext.request.get(
      `/api/v1/holdings/simulation/overtime-requests/${overtimeId}`,
    );
    expect(persistedOvertimeAfterApproval.status()).toBe(200);
    expect((await persistedOvertimeAfterApproval.json()).data).toMatchObject({
      state: "approved",
      handoff: expect.objectContaining({ request_id: overtimeId, minutes: 210 }),
    });
    await hrPage.reload({ waitUntil: "domcontentloaded" });
    const persistedOvertime = await hrContext.request.get(
      `/api/v1/holdings/simulation/overtime-requests/${overtimeId}`,
    );
    expect(persistedOvertime.status()).toBe(200);
    expect((await persistedOvertime.json()).data).toMatchObject({
      state: "approved",
      handoff: expect.objectContaining({ request_id: overtimeId, minutes: 210 }),
    });

    await signInRole(payrollContext, payrollPage, credentials, "payroll_preparer");
    await payrollPage.goto("/holdings/simulation/people?view=payroll", {
      waitUntil: "domcontentloaded",
    });
    await payrollPage.getByRole("button", { name: "Create payroll preview", exact: true }).click();
    const createForm = payrollPage.locator("form").filter({ hasText: "Select pay group" });
    await createForm
      .locator('select[name="payGroupId"]')
      .selectOption({ label: "Demo monthly payroll · THB" });
    await createForm.locator('input[name="periodStart"]').fill("2026-09-01");
    await createForm.locator('input[name="periodEnd"]').fill("2026-09-30");
    await createForm.locator('input[name="payDate"]').fill("2026-09-30");
    const createResponse = payrollPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith("/api/v1/holdings/simulation/people/payroll/runs"),
    );
    await createForm.getByRole("button", { name: "Save payroll run", exact: true }).click();
    expect([200, 201]).toContain((await createResponse).status());
    await expect(payrollPage).toHaveURL(/\/people\/payroll\/[^/?]+/);
    const runId = new URL(payrollPage.url()).pathname.split("/").pop();
    expect(runId).toBeTruthy();
    const calculateResponse = payrollPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith(`/payroll/runs/${runId}/calculate`),
    );
    await payrollPage.getByRole("button", { name: "Calculate gross-pay preview", exact: true }).click();
    expect((await calculateResponse).status()).toBe(200);
    await expect(payrollPage.getByText("THB 1000.00").first()).toBeVisible();

    const payrollResponse = await payrollContext.request.get(
      `/api/v1/holdings/simulation/people/payroll/runs/${runId}`,
    );
    expect(payrollResponse.status()).toBe(200);
    const payrollData = (await payrollResponse.json()).data as {
      items: Array<{ input_snapshot_json: string }>;
    };
    const item = payrollData.items.find((entry) => {
      const snapshot = JSON.parse(entry.input_snapshot_json) as {
        hours: { handoffs: Array<{ request_id: string }> };
      };
      return snapshot.hours.handoffs.some((handoff) => handoff.request_id === overtimeId);
    });
    expect(item, "the calculated payroll item includes the approved overtime request").toBeTruthy();
    const input = JSON.parse(item!.input_snapshot_json) as {
      hours: { overtime: number; handoffs: Array<{ id: string; request_id: string; minutes: number }> };
    };
    expect(input.hours.overtime).toBe(3.5);
    expect(input.hours.handoffs).toContainEqual(
      expect.objectContaining({ request_id: overtimeId, minutes: 210 }),
    );
  } finally {
    await Promise.all([
      employeeContext.close(),
      managerContext.close(),
      hrContext.close(),
      payrollContext.close(),
      unauthorizedContext.close(),
    ]);
  }
});
