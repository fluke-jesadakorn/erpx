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
  const financeContext = await browser.newContext(testInfo.project.use);
  const treasuryContext = await browser.newContext(testInfo.project.use);
  try {
    const preparerPage = await preparerContext.newPage();
    const routingPage = await routingContext.newPage();
    const approverPage = await approverContext.newPage();
    const financePage = await financeContext.newPage();
    const treasuryPage = await treasuryContext.newPage();
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

    // Finance posts the approved payroll into the ledger, then prepares the
    // payable instruction. The two actions use the same source run but must
    // still survive a browser reload before the independent treasury handoff.
    await signInRole(financeContext, financePage, credentials, "finance_approver");
    await financePage.goto(runUrl, { waitUntil: "domcontentloaded" });
    const postForm = financePage.locator("form.business-form:visible").filter({
      hasText: "Post approved payroll to Finance",
    });
    await expect(postForm).toBeVisible();
    const periodSelect = postForm.locator('select[name="periodId"]');
    const periodValue = await periodSelect.locator("option").evaluateAll((options) => {
      const option = options.find((item) => item.value && item.textContent?.trim().startsWith("2026"));
      return option?.value ?? "";
    });
    expect(periodValue, "an open period covering the payroll pay date").toBeTruthy();
    await periodSelect.selectOption(periodValue);
    await postForm.locator('select[name="expenseAccountId"]').selectOption({ index: 1 });
    await postForm.locator('select[name="netPayableAccountId"]').selectOption({ index: 1 });
    const postResponse = financePage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith(`/payroll/runs/${runId}/post`),
    );
    await postForm.getByRole("button", { name: "Post GL and create payable obligation", exact: true }).click();
    const postResult = await postResponse;
    expect(postResult.status(), await postResult.text()).toBe(200);
    await expect(financePage.getByText("Prepare payroll payment batch", { exact: true })).toBeVisible();

    const prepareForm = financePage.locator("form.business-form:visible").filter({
      hasText: "Prepare payroll payment batch",
    });
    const prepareResponse = financePage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith("/people/payments/batches"),
    );
    await prepareForm.getByRole("button", { name: "Create payment batch", exact: true }).click();
    expect((await prepareResponse).status()).toBe(201);
    await expect(financePage.getByText(/draft · THB/).first()).toBeVisible();
    await financePage.reload({ waitUntil: "domcontentloaded" });
    await expect(financePage.getByText(/draft · THB/).first()).toBeVisible();

    // Treasury is a different authenticated user. It independently approves,
    // exports and confirms every generated payee row against a sandbox bank
    // result, which is the browser equivalent of the controlled handoff.
    await signInRole(treasuryContext, treasuryPage, credentials, "treasury");
    await treasuryPage.goto(runUrl, { waitUntil: "domcontentloaded" });
    await expect(treasuryPage.getByRole("button", { name: "Approve payment batch", exact: true })).toBeVisible();
    const approvePaymentResponse = treasuryPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes("/people/payments/batches/") &&
        response.url().endsWith("/approve"),
    );
    await treasuryPage.getByRole("button", { name: "Approve payment batch", exact: true }).click();
    expect((await approvePaymentResponse).status()).toBe(200);
    await expect(treasuryPage.getByText(/approved · THB/).first()).toBeVisible();

    const exportPaymentResponse = treasuryPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes("/people/payments/batches/") &&
        response.url().endsWith("/export"),
    );
    await treasuryPage.getByRole("button", { name: "Export payment instruction", exact: true }).click();
    expect((await exportPaymentResponse).status()).toBe(200);
    await expect(treasuryPage.getByText(/exported · THB/).first()).toBeVisible();

    const confirmationForm = treasuryPage.locator("form.business-form:visible").filter({
      hasText: "Confirm sandbox bank results",
    });
    await expect(confirmationForm).toBeVisible();
    const bankReferences = confirmationForm.locator('input[name^="ref:"]');
    const bankReferenceCount = await bankReferences.count();
    expect(bankReferenceCount).toBeGreaterThan(0);
    for (let index = 0; index < bankReferenceCount; index += 1) {
      await bankReferences.nth(index).fill(`DEMO-PAYROLL-${index + 1}`);
    }
    const confirmPaymentResponse = treasuryPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes("/people/payments/batches/") &&
        response.url().endsWith("/confirm"),
    );
    await confirmationForm.getByRole("button", { name: "Save payment results", exact: true }).click();
    expect((await confirmPaymentResponse).status()).toBe(200);
    await expect(treasuryPage.getByText(/settled · THB/).first()).toBeVisible();
    await treasuryPage.reload({ waitUntil: "domcontentloaded" });
    await expect(treasuryPage.getByText(/settled · THB/).first()).toBeVisible();
  } finally {
    await preparerContext.close();
    await routingContext.close();
    await approverContext.close();
    await financeContext.close();
    await treasuryContext.close();
  }
});
