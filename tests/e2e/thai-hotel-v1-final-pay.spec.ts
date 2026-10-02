import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; userId: string };
type Credentials = { password: string; users: DemoUser[] };
type DemoState = {
  fixture: {
    hospitality: {
      payrollEngagementIds: Record<string, string>;
      payrollPersonIds: Record<string, string>;
    };
  };
};

async function signIn(
  context: BrowserContext,
  page: Page,
  credentials: Credentials,
  role: string,
) {
  const user = credentials.users.find(
    (item) =>
      item.role === role && item.email.endsWith("@riverside-lantern.example"),
  );
  expect(user, `${role} Riverside account`).toBeTruthy();
  const origin = process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102";
  const login = await context.request.post("/api/auth/sign-in/email", {
    headers: { Origin: origin },
    data: { email: user!.email, password: credentials.password },
  });
  expect(login.ok(), `${role} login`).toBe(true);
  const endpoint =
    "/api/v1/holdings/riverside-lantern/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find(
    (item: { role: string }) => item.role === role,
  );
  expect(assignment, `${role} active assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, {
    headers: { Origin: origin },
    data: { assignmentId: assignment.id, revision: active.revision },
  });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto("/holdings/riverside-lantern/home", {
    waitUntil: "domcontentloaded",
  });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute(
    "data-work-stale",
    "false",
  );
  const workHeaders = await page.locator("[data-work-revision]").evaluate((node) => ({
    "x-erpx-context-revision": node.getAttribute("data-work-revision") ?? "",
    "x-erpx-context-company": node.getAttribute("data-work-company") ?? "",
    "x-erpx-context-assignment": node.getAttribute("data-work-assignment") ?? "",
  }));
  return { user: user!, workHeaders };
}

test("H002 prepares and calculates person-scoped final-pay mock from retirement clearance", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  expect(run, "The Thai Hotel mock runner must supply an exact prepared simulation run.").toMatch(/^[a-f0-9]{12}$/);
  test.setTimeout(240_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(
    await readFile(resolve(directory, "credentials.json"), "utf8"),
  ) as Credentials;
  const state = JSON.parse(
    await readFile(resolve(directory, "state.json"), "utf8"),
  ) as DemoState;
  const fixtures = state.fixture.hospitality;
  const personId = fixtures.payrollPersonIds.employee;
  const engagementId = fixtures.payrollEngagementIds.employee;
  expect(personId).toBeTruthy();
  expect(engagementId).toBeTruthy();

  const hr = await browser.newContext(testInfo.project.use);
  const payroll = await browser.newContext(testInfo.project.use);
  [hr, payroll].forEach((context) => context.setDefaultTimeout(15_000));
  try {
    const hrPage = await hr.newPage();
    const { workHeaders } = await signIn(hr, hrPage, credentials, "hr_admin");
    const payrollUser = credentials.users.find(
      (item) =>
        item.role === "payroll_preparer" &&
        item.email.endsWith("@riverside-lantern.example"),
    );
    expect(payrollUser, "assigned final-pay preparer").toBeTruthy();

    const origin = process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102";
    const api = "/api/v1/holdings/riverside-lantern/people/lifecycle";
    const lifecycle = {
      expectedVersion: 0,
      kind: "offboarding",
      departureReason: "retirement",
      personId,
      engagementId,
      positionId: null,
      managerPersonId: null,
      effectiveAt: Date.parse("2026-09-28T00:00:00+07:00"),
      timezone: "Asia/Bangkok",
      lastWorkingDate: "2026-09-27",
      clearanceOwners: {
        equipment: null,
        payroll: payrollUser!.userId,
        finance: null,
      },
      reason: "Synthetic Thai Hotel v1 final-pay acceptance scenario",
    };
    const previewResponse = await hr.request.post(`${api}/cases/preview`, {
      headers: {
        Origin: origin,
        ...workHeaders,
        "Idempotency-Key": crypto.randomUUID(),
      },
      data: lifecycle,
    });
    expect(previewResponse.ok(), await previewResponse.text()).toBe(true);
    const preview = (await previewResponse.json()).data;
    expect(preview.token).toBeTruthy();

    const submitResponse = await hr.request.post(`${api}/cases/submit`, {
      headers: {
        Origin: origin,
        ...workHeaders,
        "Idempotency-Key": crypto.randomUUID(),
      },
      data: { ...lifecycle, previewToken: preview.token },
    });
    expect(submitResponse.ok(), await submitResponse.text()).toBe(true);
    const retirementCase = (await submitResponse.json()).data;
    expect(retirementCase).toMatchObject({ status: "scheduled" });

    const payrollPage = await payroll.newPage();
    await signIn(payroll, payrollPage, credentials, "payroll_preparer");
    await payrollPage.goto(
      `/holdings/riverside-lantern/people/clearance/${retirementCase.id}`,
      { waitUntil: "domcontentloaded" },
    );
    const clearance = payrollPage
      .locator("section.card")
      .filter({ has: payrollPage.getByRole("heading", { name: "Prepare final pay" }) });
    await expect(clearance).toBeVisible();
    await clearance.getByLabel("Period start", { exact: true }).fill("2026-09-01");
    await clearance.getByLabel("Period end", { exact: true }).fill("2026-09-27");
    await clearance.getByLabel("Pay date", { exact: true }).fill("2026-09-28");
    await clearance
      .getByRole("textbox", { name: "Preparation reason" })
      .fill("Synthetic final-pay mock through the employee's last working day.");
    const finalPayResponse = payrollPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes(`/people/lifecycle/tasks/`) &&
        response.url().endsWith("/final-pay"),
    );
    await clearance.getByRole("button", { name: "Create final-pay preview" }).click();
    expect((await finalPayResponse).ok()).toBe(true);

    const finalPayLink = payrollPage.getByRole("link", {
      name: /payrun-final-/,
    });
    await expect(finalPayLink).toBeVisible();
    const runId = new URL(await finalPayLink.getAttribute("href")!, origin)
      .pathname.split("/")
      .at(-1)!;
    await finalPayLink.click();
    await expect(payrollPage).toHaveURL(new RegExp(`/people/payroll/runs/${runId}$`));
    await expect(payrollPage.locator("[data-work-stale]")).toHaveAttribute(
      "data-work-stale",
      "false",
    );
    await expect(payrollPage.getByRole("heading", { level: 1 })).toBeVisible();
    await expect(
      payrollPage.locator('[data-payroll-adapter="mock"]'),
    ).toContainText("not a Thai statutory calculation");
    const calculationEvents: string[] = [];
    payrollPage.on("request", (request) => {
      const path = new URL(request.url()).pathname;
      if (path.includes("/people/payroll/runs/")) calculationEvents.push(`${request.method()} ${path}`);
    });
    payrollPage.on("requestfailed", (request) => {
      const path = new URL(request.url()).pathname;
      if (path.includes("/people/payroll/runs/")) calculationEvents.push(`failed ${request.method()} ${path}`);
    });
    payrollPage.on("pageerror", (error) => calculationEvents.push(`pageerror ${error.name}`));
    const calculation = payrollPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith(`/people/payroll/runs/${runId}/calculate`),
    ).catch((error: unknown) => {
      throw new Error(`Missing final-pay calculate response; observed: ${JSON.stringify(calculationEvents.slice(-20))}`, { cause: error });
    });
    await payrollPage
      .getByRole("button", { name: "Calculate mock preview", exact: true })
      .click();
    expect((await calculation).ok()).toBe(true);

    const detailResponse = await payroll.request.get(
      `/api/v1/holdings/riverside-lantern/people/payroll/runs/${runId}`,
    );
    expect(detailResponse.ok(), await detailResponse.text()).toBe(true);
    const detail = (await detailResponse.json()).data as {
      run: Record<string, unknown>;
      items: Array<Record<string, unknown>>;
    };
    expect(detail.run).toMatchObject({
      status: "preview",
      mode: "final_pay",
      scope_engagement_id: engagementId,
      period_start: "2026-09-01",
      period_end: "2026-09-27",
      pay_date: "2026-09-28",
    });
    expect(detail.items).toHaveLength(1);
    expect(detail.items[0]).toMatchObject({ person_id: personId });
    expect(Number(detail.items[0].gross_minor)).toBeGreaterThan(0);
    expect(
      Number(detail.items[0].gross_minor) -
        Number(detail.items[0].deductions_minor),
    ).toBe(Number(detail.items[0].net_minor));
    await expect(
      payrollPage.getByRole("heading", { name: "Employee preview breakdown", exact: true }).first(),
    ).toBeVisible();
    await expect(
      payrollPage.getByRole("heading", { name: "Payslips", exact: true }),
    ).toBeVisible();
    await expect(
      payrollPage.getByText(/Live payslips require validated Thai calculations/),
    ).toBeVisible();
  } finally {
    await Promise.all([hr.close(), payroll.close()]);
  }
});
