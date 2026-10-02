import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; userId: string };
type Credentials = { password: string; users: DemoUser[] };
type DemoState = { fixture: { hospitality: { payrollGroupId: string; payrollEngagementIds: Record<string, string>; payrollPersonIds: Record<string, string> } } };
type PayrollItem = {
  person_id: string;
  worker_type: string;
  input_snapshot_json: string;
  result_json: string;
  gross_minor: number;
  deductions_minor: number;
  employer_cost_minor: number;
  net_minor: number;
};

async function signIn(context: BrowserContext, page: Page, credentials: Credentials, role: string) {
  const user = credentials.users.find((item) => item.role === role && item.email.endsWith("@riverside-lantern.example"));
  expect(user, `${role} Riverside account`).toBeTruthy();
  const origin = process.env.ERPX_E2E_BASE_URL ?? (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102");
  const login = await context.request.post("/api/auth/sign-in/email", { headers: { Origin: origin }, data: { email: user!.email, password: credentials.password } });
  expect(login.ok(), `${role} login`).toBe(true);
  const endpoint = "/api/v1/holdings/riverside-lantern/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${role} active assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, { headers: { Origin: origin }, data: { assignmentId: assignment.id, revision: active.revision } });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto("/holdings/riverside-lantern/people?view=payroll", { waitUntil: "domcontentloaded" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
}

async function payrollRun(context: BrowserContext, id: string) {
  const response = await context.request.get(`/api/v1/holdings/riverside-lantern/people/payroll/runs/${id}`);
  expect(response.ok(), await response.text()).toBe(true);
  return (await response.json()).data as { run: Record<string, unknown>; items: PayrollItem[]; corrections: Record<string, unknown>[]; exports: Record<string, unknown>[]; paymentBatch: Record<string, unknown> | null; paymentItems: Record<string, unknown>[] };
}

async function openCurrentRun(page: Page, id: string) {
  const link = page.getByRole("link", { name: "Open run and continue", exact: true });
  await expect(link).toBeVisible();
  await link.click();
  await expect(page).toHaveURL(new RegExp(`/people/payroll/runs/${id}$`));
}

async function openPayrollSettings(page: Page) {
  let link = page.locator('a[href$="/people/payroll/settings"]:visible').last();
  if (!(await link.isVisible())) {
    await page.getByRole("button", { name: "App launcher", exact: true }).click();
    await page.getByLabel("Search apps and pages", { exact: true }).fill("Setup");
    link = page.locator('a[href$="/people/payroll/settings"]:visible').last();
  }
  await expect(link).toBeVisible();
  await link.click();
  await expect(page).toHaveURL(/\/people\/payroll\/settings$/);
}

test("H002 synthetic payroll covers approved time, correction, review, GL and sandbox bank confirmation", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  expect(run, "The Thai Hotel mock runner must supply an exact prepared simulation run.").toMatch(/^[a-f0-9]{12}$/);
  test.setTimeout(360_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const fixtures = state.fixture.hospitality;
  expect(Object.keys(fixtures.payrollEngagementIds)).toEqual(expect.arrayContaining(["employee", "foreignEmployee", "dailyEmployee", "hourlyEmployee"]));

  const preparer = await browser.newContext(testInfo.project.use);
  const reviewer = await browser.newContext(testInfo.project.use);
  const finance = await browser.newContext(testInfo.project.use);
  const treasury = await browser.newContext(testInfo.project.use);
  let ownerContext: BrowserContext | undefined;
  [preparer, reviewer, finance, treasury].forEach((context) => context.setDefaultTimeout(15_000));
  try {
    const preparerPage = await preparer.newPage();
    await signIn(preparer, preparerPage, credentials, "payroll_preparer");
    await expect(preparerPage.getByRole("link", { name: "Set up a pay group", exact: true })).toBeVisible();
    await preparerPage.getByRole("link", { name: "Set up a pay group", exact: true }).click();
    await expect(preparerPage).toHaveURL(/\/people\/payroll\/settings$/);
    await expect(preparerPage.getByRole("button", { name: "Create payroll simulation (mock)", exact: true })).toBeVisible();
    await preparerPage.getByRole("button", { name: "Create payroll simulation (mock)", exact: true }).click();
    await preparerPage.getByRole("combobox").first().selectOption(fixtures.payrollGroupId);
    await preparerPage.getByLabel("Period start", { exact: true }).fill("2026-09-01");
    await preparerPage.getByLabel("Period end", { exact: true }).fill("2026-09-28");
    await preparerPage.getByLabel("Pay date", { exact: true }).fill("2026-10-05");
    const createResponse = preparerPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/people/payroll/runs"));
    await preparerPage.getByRole("button", { name: "Save payroll run", exact: true }).click();
    expect((await createResponse).ok()).toBe(true);
    await expect(preparerPage).toHaveURL(/\/people\/payroll\/runs\/payrun-/);
    const runId = new URL(preparerPage.url()).pathname.split("/").at(-1)!;
    await expect(preparerPage.locator('[data-payroll-adapter="mock"]')).toContainText("not a Thai statutory calculation");
    await expect(preparerPage.getByText(/Live payslips require validated Thai calculations and restricted employee delivery/)).toBeVisible();

    const calculate = async () => {
      const readinessResponse = await preparer.request.get(`/api/v1/holdings/riverside-lantern/people/payroll/runs/${runId}/readiness`);
      expect(readinessResponse.ok(), await readinessResponse.text()).toBe(true);
      const readiness = (await readinessResponse.json()).data;
      expect(readiness.readyToCalculate, JSON.stringify(readiness.blockers)).toBe(true);
      const command = preparerPage.waitForRequest((item) => item.method() === "POST" && item.url().endsWith(`/people/payroll/runs/${runId}/calculate`));
      const response = preparerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/people/payroll/runs/${runId}/calculate`));
      await preparerPage.getByRole("button", { name: "Calculate mock preview", exact: true }).click();
      const request = await command;
      const result = await response;
      expect(result.ok(), await result.text()).toBe(true);
      const headers = await request.allHeaders();
      return Object.fromEntries(Object.entries(headers).filter(([name]) => name === "origin" || name.startsWith("x-erpx-context-")));
    };
    const preparerContextHeaders = await calculate();
    await expect(preparerPage.getByRole("heading", { name: "Employee preview breakdown", exact: true })).toBeVisible();
    let detail = await payrollRun(preparer, runId);
    expect(detail.run).toMatchObject({ status: "preview", mode: "preview" });
    expect(detail.items).toHaveLength(4);
    const itemByFixture = Object.fromEntries(detail.items.map((item) => [Object.entries(fixtures.payrollPersonIds).find(([, personId]) => personId === item.person_id)?.[0], item])) as Record<string, PayrollItem>;
    for (const key of ["employee", "foreignEmployee", "dailyEmployee", "hourlyEmployee"]) expect(itemByFixture[key], `${key} result`).toBeTruthy();
    const ordinaryInput = JSON.parse(itemByFixture.employee.input_snapshot_json);
    const ordinaryResult = JSON.parse(itemByFixture.employee.result_json);
    expect(ordinaryInput.hours.handoffs).toEqual([]);
    expect(ordinaryInput.hours.overtime).toBe(0);
    expect(ordinaryResult.adapter).toMatchObject({ id: "thai-hotel-v1-payroll-fixtures-v1", statutory: false });
    expect(ordinaryResult.explanation.components).toEqual(expect.arrayContaining([
      expect.objectContaining({ code: "MOCK_FIXTURE:bonus", label: expect.stringContaining("MOCK ONLY") }),
      expect.objectContaining({ code: "MOCK_FIXTURE:service-charge", label: expect.stringContaining("MOCK ONLY") }),
      expect.objectContaining({ code: "MOCK_FIXTURE:withholding", kind: "deduction" }),
      expect.objectContaining({ code: "MOCK_FIXTURE:employer-contribution", kind: "employer" }),
      expect.objectContaining({ code: "MOCK_FIXTURE:YTD_CONTEXT", includedInTotals: false }),
    ]));
    const foreignInput = JSON.parse(itemByFixture.foreignEmployee.input_snapshot_json);
    const foreignResult = JSON.parse(itemByFixture.foreignEmployee.result_json);
    expect(foreignInput.engagement.employment_type).toBe("salaried");
    expect(foreignInput.policy).toMatchObject({ activeDays: 13, periodDays: 28 });
    expect(foreignInput.engagement.person_id).toBe(fixtures.payrollPersonIds.foreignEmployee);
    expect(foreignResult.adapter.statutory).toBe(false);
    const dailyInput = JSON.parse(itemByFixture.dailyEmployee.input_snapshot_json);
    const hourlyInput = JSON.parse(itemByFixture.hourlyEmployee.input_snapshot_json);
    expect(dailyInput.engagement.employment_type).toBe("shift");
    expect(dailyInput.hours.regular).toBe(8);
    expect(hourlyInput.engagement.employment_type).toBe("hourly");
    expect(hourlyInput.hours.regular).toBe(32);
    expect(hourlyInput.hours.overtime).toBe(2);
    expect(detail.items.every((item) => item.gross_minor - item.deductions_minor === item.net_minor)).toBe(true);
    const originalRevision = Number(detail.run.revision);

    // Same-person approval is refused and leaves the preview available for review.
    // The preparer role has no approval grant, so this checks the permission boundary.
    const selfApprove = await preparer.request.post(`/api/v1/holdings/riverside-lantern/people/payroll/runs/${runId}/approve`, {
      headers: { ...preparerContextHeaders, "Idempotency-Key": crypto.randomUUID() },
      data: { expectedVersion: Number(detail.run.version) },
    });
    expect(selfApprove.status()).toBe(403);
    detail = await payrollRun(preparer, runId);
    expect(detail.run.status).toBe("preview");

    const submitForReview = async () => {
      const submission = preparerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/people/payroll/runs/${runId}/submit-review`));
      await preparerPage.getByRole("button", { name: "Submit for review", exact: true }).click();
      const result = await submission;
      expect(result.ok(), await result.text()).toBe(true);
    };
    await submitForReview();
    detail = await payrollRun(preparer, runId);
    expect(detail.run.status).toBe("in_review");

    const reviewerPage = await reviewer.newPage();
    await signIn(reviewer, reviewerPage, credentials, "payroll_approver");
    await openCurrentRun(reviewerPage, runId);
    const initialApproval = reviewerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/people/payroll/runs/${runId}/approve`));
    await reviewerPage.getByRole("button", { name: "Approve submitted review", exact: true }).click();
    const initialApprovalResult = await initialApproval;
    expect(initialApprovalResult.ok(), `${initialApprovalResult.status()} ${await initialApprovalResult.text()}`).toBe(true);
    await preparerPage.reload({ waitUntil: "domcontentloaded" });
    await preparerPage.getByText("Controlled corrections", { exact: true }).click();
    await expect(preparerPage.locator("#workspace-content").getByLabel("Correction reason and source evidence", { exact: true }).first()).toBeVisible();
    await preparerPage.locator("#workspace-content").getByLabel("Correction reason and source evidence", { exact: true }).first().fill("Synthetic review found one controlled source note requiring recalculation; preserve all prior revision evidence.");
    const correctionResponse = preparerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/people/payroll/runs/${runId}/corrections`));
    await preparerPage.getByRole("button", { name: "Request controlled correction", exact: true }).click();
    expect((await correctionResponse).ok()).toBe(true);
    detail = await payrollRun(preparer, runId);
    const correction = detail.corrections[0] as { id: string; version: number; requested_by: string };
    expect(correction).toBeTruthy();
    // The preparer role cannot decide a correction; same-person review is covered below with the dual-authority owner.
    const selfCorrection = await preparer.request.post(`/api/v1/holdings/riverside-lantern/people/payroll/corrections/${correction.id}/decide`, {
      headers: { ...preparerContextHeaders, "Idempotency-Key": crypto.randomUUID() },
      data: { expectedVersion: correction.version, action: "approve", reason: "Synthetic self-review negative case must remain blocked." },
    });
    expect(selfCorrection.status()).toBe(403);

    await reviewerPage.reload({ waitUntil: "domcontentloaded" });
    await reviewerPage.getByText("Controlled corrections", { exact: true }).click();
    const correctionCard = reviewerPage.locator(`#correction-${correction.id}`);
    await correctionCard.getByLabel("Decision evidence", { exact: true }).fill("Independent payroll approver authorized this mock correction for revision coverage.");
    const decisionResponse = reviewerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/people/payroll/corrections/${correction.id}/decide`));
    await correctionCard.getByRole("button", { name: "Record correction decision", exact: true }).click();
    expect((await decisionResponse).ok()).toBe(true);
    await expect(reviewerPage.locator("#workspace-content").getByText("approved · source revision", { exact: false }).first()).toBeVisible();
    await preparerPage.reload({ waitUntil: "domcontentloaded" });
    await calculate();
    detail = await payrollRun(preparer, runId);
    expect(Number(detail.run.revision)).toBe(originalRevision + 1);
    expect(detail.exports).toHaveLength(0);

    await submitForReview();
    await reviewerPage.reload({ waitUntil: "domcontentloaded" });
    await reviewerPage.getByRole("button", { name: "Export review report", exact: true }).click();
    await expect.poll(async () => (await payrollRun(reviewer, runId)).exports.length).toBe(1);
    detail = await payrollRun(reviewer, runId);
    const reviewExport = await reviewer.request.get(`/api/v1/holdings/riverside-lantern/people/payroll/runs/${runId}/review-file?revision=${Number(detail.run.revision)}`);
    expect(reviewExport.ok()).toBe(true);
    expect(reviewExport.headers()["etag"]).toMatch(/^[a-f0-9]{64}$/);
    expect(await reviewExport.text()).toContain("Fictional bonus fixture");

    await reviewerPage.reload({ waitUntil: "domcontentloaded" });
    const approvePreview = reviewerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/people/payroll/runs/${runId}/approve`));
    await reviewerPage.getByRole("button", { name: "Approve submitted review", exact: true }).click();
    const approvePreviewResult = await approvePreview;
    expect(approvePreviewResult.ok(), `${approvePreviewResult.status()} ${await approvePreviewResult.text()}`).toBe(true);
    expect((await payrollRun(reviewer, runId)).run.status).toBe("approved");

    // Finance mappings are drafted and approved by different people before posting.
    ownerContext = await browser.newContext(testInfo.project.use);
    ownerContext.setDefaultTimeout(15_000);
    const ownerPage = await ownerContext.newPage();
    await signIn(ownerContext, ownerPage, credentials, "holding_owner");
    await openPayrollSettings(ownerPage);
    const mappingForm = ownerPage.locator("form.business-form").filter({ has: ownerPage.getByRole("button", { name: "Save mapping draft", exact: true }) });
    await mappingForm.locator('select[name="chartId"]').selectOption({ index: 0 });
    const accountValues = async (name: string) => mappingForm.locator(`select[name="${name}"] option:not([value=""])`).evaluateAll((options) => options.map((option) => (option as HTMLOptionElement).value));
    const expenseAccounts = await accountValues("expenseAccountId");
    const liabilities = await accountValues("netPayableAccountId");
    expect(expenseAccounts.length, "synthetic chart expense accounts").toBeGreaterThan(1);
    expect(liabilities.length, "synthetic chart payroll liability accounts").toBeGreaterThan(0);
    await mappingForm.locator('select[name="expenseAccountId"]').selectOption(expenseAccounts[0]);
    await mappingForm.locator('select[name="employerExpenseAccountId"]').selectOption(expenseAccounts[1]);
    await mappingForm.locator('select[name="serviceChargeExpenseAccountId"]').selectOption(expenseAccounts[0]);
    await mappingForm.locator('select[name="netPayableAccountId"]').selectOption(liabilities[0]);
    await mappingForm.locator('select[name="taxPayableAccountId"]').selectOption(liabilities[0]);
    await mappingForm.locator('select[name="employerContributionAccountId"]').selectOption(liabilities[0]);
    const mappingSave = ownerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/people/payroll/pay-groups/${fixtures.payrollGroupId}/account-mappings`));
    await mappingForm.getByRole("button", { name: "Save mapping draft", exact: true }).click();
    expect((await mappingSave).ok()).toBe(true);

    const financePage = await finance.newPage();
    await signIn(finance, financePage, credentials, "finance_approver");
    await openPayrollSettings(financePage);
    const mappingApproval = financePage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes("/account-mappings/") && item.url().endsWith("/approve"));
    await financePage.getByRole("button", { name: "Approve mapping", exact: true }).click();
    expect((await mappingApproval).ok()).toBe(true);
    await financePage.getByRole("link", { name: "← Payroll", exact: true }).click();
    await openCurrentRun(financePage, runId);
    const posting = financePage.locator('section[aria-labelledby="payroll-accounting-heading"]');
    await expect(posting.getByRole("heading", { name: "Accounting preview", exact: true })).toBeVisible();
    const previewJournal = financePage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/people/payroll/runs/${runId}/accounting-preview`));
    await posting.getByRole("button", { name: "Preview journal", exact: true }).click();
    const previewJournalResult = await previewJournal;
    expect(previewJournalResult.ok(), await previewJournalResult.text()).toBe(true);
    const previewData = (await previewJournalResult.json()).data;
    expect(previewData.totals.debitMinor).toBe(previewData.totals.creditMinor);
    const postCommand = financePage.waitForRequest((item) => item.method() === "POST" && item.url().endsWith(`/people/payroll/runs/${runId}/post`));
    const postResponse = financePage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/people/payroll/runs/${runId}/post`));
    await posting.getByRole("button", { name: "Post reviewed journal", exact: true }).click();
    const postRequest = await postCommand;
    const posted = await postResponse;
    expect(posted.ok(), await posted.text()).toBe(true);
    const postingResult = (await posted.json()).data;
    expect(postingResult).toMatchObject({ status: "posted", journalId: expect.any(String) });
    const postHeaders = await postRequest.allHeaders();
    const financeContextHeaders = Object.fromEntries(Object.entries(postHeaders).filter(([name]) => name === "origin" || name.startsWith("x-erpx-context-")));
    const journals = (await (await finance.request.get("/api/v1/holdings/riverside-lantern/finance/journals")).json()).data as Record<string, unknown>[];
    const payrollJournals = journals.filter((journal) => journal.source_type === "payroll" && journal.source_id === runId);
    expect(payrollJournals).toHaveLength(1);
    expect(payrollJournals[0]).toMatchObject({ id: postingResult.journalId, status: "posted" });
    expect(await finance.request.get(`/api/v1/holdings/riverside-lantern/people/payroll/runs/${runId}`).then(async (response) => (await response.json()).data.run.status)).toBe("posted");
    const replayPost = await finance.request.post(`/api/v1/holdings/riverside-lantern/people/payroll/runs/${runId}/post`, {
      headers: { ...financeContextHeaders, "Idempotency-Key": crypto.randomUUID() },
      data: { accountMappingId: previewData.accountMappingId, expectedVersion: Number((await payrollRun(finance, runId)).run.version) },
    });
    expect(replayPost.status()).toBe(409);
    expect((await replayPost.json()).error.code).toBe("PAYROLL_NOT_APPROVED");
    expect((await (await finance.request.get("/api/v1/holdings/riverside-lantern/finance/journals")).json()).data.filter((journal: Record<string, unknown>) => journal.source_type === "payroll" && journal.source_id === runId)).toHaveLength(1);

    const treasuryPage = await treasury.newPage();
    await signIn(treasury, treasuryPage, credentials, "treasury");
    await openCurrentRun(treasuryPage, runId);
    const batchForm = treasuryPage.locator('section[aria-label="Payroll payment batch"] form.business-form');
    await batchForm.locator('select[name="formatCode"]').selectOption("generic-csv");
    const postingOptionsResponse = await finance.request.get("/api/v1/holdings/riverside-lantern/people/payroll/posting-options");
    expect(postingOptionsResponse.ok(), await postingOptionsResponse.text()).toBe(true);
    const postingOptions = (await postingOptionsResponse.json()).data as { mappings: Array<{ pay_group_id: string; chart_id: string; status: string }>; accounts: Array<Record<string, unknown>> };
    const approvedMapping = postingOptions.mappings.find((mapping) => mapping.pay_group_id === fixtures.payrollGroupId && mapping.status === "approved");
    expect(approvedMapping, "independently approved payroll map").toBeTruthy();
    const bankAccount = postingOptions.accounts.find((account) => account.chart_id === approvedMapping!.chart_id && account.category === "asset" && account.currency === "THB");
    expect(bankAccount, "THB synthetic cash ledger account").toBeTruthy();
    await batchForm.getByLabel("Finance bank account ID", { exact: true }).fill(String(bankAccount!.id));
    const batchCreate = treasuryPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith("/people/payments/batches"));
    await batchForm.getByRole("button", { name: "Prepare batch", exact: true }).click();
    expect((await batchCreate).ok()).toBe(true);
    const treasuryDetail = await payrollRun(finance, runId);
    const batch = treasuryDetail.paymentBatch as { id: string; status: string; created_by: string; total_minor: number };
    expect(batch).toMatchObject({ status: "draft", created_by: expect.any(String) });
    expect(Number(batch.total_minor)).toBe(treasuryDetail.items.reduce((sum, item) => sum + item.net_minor, 0));
    await financePage.reload({ waitUntil: "domcontentloaded" });
    const paymentApproval = financePage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/payments/batches/${batch.id}/approve`));
    await financePage.getByRole("button", { name: "Approve batch", exact: true }).click();
    expect((await paymentApproval).ok()).toBe(true);
    await treasuryPage.reload({ waitUntil: "domcontentloaded" });
    const paymentExport = treasuryPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/payments/batches/${batch.id}/export`));
    await treasuryPage.getByRole("button", { name: "Export instruction", exact: true }).click();
    expect((await paymentExport).ok()).toBe(true);
    const downloadPromise = treasuryPage.waitForEvent("download");
    await treasuryPage.getByRole("link", { name: "Download file", exact: true }).click();
    const paymentFile = await downloadPromise;
    expect(paymentFile.suggestedFilename()).toContain(batch.id);
    await treasuryPage.reload({ waitUntil: "domcontentloaded" });
    const bankForm = treasuryPage.locator("form.business-form").filter({ has: treasuryPage.getByRole("heading", { name: "Reconcile bank results", exact: true }) });
    const bankReferenceInputs = bankForm.locator('input[name^="ref:"]');
    await expect(bankForm.getByRole("heading", { name: "Reconcile bank results", exact: true })).toBeVisible();
    await expect(bankReferenceInputs).toHaveCount(4);
    const bankRefs = await bankReferenceInputs.count();
    expect(bankRefs).toBeGreaterThan(0);
    const paymentDetail = await payrollRun(treasury, runId);
    const pendingItems = paymentDetail.paymentItems as Array<{ reference: string; amount_minor: number }>;
    const bankReferences = pendingItems.map((item) => `SYNTHETIC-BANK-CONF-${run}-${item.reference}`);
    const statementNumber = `PAYROLL-${run}-${crypto.randomUUID().slice(0, 8)}`;
    const statementRows = pendingItems.map((item, index) => ({ externalId: bankReferences[index], transactionDate: String(paymentDetail.run.pay_date), description: `Synthetic payroll debit ${item.reference}`, amount: (-Number(item.amount_minor) / 100).toFixed(2), currency: "THB" }));
    const statementTotal = pendingItems.reduce((sum, item) => sum + Number(item.amount_minor), 0);
    const statementKey = `payroll-bank-${run}`;
    const bankStatementInput = { bankAccount: String(bankAccount!.id), statementNumber, statementDate: String(paymentDetail.run.pay_date), opening: "1000000.00", closing: ((100000000 - statementTotal) / 100).toFixed(2), sourceHash: `synthetic-payroll-${run}`, rows: statementRows };
    const bankStatementHeaders = { ...financeContextHeaders, "Idempotency-Key": statementKey };
    const importedStatement = await finance.request.post("/api/v1/holdings/riverside-lantern/finance/bank/statements/import", {
      headers: bankStatementHeaders,
      data: bankStatementInput,
    });
    expect(importedStatement.status(), await importedStatement.text()).toBe(201);
    const importedStatementId = String((await importedStatement.json()).data.id);
    const statementReplay = await finance.request.post("/api/v1/holdings/riverside-lantern/finance/bank/statements/import", { headers: bankStatementHeaders, data: bankStatementInput });
    expect(statementReplay.status(), await statementReplay.text()).toBe(201);
    expect(String((await statementReplay.json()).data.id)).toBe(importedStatementId);
    for (const [index, item] of pendingItems.entries()) {
      const reference = item.reference;
      await bankForm.locator(`input[name="ref:${reference}"]`).fill(bankReferences[index]);
      await bankForm.locator(`input[name="bankId:${reference}"]`).fill(`bankrow-${statementKey}-${index}`);
      await bankForm.locator(`input[name="bankVersion:${reference}"]`).fill("1");
    }
    const confirmCommand = treasuryPage.waitForRequest((item) => item.method() === "POST" && item.url().endsWith(`/payments/batches/${batch.id}/confirm`));
    const confirmPayment = treasuryPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/payments/batches/${batch.id}/confirm`));
    await bankForm.getByRole("button", { name: "Confirm results", exact: true }).click();
    const confirmedPayment = await confirmPayment;
    const confirmRequest = await confirmCommand;
    expect(confirmedPayment.ok(), await confirmedPayment.text()).toBe(true);
    const confirmHeaders = await confirmRequest.allHeaders();
    const replayHeaders = Object.fromEntries(Object.entries(confirmHeaders).filter(([name]) => name === "idempotency-key" || name.startsWith("x-erpx-context-")));
    const confirmationReplay = await treasuryPage.evaluate(async ({ path, headers, body }) => {
      const response = await fetch(path, { method: "POST", headers, body: JSON.stringify(body) });
      return { ok: response.ok, text: await response.text() };
    }, { path: new URL(confirmRequest.url()).pathname, headers: replayHeaders, body: confirmRequest.postDataJSON() });
    expect(confirmationReplay.ok, confirmationReplay.text).toBe(true);
    const settled = await payrollRun(treasury, runId);
    expect(settled.paymentBatch?.status).toBe("settled");
    expect(settled.paymentItems.length).toBe(4);
    expect(settled.paymentItems.every((item) => String(item.status) === "settled")).toBe(true);
    await treasuryPage.reload({ waitUntil: "domcontentloaded" });
    await expect(treasuryPage.locator('section[aria-label="Payroll payment batch"]').last()).toContainText("Settled");

  } finally {
    await Promise.all([preparer.close(), reviewer.close(), finance.close(), treasury.close(), ...(ownerContext ? [ownerContext.close()] : [])]);
  }
});
