import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { Pool } from "pg";

type DemoUser = {
  label?: string;
  email: string;
  role: string;
  roles?: string[];
  userId: string;
};
type Credentials = { password: string; users: DemoUser[] };
type DemoState = { fixture: { companyId: string; closeBankAccountId: string } };
type DatabaseManifest = { databases: { H001_DB: string } };

async function signInRole(
  context: BrowserContext,
  page: Page,
  credentials: Credentials,
  role: string,
  userId?: string,
) {
  const user = credentials.users.find(
    (item) =>
      (!userId || item.userId === userId) &&
      (item.roles?.includes(role) || item.role === role),
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
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const preparerContext = await browser.newContext(testInfo.project.use);
  const routingContext = await browser.newContext(testInfo.project.use);
  const approverContext = await browser.newContext(testInfo.project.use);
  const financeContext = await browser.newContext(testInfo.project.use);
  const treasuryContext = await browser.newContext(testInfo.project.use);
  const mappingDraftContext = await browser.newContext(testInfo.project.use);
  const mappingApprovalContext = await browser.newContext(testInfo.project.use);
  try {
    const preparerPage = await preparerContext.newPage();
    let routingPage = await routingContext.newPage();
    let approverPage = await approverContext.newPage();
    let financePage = await financeContext.newPage();
    let treasuryPage = await treasuryContext.newPage();
    let mappingDraftPage = await mappingDraftContext.newPage();
    let mappingApprovalPage = await mappingApprovalContext.newPage();
    await signInRole(preparerContext, preparerPage, credentials, "payroll_preparer");
    const setupPayGroup = preparerPage.getByRole("link", { name: "Set up a pay group", exact: true });
    await expect(setupPayGroup).toBeVisible();
    await setupPayGroup.click();
    await expect(preparerPage).toHaveURL(/\/people\/payroll\/settings$/);

    // A recurring mapping needs a distinct Finance author and approver. Set it
    // up through the company controls before creating the first payroll run.
    const financeUsers = credentials.users.filter(
      (user) => user.roles?.includes("finance_approver") || user.role === "finance_approver",
    );
    const mappingAuthor = financeUsers.find((user) => user.label === "Finance reviewer") ?? financeUsers[0];
    const mappingReviewer = financeUsers.find((user) => user.userId !== mappingAuthor?.userId);
    expect(mappingAuthor, "Finance mapping author").toBeTruthy();
    expect(mappingReviewer, "independent Finance mapping approver").toBeTruthy();

    await signInRole(mappingDraftContext, mappingDraftPage, credentials, "finance_approver", mappingAuthor!.userId);
    await mappingDraftPage.close();
    mappingDraftPage = await mappingDraftContext.newPage();
    await mappingDraftPage.goto("/holdings/simulation/people/payroll/settings", { waitUntil: "commit" });
    const mappingDraftCard = mappingDraftPage.getByRole("region", { name: "Payroll account mapping", exact: true });
    await expect(mappingDraftCard).toBeVisible();
    await mappingDraftCard.getByLabel("Pay group").selectOption({ label: "Demo monthly payroll · THB" });
    const mappingDraftForm = mappingDraftCard.locator("form.business-form");
    await mappingDraftForm.locator('select[name="expenseAccountId"]').selectOption({ index: 1 });
    await mappingDraftForm.locator('select[name="netPayableAccountId"]').selectOption({ index: 1 });
    const mappingSaveResponse = mappingDraftPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes("/payroll/pay-groups/") &&
        response.url().endsWith("/account-mappings"),
    );
    await mappingDraftCard.getByRole("button", { name: "Save mapping draft", exact: true }).click();
    const mappingSaveResult = await mappingSaveResponse;
    expect(mappingSaveResult.status(), await mappingSaveResult.text()).toBe(200);
    const mappingId = String((await mappingSaveResult.json()).data.id);

    await signInRole(mappingApprovalContext, mappingApprovalPage, credentials, "finance_approver", mappingReviewer!.userId);
    await mappingApprovalPage.close();
    mappingApprovalPage = await mappingApprovalContext.newPage();
    await mappingApprovalPage.goto("/holdings/simulation/people/payroll/settings", { waitUntil: "commit" });
    const mappingApprovalCard = mappingApprovalPage.getByRole("region", { name: "Payroll account mapping", exact: true });
    await expect(mappingApprovalCard).toBeVisible();
    await mappingApprovalCard.getByLabel("Pay group").selectOption({ label: "Demo monthly payroll · THB" });
    await expect(mappingApprovalCard.getByText("draft · version 1", { exact: true })).toBeVisible();
    const mappingApprovalResponse = mappingApprovalPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith(`/payroll/account-mappings/${mappingId}/approve`),
    );
    await mappingApprovalCard.getByRole("button", { name: "Approve mapping", exact: true }).click();
    const mappingApprovalResult = await mappingApprovalResponse;
    expect(mappingApprovalResult.status(), await mappingApprovalResult.text()).toBe(200);
    await expect(mappingApprovalCard.getByText("approved · version 2", { exact: true })).toBeVisible();

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
    await createForm.locator('input[name="periodEnd"]').fill("2026-09-28");
    await createForm.locator('input[name="payDate"]').fill("2026-09-30");
    const createResponse = preparerPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith("/api/v1/holdings/simulation/people/payroll/runs"),
    );
    await createForm.getByRole("button", { name: "Save payroll run", exact: true }).click();
    expect([200, 201]).toContain((await createResponse).status());
    await expect(preparerPage).toHaveURL(/\/people\/payroll\/runs\/[^/?]+/);
    const runUrl = preparerPage.url();
    await expect(preparerPage.getByRole("button", { name: "Calculate payroll preview", exact: true })).toBeVisible();
    const calculateResponse = preparerPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes("/payroll/runs/") &&
        response.url().endsWith("/calculate"),
    );
    await preparerPage.getByRole("button", { name: "Calculate payroll preview", exact: true }).click();
    expect((await calculateResponse).status()).toBe(200);
    await expect(preparerPage.getByText(/preview/).first()).toBeVisible();
    await expect(preparerPage.getByText("THB 1,000.00").first()).toBeVisible();

    const runId = new URL(preparerPage.url()).pathname.split("/").pop();
    expect(runId, "created payroll run id").toBeTruthy();
    await signInRole(routingContext, routingPage, credentials, "holding_owner");
    // Sign-in refreshes several workspace queries. Use a fresh document for
    // the server-rendered routing page so a late refresh cannot cancel its
    // navigation while payroll work is being prepared in another context.
    await routingPage.close();
    routingPage = await routingContext.newPage();
    await routingPage.goto(`/holdings/simulation/work/routing/payroll/${runId}`, {
      waitUntil: "commit",
    });
    await expect(routingPage.getByRole("heading", { name: "Review responsibility", exact: true })).toBeVisible();
    const reviewStep = routingPage.locator("li").filter({
      hasText: "Review and approve payroll",
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

    const submitResponse = preparerPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith(`/payroll/runs/${runId}/submit-review`),
    );
    await preparerPage.getByRole("button", { name: "Submit for review", exact: true }).click();
    expect((await submitResponse).status()).toBe(200);
    await expect(preparerPage.getByText("Awaiting an independent approver.", { exact: true })).toBeVisible();

    await signInRole(approverContext, approverPage, credentials, "payroll_approver");
    await approverPage.close();
    approverPage = await approverContext.newPage();
    await approverPage.goto(runUrl, { waitUntil: "commit" });
    await expect(approverPage.getByRole("button", { name: "Approve submitted review", exact: true })).toBeVisible();
    const approveResponse = approverPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes("/payroll/runs/") &&
        response.url().endsWith("/approve"),
    );
    await approverPage.getByRole("button", { name: "Approve submitted review", exact: true }).click();
    expect((await approveResponse).status()).toBe(200);
    await expect(approverPage.getByText("approved").first()).toBeVisible();
    await preparerPage.reload({ waitUntil: "domcontentloaded" });
    await expect(preparerPage.getByText("approved").first()).toBeVisible();

    // Finance posts the approved payroll into the ledger, then prepares the
    // payable instruction. The two actions use the same source run but must
    // still survive a browser reload before the independent treasury handoff.
    await signInRole(financeContext, financePage, credentials, "finance_approver");
    await financePage.close();
    financePage = await financeContext.newPage();
    await financePage.goto(runUrl, { waitUntil: "commit" });
    const accounting = financePage.getByRole("region", { name: "Accounting preview", exact: true });
    await expect(accounting).toBeVisible();
    await expect(accounting.getByText(/Approved pay-group mapping/)).toBeVisible();
    const previewResponse = financePage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith(`/payroll/runs/${runId}/accounting-preview`),
    );
    await accounting.getByRole("button", { name: "Preview journal", exact: true }).click();
    const previewResult = await previewResponse;
    expect(previewResult.status(), await previewResult.text()).toBe(200);
    await expect(accounting.getByRole("heading", { name: "Journal lines", exact: true })).toBeVisible();
    await expect(accounting.getByText("Debit THB 1000.00 = Credit THB 1000.00", { exact: true })).toBeVisible();
    const postResponse = financePage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith(`/payroll/runs/${runId}/post`),
    );
    await accounting.getByRole("button", { name: "Post reviewed journal", exact: true }).click();
    const postResult = await postResponse;
    expect(postResult.status(), await postResult.text()).toBe(200);
    const paymentPanel = financePage.getByRole("region", { name: "Payroll payment batch", exact: true });
    const prepareForm = paymentPanel.locator("form.business-form:visible");
    await expect(prepareForm).toBeVisible();
    await financePage.getByLabel("Finance bank account ID", { exact: true }).fill(state.fixture.closeBankAccountId);
    const prepareResponse = financePage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith("/people/payments/batches"),
    );
    await prepareForm.getByRole("button", { name: "Prepare batch", exact: true }).click();
    expect((await prepareResponse).status()).toBe(201);
    await expect(paymentPanel.getByText(/1 payees · Draft/)).toBeVisible();
    await financePage.reload({ waitUntil: "domcontentloaded" });
    await expect(paymentPanel.getByText(/1 payees · Draft/)).toBeVisible();
    await financePage.goto("/holdings/simulation/finance/payments", { waitUntil: "commit" });
    await expect(financePage.getByRole("heading", { name: "Payment obligations", exact: true }).first()).toBeVisible();
    const payrollObligation = financePage.getByRole("row").filter({ hasText: "Payroll payment batch" });
    await expect(payrollObligation).toBeVisible();
    await expect(payrollObligation).toContainText("Approve payroll or expense payment");
    await financePage.goto(runUrl, { waitUntil: "commit" });

    // Treasury is a different authenticated user. It independently approves,
    // exports and confirms every generated payee row against a sandbox bank
    // result, which is the browser equivalent of the controlled handoff.
    await signInRole(treasuryContext, treasuryPage, credentials, "treasury");
    await treasuryPage.close();
    treasuryPage = await treasuryContext.newPage();
    await treasuryPage.goto(runUrl, { waitUntil: "commit" });
    const treasuryPaymentPanel = treasuryPage.getByRole("region", { name: "Payroll payment batch", exact: true });
    await expect(treasuryPaymentPanel.getByRole("button", { name: "Approve batch", exact: true })).toBeVisible();
    const approvePaymentResponse = treasuryPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes("/people/payments/batches/") &&
        response.url().endsWith("/approve"),
    );
    await treasuryPaymentPanel.getByRole("button", { name: "Approve batch", exact: true }).click();
    expect((await approvePaymentResponse).status()).toBe(200);
    await expect(treasuryPaymentPanel.getByText(/1 payees · Approved/)).toBeVisible();

    const exportPaymentResponse = treasuryPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes("/people/payments/batches/") &&
        response.url().endsWith("/export"),
    );
    await treasuryPaymentPanel.getByRole("button", { name: "Export instruction", exact: true }).click();
    expect((await exportPaymentResponse).status()).toBe(200);
    await expect(treasuryPaymentPanel.getByText(/1 payees · Exported/)).toBeVisible();

    const confirmationForm = treasuryPaymentPanel.locator("form.business-form:visible").filter({ hasText: "Reconcile bank results" });
    await expect(confirmationForm).toBeVisible();
    const bankReferences = confirmationForm.locator('input[name^="ref:"]');
    const bankIds = confirmationForm.locator('input[name^="bankId:"]');
    const bankVersions = confirmationForm.locator('input[name^="bankVersion:"]');
    const bankReferenceCount = await bankReferences.count();
    expect(bankReferenceCount).toBeGreaterThan(0);
    expect(bankReferenceCount).toBe(1);
    const bankIdField = await bankIds.first().getAttribute("name");
    const paymentReference = bankIdField?.slice("bankId:".length);
    expect(paymentReference, "the generated payroll payment reference").toBeTruthy();

    // Import a synthetic bank statement through Finance and match its exact
    // debit, reference, date, currency and account during Treasury confirmation.
    await financePage.goto("/holdings/simulation/finance/bank?new=1", { waitUntil: "commit" });
    await expect(financePage.getByRole("heading", { name: "Bank reconciliation", exact: true })).toBeVisible();
    await financePage.getByRole("button", { name: "Import bank statement", exact: true }).click();
    await financePage.locator('select[name="bankAccount"]:visible').selectOption(state.fixture.closeBankAccountId);
    const statementNumber = `PAYROLL-SIM-${runId}`;
    await financePage.getByLabel("Statement number", { exact: true }).fill(statementNumber);
    await financePage.getByLabel("Statement date", { exact: true }).fill("2026-09-30");
    await financePage.getByLabel("Opening balance", { exact: true }).fill("100000.00");
    await financePage.getByLabel("Closing balance", { exact: true }).fill("99000.00");
    await financePage.getByLabel("Bank reference 1", { exact: true }).fill(paymentReference!);
    await financePage.getByLabel("Transaction date 1", { exact: true }).fill("2026-09-30");
    await financePage.getByLabel("Description 1", { exact: true }).fill("Synthetic payroll settlement debit");
    await financePage.getByLabel("Signed amount 1", { exact: true }).fill("-1000.00");
    const bankImport = financePage.waitForResponse(
      (response) => response.request().method() === "POST" && response.url().endsWith("/finance/bank/statements/import"),
    );
    await financePage.getByRole("button", { name: "Validate and import", exact: true }).click();
    expect((await bankImport).status()).toBe(201);

    const databaseManifest = JSON.parse(await readFile(resolve(directory, "databases.json"), "utf8")) as DatabaseManifest;
    const payrollDb = new Pool({ connectionString: databaseManifest.databases.H001_DB, max: 1 });
    let bankTransaction: { id: string; version: number };
    try {
      const result = await payrollDb.query<{ id: string; version: number }>(
        "SELECT t.id,t.version FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE s.company_id=$1 AND s.bank_account=$2 AND t.external_id=$3",
        [state.fixture.companyId, state.fixture.closeBankAccountId, paymentReference],
      );
      expect(result.rows).toHaveLength(1);
      bankTransaction = result.rows[0];
    } finally {
      await payrollDb.end();
    }

    await treasuryPage.reload({ waitUntil: "commit" });
    const refreshedPaymentPanel = treasuryPage.getByRole("region", { name: "Payroll payment batch", exact: true });
    const refreshedConfirmationForm = refreshedPaymentPanel.locator("form.business-form:visible").filter({ hasText: "Reconcile bank results" });
    await expect(refreshedConfirmationForm).toBeVisible();
    await refreshedConfirmationForm.locator(`input[name="ref:${paymentReference}"]`).fill(paymentReference!);
    await refreshedConfirmationForm.locator(`input[name="bankId:${paymentReference}"]`).fill(bankTransaction.id);
    await refreshedConfirmationForm.locator(`input[name="bankVersion:${paymentReference}"]`).fill(String(bankTransaction.version));
    const confirmPaymentResponse = treasuryPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes("/people/payments/batches/") &&
        response.url().endsWith("/confirm"),
    );
    await refreshedConfirmationForm.getByRole("button", { name: "Confirm results", exact: true }).click();
    const confirmationResult = await confirmPaymentResponse;
    expect(confirmationResult.status(), await confirmationResult.text()).toBe(200);
    await expect(refreshedPaymentPanel.getByText(/1 payees · Settled/)).toBeVisible();
    await treasuryPage.reload({ waitUntil: "domcontentloaded" });
    await expect(treasuryPage.getByRole("region", { name: "Payroll payment batch", exact: true }).getByText(/1 payees · Settled/)).toBeVisible();
  } finally {
    await preparerContext.close();
    await routingContext.close();
    await approverContext.close();
    await financeContext.close();
    await treasuryContext.close();
    await mappingDraftContext.close();
    await mappingApprovalContext.close();
  }
});
