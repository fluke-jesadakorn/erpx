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
    headers: { Origin: "http://127.0.0.1:3102" },
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
    headers: { Origin: "http://127.0.0.1:3102" },
    data: { assignmentId: assignment.id, revision: active.revision },
  });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto(`/holdings/simulation/people?view=payroll&refresh=${Date.now()}`, {
    waitUntil: "domcontentloaded",
  });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute(
    "data-work-stale",
    "false",
  );
}

test("isolated demo calculates and independently approves a payroll preview", async ({
  browser,
}, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(
    await readFile(resolve(directory, "credentials.json"), "utf8"),
  ) as Credentials;
  const preparerContext = await browser.newContext(testInfo.project.use);
  const routingContext = await browser.newContext(testInfo.project.use);
  const approverContext = await browser.newContext(testInfo.project.use);
  try {
    const preparerPage = await preparerContext.newPage();
    const routingPage = await routingContext.newPage();
    const approverPage = await approverContext.newPage();
    await signInRole(preparerContext, preparerPage, credentials, "payroll_preparer");
    await preparerPage
      .getByRole("button", { name: "Create payroll preview", exact: true })
      .click();
    const createForm = preparerPage.locator("form").filter({
      hasText: "Select pay group",
    });
    await createForm
      .locator('select[name="payGroupId"]')
      .selectOption({ label: "Demo monthly payroll · THB" });
    await createForm.locator('input[name="periodStart"]').fill("2026-09-01");
    await createForm.locator('input[name="periodEnd"]').fill("2026-09-30");
    await createForm.locator('input[name="payDate"]').fill("2026-09-30");
    const createResponse = preparerPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith("/api/v1/holdings/simulation/people/payroll/runs"),
    );
    await createForm.getByRole("button", { name: "Save payroll run", exact: true }).click();
    expect([200, 201]).toContain((await createResponse).status());
    await expect(preparerPage).toHaveURL(/\/people\/payroll\/[^/?]+/);
    const runUrl = preparerPage.url();
    await expect(preparerPage.getByRole("button", { name: "Calculate gross-pay preview", exact: true })).toBeVisible();
    const calculateResponse = preparerPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes("/payroll/runs/") &&
        response.url().endsWith("/calculate"),
    );
    await preparerPage.getByRole("button", { name: "Calculate gross-pay preview", exact: true }).click();
    expect((await calculateResponse).status()).toBe(200);
    await expect(preparerPage.getByText(/preview/).first()).toBeVisible();
    await expect(preparerPage.getByText("THB 1000.00").first()).toBeVisible();

    const runId = new URL(preparerPage.url()).pathname.split("/").pop();
    expect(runId, "created payroll run id").toBeTruthy();
    await signInRole(routingContext, routingPage, credentials, "holding_owner");
    await routingPage.goto(`/holdings/simulation/work/routing/payroll/${runId}`, {
      waitUntil: "domcontentloaded",
    });
    const reviewStep = routingPage.locator("li").filter({
      hasText: "Review payroll preview",
    });
    const payrollReviewer = credentials.users.find((user) =>
      user.roles?.includes("payroll_approver"),
    );
    expect(payrollReviewer, "independent payroll reviewer").toBeTruthy();
    await reviewStep.locator("summary").click();
    await reviewStep
      .locator('select[name="userId"]')
      .selectOption(payrollReviewer!.userId);
    await reviewStep
      .locator('input[name="reason"]')
      .fill("Assign independent Demo payroll reviewer.");
    const routingResponse = routingPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes(`/flows/payroll/${runId}`),
    );
    await reviewStep.getByRole("button", { name: "Save owner", exact: true }).click();
    expect((await routingResponse).status()).toBe(200);

    await signInRole(approverContext, approverPage, credentials, "payroll_approver");
    await approverPage.goto(runUrl, { waitUntil: "domcontentloaded" });
    await expect(approverPage.getByRole("button", { name: "Approve reviewed preview", exact: true })).toBeVisible();
    const approveResponse = approverPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes("/payroll/runs/") &&
        response.url().endsWith("/approve"),
    );
    await approverPage.getByRole("button", { name: "Approve reviewed preview", exact: true }).click();
    expect((await approveResponse).status()).toBe(200);
    await expect(approverPage.getByText("approved").first()).toBeVisible();
    await preparerPage.reload({ waitUntil: "domcontentloaded" });
    await expect(preparerPage.getByText("approved").first()).toBeVisible();
  } finally {
    await preparerContext.close();
    await routingContext.close();
    await approverContext.close();
  }
});
