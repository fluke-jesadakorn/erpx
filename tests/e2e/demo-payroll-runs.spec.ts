import {createEngineeringBrowserContext} from "./helpers/engineering-browser-evidence";
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
  destination?: string,
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
  await page.goto(destination ?? `/holdings/simulation/people?view=payroll&refresh=${Date.now()}`, {
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
  test.setTimeout(180_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(
    await readFile(resolve(directory, "credentials.json"), "utf8"),
  ) as Credentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const preparerContext = await createEngineeringBrowserContext(browser,testInfo);
  const routingContext = await createEngineeringBrowserContext(browser,testInfo);
  const approverContext = await createEngineeringBrowserContext(browser,testInfo);
  const financeContext = await createEngineeringBrowserContext(browser,testInfo);
  const treasuryContext = await createEngineeringBrowserContext(browser,testInfo);
  const mappingDraftContext = await createEngineeringBrowserContext(browser,testInfo);
  const mappingApprovalContext = await createEngineeringBrowserContext(browser,testInfo);
  try {
    const preparerPage = await preparerContext.newPage();
    let routingPage = await routingContext.newPage();
    let approverPage = await approverContext.newPage();
    let financePage = await financeContext.newPage();
    let treasuryPage = await treasuryContext.newPage();
    let mappingDraftPage = await mappingDraftContext.newPage();
    let mappingApprovalPage = await mappingApprovalContext.newPage();
    await signInRole(preparerContext, preparerPage, credentials, "payroll_preparer", undefined, "/holdings/simulation/people/payroll/settings");
    await expect(preparerPage.getByRole("heading", { name: "Settings", exact: true })).toBeVisible();

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
    const mappingDraftForm = mappingDraftCard.locator("form.ui-form-grid");
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
      .getByRole("button", { name: "Create payroll run", exact: true })
      .click();
    const createDialog = preparerPage.getByRole("dialog", { name: "Create payroll run", exact: true });
    await expect(createDialog).toHaveCount(1);
    await expect(createDialog).toBeVisible();
    const createForm = createDialog.locator("form").filter({
      hasText: "Select pay group",
    });
    await createForm
      .locator('select[name="payGroupId"]')
      .selectOption({ label: "Demo monthly payroll · THB" });
    // Recover an accidentally entered future work period through the native
    // creator cancellation command; retain its source instead of resetting it.
    await createForm.locator('input[name="periodStart"]').fill("2026-10-01");
    await createForm.locator('input[name="periodEnd"]').fill("2026-10-31");
    await createForm.locator('input[name="payDate"]').fill("2026-11-01");
    const createResponse = preparerPage.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().endsWith("/api/v1/holdings/simulation/people/payroll/runs"),
    );
    await createForm.getByRole("button", { name: "Save payroll run", exact: true }).click();
    expect([200, 201]).toContain((await createResponse).status());
    await expect(preparerPage).toHaveURL(/\/people\/payroll\/runs\/[^/?]+/);
    const cancelledRunId = new URL(preparerPage.url()).pathname.split("/").pop()!;
    await expect(preparerPage.locator('#workspace-content').getByText("Payroll preview is available after the work period closes.", { exact: true })).toBeVisible();
    await expect(preparerPage.getByRole("button", { name: "Calculate payroll preview", exact: true })).toBeDisabled();
    const cancellation = preparerPage.getByRole("region", { name: "Cancel payroll draft", exact: true });
    await cancellation.getByRole("button", { name: "Cancel payroll draft", exact: true }).click();
    const cancellationReason = "Correct accidental future-period draft; keep its evidence and prepare completed October work.";
    await cancellation.getByLabel("Cancellation reason", { exact: true }).fill(cancellationReason);
    const cancellationResponse = preparerPage.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith(`/payroll/runs/${cancelledRunId}/cancel`));
    await cancellation.getByRole("button", { name: "Confirm payroll cancellation", exact: true }).click();
    const cancellationResult = await cancellationResponse;
    expect(cancellationResult.status(), await cancellationResult.text()).toBe(200);
    expect((await cancellationResult.json()).data).toMatchObject({ id: cancelledRunId, status: "cancelled", version: 2 });
    await preparerPage.reload({ waitUntil: "domcontentloaded" });
    await expect(preparerPage.getByRole("status").filter({ hasText: "Payroll run cancelled." })).toBeVisible();
    const retainedWorkspace = preparerPage.locator("#workspace-content");
    await retainedWorkspace.getByText("Approval details and history", { exact: true }).click();
    await retainedWorkspace.locator("summary").filter({ hasText: /^Handoff history \(/ }).click();
    await expect(retainedWorkspace.locator("li").filter({ hasText: cancellationReason })).toBeVisible();
    const cancellationManifest = JSON.parse(await readFile(resolve(directory, "databases.json"), "utf8")) as DatabaseManifest;
    const cancellationDb = new Pool({ connectionString: cancellationManifest.databases.H001_DB, max: 1 });
    try {
      const result = await cancellationDb.query<{ status: string; version: string; prior_status: string; prior_version: string; source_snapshot_json: string; reason: string }>(
        "SELECT r.status,r.version,c.prior_status,c.prior_version,c.source_snapshot_json,c.reason FROM hr_payroll_runs r JOIN hr_payroll_run_cancellations c ON c.run_id=r.id AND c.company_id=r.company_id WHERE r.company_id=$1 AND r.id=$2",
        [state.fixture.companyId, cancelledRunId],
      );
      expect(result.rows).toHaveLength(1);
      expect(result.rows[0]).toMatchObject({ status: "cancelled", version: "2", prior_status: "calculating", prior_version: "1", reason: cancellationReason });
      expect(JSON.parse(result.rows[0].source_snapshot_json)).toMatchObject({ status: "calculating", version: 1, revision: 1, period_start: "2026-10-01", period_end: "2026-10-31", pay_date: "2026-11-01", approved_by: null, posted_by: null });
      const sources = await cancellationDb.query<{ journals: string; batches: string; audit: string; outbox: string }>(
        "SELECT (SELECT COUNT(*) FROM finance_journals WHERE source_type='payroll' AND source_id=$1) journals,(SELECT COUNT(*) FROM hr_payment_batches WHERE source_type='payroll' AND source_id=$1) batches,(SELECT COUNT(*) FROM audit_events WHERE action='people.payroll.cancel' AND record_id=$1) audit,(SELECT COUNT(*) FROM outbox_events WHERE event_type='people.payroll.cancel' AND aggregate_id=$1) outbox",
        [cancelledRunId],
      );
      expect(sources.rows[0]).toEqual({ journals: "0", batches: "0", audit: "1", outbox: "1" });
    } finally { await cancellationDb.end(); }

    await preparerPage.goto("/holdings/simulation/people/payroll/settings", { waitUntil: "commit" });
    await preparerPage.getByRole("button", { name: "Create payroll run", exact: true }).click();
    const completedDialog = preparerPage.getByRole("dialog", { name: "Create payroll run", exact: true });
    await expect(completedDialog).toHaveCount(1);
    await expect(completedDialog).toBeVisible();
    const completedForm = completedDialog.locator("form").filter({ hasText: "Select pay group" });
    await completedForm.locator('select[name="payGroupId"]').selectOption({ label: "Demo monthly payroll · THB" });
    // September OT retains its own preview. October 1–2 is completed as of the
    // declared engineering fixture clock and does not overlap that source.
    await completedForm.locator('input[name="periodStart"]').fill("2026-10-01");
    await completedForm.locator('input[name="periodEnd"]').fill("2026-10-02");
    await completedForm.locator('input[name="payDate"]').fill("2026-10-03");
    const completedResponse = preparerPage.waitForResponse(response => response.request().method() === "POST" && response.url().endsWith("/api/v1/holdings/simulation/people/payroll/runs"));
    await completedForm.getByRole("button", { name: "Save payroll run", exact: true }).click();
    expect([200, 201]).toContain((await completedResponse).status());
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
    await expect(preparerPage.locator('#workspace-content').getByText(/preview/).first()).toBeVisible();
    await expect(preparerPage.locator('#workspace-content').getByText("THB 1,000.00").first()).toBeVisible();

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
    const reviewStep = routingPage.locator("#workspace-content").getByRole("listitem").filter({
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
    await expect(preparerPage.locator('#workspace-content').getByText("Awaiting an independent approver.", { exact: true })).toBeVisible();

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
    await expect(approverPage.locator('#workspace-content').getByText("approved").first()).toBeVisible();
    await preparerPage.reload({ waitUntil: "domcontentloaded" });
    await expect(preparerPage.locator('#workspace-content').getByText("approved").first()).toBeVisible();

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
    const prepareForm = paymentPanel.locator("form.ui-form-grid:visible");
    await expect(prepareForm).toBeVisible();
    await financePage.locator('#workspace-content').getByLabel("Finance bank account ID", { exact: true }).fill(state.fixture.closeBankAccountId);
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

    const confirmationForm = treasuryPaymentPanel.locator("form.ui-form-grid:visible").filter({ hasText: "Reconcile bank results" });
    await expect(confirmationForm).toBeVisible();
    const bankReferences = confirmationForm.locator('input[name^="ref:"]');
    const bankIds = confirmationForm.locator('input[name^="bankId:"]');
    const bankReferenceCount = await bankReferences.count();
    expect(bankReferenceCount).toBeGreaterThan(0);
    expect(bankReferenceCount).toBe(1);
    const bankIdField = await bankIds.first().getAttribute("name");
    const paymentReference = bankIdField?.slice("bankId:".length);
    expect(paymentReference, "the generated payroll payment reference").toBeTruthy();

    // Import a synthetic bank statement through Finance and match its exact
    // debit, reference, date, currency and account during Treasury confirmation.
    await financePage.goto("/holdings/simulation/finance/bank?new=1", { waitUntil: "commit" });
    await expect(financePage.getByRole("heading", { name: "Import bank statement", exact: true })).toBeVisible();
    await expect(financePage.locator('#workspace-content').locator('form').filter({ has: financePage.getByRole("button", { name: "Validate and import", exact: true }) })).toBeVisible();
    await financePage.locator('#workspace-content').locator('select[name="bankAccount"]:visible').selectOption(state.fixture.closeBankAccountId);
    const statementNumber = `PAYROLL-SIM-${runId}`;
    await financePage.locator('#workspace-content').getByLabel("Statement number", { exact: true }).fill(statementNumber);
    await financePage.locator('#workspace-content').getByLabel("Statement date", { exact: true }).fill("2026-10-03");
    await financePage.locator('#workspace-content').getByLabel("Opening balance", { exact: true }).fill("100000.00");
    await financePage.locator('#workspace-content').getByLabel("Closing balance", { exact: true }).fill("99000.00");
    await financePage.locator('#workspace-content').getByLabel("Bank reference 1", { exact: true }).fill(paymentReference!);
    await financePage.locator('#workspace-content').getByLabel("Transaction date 1", { exact: true }).fill("2026-10-03");
    await financePage.locator('#workspace-content').getByLabel("Description 1", { exact: true }).fill("Synthetic payroll settlement debit");
    await financePage.locator('#workspace-content').getByLabel("Signed amount 1", { exact: true }).fill("-1000.00");
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
    const refreshedConfirmationForm = refreshedPaymentPanel.locator("form.ui-form-grid:visible").filter({ hasText: "Reconcile bank results" });
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
    const sourceDb = new Pool({ connectionString: databaseManifest.databases.H001_DB, max: 1 });
    try {
      const retained = await sourceDb.query<{ status: string; gross: string; net: string; batch_status: string; total_minor: string; bank_amount: string; match_status: string; matched_type: string; slips: string }>(
        `SELECT r.status,r.totals_json::jsonb->>'gross' gross,r.totals_json::jsonb->>'net' net,b.status batch_status,b.total_minor,
          t.amount_minor bank_amount,t.match_status,t.matched_type,(SELECT COUNT(*) FROM hr_issued_payslips WHERE run_id=r.id) slips
          FROM hr_payroll_runs r JOIN hr_payment_batches b ON b.source_id=r.id AND b.source_type='payroll' AND b.company_id=r.company_id
          JOIN hr_payroll_payment_settlements s ON s.batch_id=b.id AND s.company_id=r.company_id
          JOIN finance_bank_transactions t ON t.id=s.bank_transaction_id
          WHERE r.id=$1 AND r.company_id=$2`, [runId, state.fixture.companyId],
      );
      expect(retained.rows).toHaveLength(1);
      expect(retained.rows[0]).toEqual({ status: "posted", gross: "100000", net: "100000", batch_status: "settled", total_minor: "100000", bank_amount: "-100000", match_status: "matched", matched_type: "payroll_payment", slips: "1" });
      const ledger = await sourceDb.query<{ source_type: string; status: string; debit: string; credit: string }>(
        `SELECT j.source_type,j.status,SUM(l.debit_minor)::text debit,SUM(l.credit_minor)::text credit
          FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id
          WHERE j.id IN (SELECT journal_id FROM hr_payroll_postings WHERE run_id=$1
            UNION ALL SELECT s.journal_id FROM hr_payroll_payment_settlements s JOIN hr_payment_batches b ON b.id=s.batch_id WHERE b.source_type='payroll' AND b.source_id=$1)
          GROUP BY j.id,j.source_type,j.status ORDER BY j.source_type`, [runId],
      );
      expect(ledger.rows).toEqual([{ source_type: "payroll", status: "posted", debit: "100000", credit: "100000" }, { source_type: "payroll_payment", status: "posted", debit: "100000", credit: "100000" }]);
      const payable = await sourceDb.query<{ difference: string }>(
        `SELECT COALESCE(SUM(l.credit_minor-l.debit_minor),0)::text difference FROM hr_payroll_postings p
          JOIN hr_payroll_account_mappings m ON m.id=p.account_mapping_id
          JOIN finance_journal_lines l ON l.account_id=m.accounts_json::jsonb->>'netPayableAccountId'
          WHERE p.run_id=$1 AND l.journal_id IN (p.journal_id,
            (SELECT s.journal_id FROM hr_payroll_payment_settlements s JOIN hr_payment_batches b ON b.id=s.batch_id WHERE b.source_type='payroll' AND b.source_id=$1))`, [runId],
      );
      expect(payable.rows).toEqual([{ difference: "0" }]);
    } finally { await sourceDb.end(); }
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
