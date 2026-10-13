import { captureInvestorCheckpoint } from "../../support/helpers/investor-capture";
import {createEngineeringBrowserContext} from "../../support/browser/engineering-browser-evidence";
import { customerDeliveryProofPdf } from "../../support/browser/customer-delivery-proof";
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; roles?: string[]; label: string; userId: string };
type Credentials = { password: string; users: DemoUser[] };
type DemoState = { fixture: { quotationId: string; periodId: string; closeBankAccountId: string; customerRefundBankTransactionId: string } };

async function signInRole(context: BrowserContext, page: Page, credentials: Credentials, role: string, user: DemoUser, path: string) {
  const login = await context.request.post("/api/auth/sign-in/email", { headers: { Origin: (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102") }, data: { email: user.email, password: credentials.password } });
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${user.label} ${role} assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, { headers: { Origin: (process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102") }, data: { assignmentId: assignment.id, revision: active.revision } });
  expect(selected.ok(), await selected.text()).toBe(true);
  const separator = path.includes("?") ? "&" : "?";
  await page.goto(`${path}${separator}refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
}

async function clickCommand(page: Page, endpoint: string, button: string) {
  await expect(page.getByRole("button", { name: button, exact: true })).toBeVisible();
  const responsePromise = page.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(endpoint));
  await page.getByRole("button", { name: button, exact: true }).click();
  expect((await responsePromise).status()).toBeLessThan(400);
  await page.waitForTimeout(1_500);
  for (let attempt = 0; attempt < 8; attempt += 1) {
    await page.reload({ waitUntil: "domcontentloaded" });
    await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
    if (!(await page.getByRole("button", { name: button, exact: true }).isVisible().catch(() => false))) return;
    await page.waitForTimeout(500);
  }
  await expect(page.getByRole("button", { name: button, exact: true })).not.toBeVisible();
}

async function waitForOrderState(page: Page, orderId: string, state: string) {
  for (let attempt = 0; attempt < 12; attempt += 1) {
    const response = await page.request.get(`/api/v1/holdings/simulation/commerce/orders/${orderId}`);
    if (response.ok()) {
      const body = await response.json() as { data?: { order?: { state?: string } } };
      if (body.data?.order?.state === state) return;
    }
    await page.waitForTimeout(500);
  }
  throw new Error(`Order ${orderId} did not reach ${state}.`);
}

test("isolated demo carries a quotation through sales delivery, customer invoice and collection", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(180_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const quotationId = state.fixture.quotationId;
  const salesOperator = credentials.users.find((user) => user.roles?.includes("sales_operator"));
  const salesManager = credentials.users.find((user) => user.roles?.includes("sales_manager"));
  const warehouse = credentials.users.find((user) => user.roles?.includes("warehouse_manager"));
  const financePreparer = credentials.users.find((user) => user.role === "finance_preparer");
  const financeApprover = credentials.users.find((user) => user.role === "finance_approver");
  const treasury = credentials.users.find((user) => user.roles?.includes("treasury"));
  const holdingOwner = credentials.users.find((user) => user.role === "holding_owner");
  for (const [label, user] of Object.entries({ salesOperator, salesManager, warehouse, financePreparer, financeApprover, treasury, holdingOwner })) expect(user, `${label} demo account`).toBeTruthy();
  const quotePath = `/holdings/simulation/commerce/orders/${quotationId}`;
  const salesContext = await createEngineeringBrowserContext(browser,testInfo);
  const managerContext = await createEngineeringBrowserContext(browser,testInfo);
  const warehouseContext = await createEngineeringBrowserContext(browser,testInfo);
  const preparerContext = await createEngineeringBrowserContext(browser,testInfo);
  const approverContext = await createEngineeringBrowserContext(browser,testInfo);
  const treasuryContext = await createEngineeringBrowserContext(browser,testInfo);
  const ownerContext = await createEngineeringBrowserContext(browser,testInfo);
  try {
    const salesPage = await salesContext.newPage();
    await signInRole(salesContext, salesPage, credentials, "sales_operator", salesOperator!, quotePath);
    await expect(salesPage.locator("#workspace-content:visible").getByText("draft", { exact: true }).first()).toBeVisible();
    await clickCommand(salesPage, `/commerce/orders/${quotationId}/transition`, "Submit for approval");

    const managerPage = await managerContext.newPage();
    await signInRole(managerContext, managerPage, credentials, "sales_manager", salesManager!, quotePath);
    await clickCommand(managerPage, `/commerce/orders/${quotationId}/transition`, "Approve order");

    await salesPage.goto(quotePath, { waitUntil: "domcontentloaded" });
    await expect(salesPage.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
    await expect(salesPage.locator("#workspace-content:visible").locator("h1").filter({ hasText: "QT-" }).first()).toBeVisible();
    await expect(salesPage.getByRole("button", { name: "Create sales order", exact: true })).toBeVisible();
    await captureInvestorCheckpoint(salesPage, "sales-quote", salesPage.locator("#workspace-content:visible h1").first());
    const convertRequest = salesPage.waitForRequest((item) => item.method() === "POST" && item.url().includes(`/commerce/orders/${quotationId}/transition`));
    await salesPage.getByRole("button", { name: "Create sales order", exact: true }).click();
    await convertRequest;
    await salesPage.waitForTimeout(1_500);
    await salesPage.waitForURL(/\/holdings\/simulation\/commerce\/orders\/order-/);
    const salesOrderPath = new URL(salesPage.url()).pathname;
    const salesOrderId = salesOrderPath.split("/").at(-1)!;
    await clickCommand(salesPage, `/commerce/orders/${salesOrderId}/transition`, "Submit for approval");

    await managerPage.goto(salesOrderPath, { waitUntil: "domcontentloaded" });
    await expect(managerPage.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
    await expect(managerPage.locator("#workspace-content:visible").locator("h1").filter({ hasText: "SO-" }).first()).toBeVisible();
    await expect(managerPage.getByRole("button", { name: "Approve order", exact: true })).toBeVisible();
    await clickCommand(managerPage, `/commerce/orders/${salesOrderId}/transition`, "Approve order");
    await waitForOrderState(managerPage, salesOrderId, "confirmed");

    const warehousePage = await warehouseContext.newPage();
    await signInRole(warehouseContext, warehousePage, credentials, "warehouse_manager", warehouse!, salesOrderPath);
    await expect(warehousePage.getByRole("heading", { name: "Deliver goods", exact: true })).toBeVisible();
    let deliveryCard = warehousePage.locator("#workspace-content:visible section.ui-panel:visible").filter({ hasText: "Deliver goods" });
    let deliveryForm = deliveryCard.locator("form");
    let proofPanel = deliveryForm.getByRole('region', { name: 'Proof of delivery', exact: true });
    await expect(proofPanel.getByRole('button', { name: 'Choose files', exact: true })).toBeEnabled();
    const proof = proofPanel.locator('input[type="file"]').first();
    const pdf = customerDeliveryProofPdf();
    await proof.setInputFiles({ name: "customer-delivery.pdf", mimeType: "application/pdf", buffer: pdf });
    await expect(proofPanel).toContainText('customer-delivery.pdf');
    await expect(proofPanel).toContainText(/1\/\d+ ready/, { timeout: 90_000 });
    // Confirm an uploaded proof survives a reload, then submit against the
    // freshly rendered order version so delayed workflow refreshes do not
    // leave the delivery form holding stale state.
    await warehousePage.reload({ waitUntil: "domcontentloaded" });
    await expect(warehousePage.getByRole("heading", { name: "Deliver goods", exact: true })).toBeVisible();
    deliveryCard = warehousePage.locator("#workspace-content:visible section.ui-panel:visible").filter({ hasText: "Deliver goods" });
    deliveryForm = deliveryCard.locator("form");
    proofPanel = deliveryForm.getByRole('region', { name: 'Proof of delivery', exact: true });
    await expect(proofPanel).toContainText('customer-delivery.pdf');
    await expect(proofPanel).toContainText(/1\/\d+ ready/);
    await deliveryForm.getByLabel("Receipt confirmation note", { exact: true }).fill("Customer signed delivery confirmation for one simulation item.");
    await deliveryForm.getByLabel("SIM-ITEM quantity", { exact: true }).fill("1");
    const deliveryResponse = warehousePage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/commerce/orders/${salesOrderId}/fulfill`));
    await deliveryForm.getByRole("button", { name: "Record delivery", exact: true }).click();
    const deliveryResult = await deliveryResponse;
    expect(deliveryResult.status(), await deliveryResult.text()).toBe(200);
    await warehousePage.waitForTimeout(1_500);
    for (let attempt = 0; attempt < 8; attempt += 1) {
      await warehousePage.reload({ waitUntil: "domcontentloaded" });
      await expect(warehousePage.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
      if (await warehousePage.locator("#workspace-content:visible").getByText("Delivery confirmation:", { exact: true }).isVisible().catch(() => false)) break;
      await warehousePage.waitForTimeout(500);
    }
    await expect(warehousePage.locator("#workspace-content:visible").getByText("Delivery confirmation:", { exact: true })).toBeVisible({ timeout: 20_000 });
    const approverPage = await approverContext.newPage();
    await signInRole(approverContext, approverPage, credentials, "finance_approver", financeApprover!, `${salesOrderPath}?refresh=${Date.now()}`);
    const journalLink = approverPage.getByRole("link", { name: "Review goods movement GL", exact: true }).first();
    await expect(journalLink).toBeVisible({ timeout: 20_000 });
    const journalPath = new URL(await journalLink.getAttribute("href") ?? "", approverPage.url()).pathname;

    await approverPage.goto(journalPath, { waitUntil: "domcontentloaded" });
    await expect(approverPage.getByRole("button", { name: "Approve and post journal", exact: true })).toBeVisible();
    await clickCommand(approverPage, "/finance/journals/", "Approve and post journal");

    const preparerPage = await preparerContext.newPage();
    await signInRole(preparerContext, preparerPage, credentials, "finance_preparer", financePreparer!, salesOrderPath);
    await preparerPage.locator("summary").filter({hasText:"Prepare invoice for GL review"}).click();
    await expect(preparerPage.getByRole("dialog",{name:"Prepare sales invoice"})).toBeVisible();
    await expect(preparerPage.getByRole("button", { name: "Prepare invoice for GL review", exact: true })).toBeVisible();
    const invoiceResponse = preparerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/commerce/fulfillments/`) && item.url().endsWith("/invoice"));
    await preparerPage.getByRole("button", { name: "Prepare invoice for GL review", exact: true }).click();
    const invoiceResult = await invoiceResponse;
    expect(invoiceResult.ok(), await invoiceResult.text()).toBe(true);
    const invoiceId = String((await invoiceResult.json()).data.invoiceId);
    const invoicePath = `/holdings/simulation/finance/invoices/${invoiceId}`;

    await signInRole(approverContext, approverPage, credentials, "finance_approver", financeApprover!, invoicePath);
    await expect(approverPage.getByRole("button", { name: "Approve and post invoice", exact: true })).toBeVisible();
    const invoicePosting = approverPage.locator('#workspace-content:visible form').filter({ has: approverPage.getByRole('button', { name: 'Approve and post invoice', exact: true }) });
    await invoicePosting.locator('select[name="period"]').selectOption(state.fixture.periodId);
    await clickCommand(approverPage, `/finance/invoices/${invoiceId}/post`, "Approve and post invoice");
    await expect(approverPage.locator("#workspace-content:visible").getByText("posted", { exact: true }).first()).toBeVisible();
    await captureInvestorCheckpoint(approverPage, "sales-invoice", approverPage.locator("#workspace-content:visible h1").first());

    const treasuryPage = await treasuryContext.newPage();
    await signInRole(treasuryContext, treasuryPage, credentials, "treasury", treasury!, invoicePath);
    await expect(treasuryPage.getByRole("heading", { name: "Record customer receipt", exact: true })).toBeVisible();
    const invoiceBeforeReceipt = await treasuryPage.request.get(`/api/v1/holdings/simulation/finance/invoices/${invoiceId}`);
    expect(invoiceBeforeReceipt.ok(), await invoiceBeforeReceipt.text()).toBe(true);
    const invoiceDate = String((await invoiceBeforeReceipt.json()).data.invoice.invoice_date);
    await treasuryPage.locator('#workspace-content:visible summary').filter({hasText:/^Record received payment$/}).click();
    const receiptDrawer=treasuryPage.getByRole('dialog',{name:'Record received payment',exact:true});await expect(receiptDrawer).toHaveCount(1);await expect(receiptDrawer).toBeVisible();
    const receiptForm=receiptDrawer.locator('form');await expect(receiptForm).toHaveCount(1);
    const receiptSubmit=receiptForm.getByRole('button',{name:'Record and allocate payment',exact:true});await expect(receiptSubmit).toHaveAttribute('data-client-managed-ready','true');
    await receiptForm.getByRole('combobox',{name:'Accounting period',exact:true}).selectOption(state.fixture.periodId);
    await receiptForm.getByRole('combobox',{name:'Received into bank account',exact:true}).selectOption(state.fixture.closeBankAccountId);
    await receiptForm.getByLabel("Payment date", { exact: true }).fill(invoiceDate);
    await receiptForm.getByLabel("Bank reference", { exact: true }).fill("DEMO-CUSTOMER-RECEIPT-10");
    const settleResponse = treasuryPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/commerce/invoices/${invoiceId}/settle`));
    await receiptSubmit.click();
    const settleResult = await settleResponse;
    expect(settleResult.ok(), await settleResult.text()).toBe(true);
    await treasuryPage.reload({ waitUntil: "domcontentloaded" });
    await expect(treasuryPage.locator("#workspace-content:visible").getByText("paid", { exact: true }).first()).toBeVisible();
    await expect(treasuryPage.locator("#workspace-content:visible").getByText("THB 0.00", { exact: true }).first()).toBeVisible();
    const invoiceCheck = await treasuryPage.request.get(`/api/v1/holdings/simulation/finance/invoices/${invoiceId}`);
    expect(invoiceCheck.ok(), await invoiceCheck.text()).toBe(true);
    expect((await invoiceCheck.json()).data.invoice).toMatchObject({ status: "paid", outstanding_minor: 0 });
    const summary = treasuryPage.getByRole("region", { name: "Invoice summary", exact: true });
    const outstanding = summary.locator("article").filter({ has: treasuryPage.getByText("Outstanding", { exact: true }) });
    await expect(outstanding.locator("strong")).toHaveText("THB 0.00");
    await expect.poll(() => outstanding.evaluate(card => {
      const label = card.querySelector(".eyebrow")!.getBoundingClientRect();
      const value = card.querySelector("strong")!.getBoundingClientRect();
      return value.top >= label.bottom + 4;
    }), { message: "Invoice summary labels must be separated from their values" }).toBe(true);
    await captureInvestorCheckpoint(treasuryPage, "sales-paid", testInfo.project.name === "mobile" ? outstanding : treasuryPage.locator("#workspace-content:visible h1").first());

    const issuedReceipt=treasuryPage.getByRole("link",{name:/^RC-\d{4}-\d{6,}$/}).first();await expect(issuedReceipt).toBeVisible();
    const receiptPath=new URL((await issuedReceipt.getAttribute("href"))!,treasuryPage.url()).pathname;
    await treasuryPage.goto(receiptPath,{waitUntil:"domcontentloaded"});
    await expect(treasuryPage.getByRole("heading",{name:"Receipt",exact:true})).toBeVisible();
    await expect(treasuryPage.getByRole("button",{name:"Print / Save as PDF",exact:true})).toBeVisible();
    const copy=treasuryPage.getByRole("link",{name:"Open retained issued copy",exact:true});
    const exportResponse=await treasuryPage.request.get((await copy.getAttribute("href"))!);expect(exportResponse.ok()).toBe(true);
    expect(exportResponse.headers()["x-document-sha256"]).toMatch(/^[a-f0-9]{64}$/);
    expect(await exportResponse.text()).toContain("DEMO-CUSTOMER-RECEIPT-10");
    await testInfo.attach("issued-customer-receipt",{body:await treasuryPage.screenshot({fullPage:true}),contentType:"image/png"});

    // Continue the same accepted sale through a customer return and controlled refund.
    await salesPage.goto(salesOrderPath, { waitUntil: "domcontentloaded" });
    await expect(salesPage.getByRole("button", { name: "Request return", exact: true })).toBeVisible();
    await salesPage.getByRole("button", { name: "Request return", exact: true }).click();
    const returnDrawer=salesPage.getByRole('dialog',{name:'Request a return',exact:true});await expect(returnDrawer).toHaveCount(1);await expect(returnDrawer).toBeVisible();
    const returnForm=returnDrawer.locator('form');await expect(returnForm).toHaveCount(1);
    const returnSubmit=returnForm.getByRole('button',{name:'Submit return for review',exact:true});await expect(returnSubmit).toHaveAttribute('data-client-managed-ready','true');
    await returnForm.getByLabel("Return quantity", { exact: true }).fill("1");
    await returnForm.getByLabel("Condition evidence", { exact: true }).fill("Customer reported a sealed unit returned for inspection.");
    const qualityReviewDue = returnForm.getByLabel("Quality review due", { exact: true });
    await qualityReviewDue.fill(
      (await qualityReviewDue.getAttribute("min")) ?? new Date().toISOString().slice(0, 10),
    );
    await returnForm.getByLabel("Return reason", { exact: true }).fill("Customer return for inspection and refund.");
    const returnRequest = salesPage.waitForResponse((item) => item.request().method() === "POST" && /\/commerce\/fulfillments\/[^/]+\/returns$/.test(new URL(item.url()).pathname));
    await returnSubmit.click();
    const returned=await returnRequest;expect(returned.ok(),await returned.text()).toBe(true);

    await managerPage.goto(salesOrderPath, { waitUntil: "domcontentloaded" });
    const returnReviewEvidence = managerPage.locator('#workspace-content:visible').getByLabel("Review evidence", { exact: true }).filter({ visible: true }).first();
    await expect(returnReviewEvidence).toBeVisible({ timeout: 20_000 });
    await returnReviewEvidence.fill("Return evidence reviewed and accepted for controlled credit.");
    const returnApprove = managerPage.waitForRequest((item) => item.method() === "POST" && /\/commerce\/returns\/[^/]+\/approve$/.test(new URL(item.url()).pathname));
    await managerPage.getByRole("button", { name: "Approve return", exact: true }).click();
    await returnApprove;
    await managerPage.waitForTimeout(1_500);

    // Returned customer stock is quarantined until independent Quality
    // inspection and release. Exercise that handoff before Finance posts the
    // return journals, using separate inspector and release contexts.
    await managerPage.goto(salesOrderPath, { waitUntil: "domcontentloaded" });
    const qualityLink = managerPage.getByRole("link", { name: /Quality disposition/ }).first();
    await expect(qualityLink).toBeVisible({ timeout: 20_000 });
    const qualityPath = new URL(await qualityLink.getAttribute("href") ?? "", managerPage.url()).pathname;
    await signInRole(preparerContext, preparerPage, credentials, "quality_inspector", financePreparer!, qualityPath);
    await expect(preparerPage.getByRole("heading", { name: "Record inspection", exact: true })).toBeVisible({ timeout: 20_000 });
    const inspectionForm = preparerPage.locator("#workspace-content:visible form").filter({ has: preparerPage.getByRole("heading", { name: "Record inspection", exact: true }) });
    await inspectionForm.locator('select[name="result"]').selectOption("pass");
    await inspectionForm.getByLabel("Inspection evidence", { exact: true }).fill("Returned unit inspected; packaging and function are acceptable for resale.");
    const inspectionResponse = preparerPage.waitForResponse((item) => item.request().method() === "POST" && /\/quality\/[^/]+\/inspect$/.test(new URL(item.url()).pathname));
    await inspectionForm.getByRole("button", { name: "Record inspection", exact: true }).click();
    const inspected = await inspectionResponse;
    expect(inspected.ok()).toBe(true);
    expect((await inspected.json()).data.status).toBe("awaiting_release");
    await preparerPage.reload({ waitUntil: "domcontentloaded" });
    const inspectionStatus = preparerPage.locator("#workspace-content:visible").getByRole("complementary", { name: "Quality workflow", exact: true }).getByRole("region", { name: "Status, owner and next action", exact: true }).locator('[data-slot="badge"]');
    await expect(inspectionStatus).toHaveCount(1);
    await expect(inspectionStatus.filter({ hasText: /^awaiting release$/ })).toBeVisible({ timeout: 20_000 });

    await signInRole(treasuryContext, treasuryPage, credentials, "quality_manager", treasury!, qualityPath);
    await expect(treasuryPage.getByRole("heading", { name: "Approve release", exact: true })).toBeVisible({ timeout: 20_000 });
    const releaseForm = treasuryPage.locator("#workspace-content:visible form").filter({ has: treasuryPage.getByRole("heading", { name: "Approve release", exact: true }) });
    await releaseForm.getByLabel("Release reason", { exact: true }).fill("Independent review confirms the returned unit is resalable.");
    const releaseResponse = treasuryPage.waitForResponse((item) => item.request().method() === "POST" && /\/quality\/[^/]+\/release$/.test(new URL(item.url()).pathname));
    await releaseForm.getByRole("button", { name: "Release this hold", exact: true }).click();
    const releaseResult = await releaseResponse;
    expect(releaseResult.ok()).toBe(true);
    expect((await releaseResult.json()).data.status).toBe("released");
    await treasuryPage.reload({ waitUntil: "domcontentloaded" });
    const releaseStatus = treasuryPage.locator("#workspace-content:visible").getByRole("complementary", { name: "Quality workflow", exact: true }).getByRole("region", { name: "Status, owner and next action", exact: true }).locator('[data-slot="badge"]');
    await expect(releaseStatus).toHaveCount(1);
    await expect(releaseStatus.filter({ hasText: /^released$/ })).toBeVisible({ timeout: 20_000 });

    await signInRole(approverContext, approverPage, credentials, "finance_approver", financeApprover!, `${salesOrderPath}?return=${Date.now()}`);
    const stockReversalLink = approverPage.getByRole("link", { name: /Stock reversal GL/ }).first();
    const creditNoteLink = approverPage.getByRole("link", { name: /Credit note GL/ }).first();
    await expect(stockReversalLink).toBeVisible({ timeout: 20_000 });
    await expect(creditNoteLink).toBeVisible({ timeout: 20_000 });
    const journalPaths = [
      new URL(await stockReversalLink.getAttribute("href") ?? "", approverPage.url()).pathname,
      new URL(await creditNoteLink.getAttribute("href") ?? "", approverPage.url()).pathname,
    ];
    for (const journalPath of journalPaths) {
      await approverPage.goto(journalPath, { waitUntil: "domcontentloaded" });
      await expect(approverPage.getByRole("button", { name: "Approve and post journal", exact: true })).toBeVisible();
      await clickCommand(approverPage, "/finance/journals/", "Approve and post journal");
    }

    await approverPage.goto(salesOrderPath, { waitUntil: "domcontentloaded" });
    await expect(approverPage.getByRole("button", { name: "Complete credit / debit note", exact: true })).toBeVisible({ timeout: 20_000 });
    const completeReturn = approverPage.waitForRequest((item) => item.method() === "POST" && /\/commerce\/returns\/[^/]+\/complete$/.test(new URL(item.url()).pathname));
    await approverPage.getByRole("button", { name: "Complete credit / debit note", exact: true }).click();
    await completeReturn;
    await approverPage.waitForTimeout(1_500);

    const refundLink = approverPage.getByRole("link", { name: "Review refund obligation", exact: true }).first();
    await expect(refundLink).toBeVisible({ timeout: 20_000 });
    const refundPath = new URL(await refundLink.getAttribute("href") ?? "", approverPage.url()).pathname;
    const ownerPage = await ownerContext.newPage();
    await signInRole(ownerContext, ownerPage, credentials, "holding_owner", holdingOwner!, refundPath);
    await expect(ownerPage.getByRole("heading", { name: "Independent approval", exact: true })).toBeVisible({ timeout: 20_000 });
    await ownerPage.locator('#workspace-content:visible').getByLabel("Verified counterparty bank account", { exact: true }).fill("SIM-CUSTOMER-BANK-001");
    await ownerPage.locator('#workspace-content:visible').getByLabel("Source and beneficiary review evidence", { exact: true }).fill("Customer identity and posted return credit reviewed independently.");
    const refundApprove = ownerPage.waitForResponse((item) => item.request().method() === "POST" && /\/finance\/refunds\/[^/]+\/approve$/.test(new URL(item.url()).pathname));
    await ownerPage.getByRole("button", { name: "Approve refund obligation", exact: true }).click();
    expect((await refundApprove).ok()).toBe(true);

    await signInRole(treasuryContext, treasuryPage, credentials, "treasury", treasury!, refundPath);
    await expect(treasuryPage.getByRole("heading", { name: "Prepare partial or final instruction", exact: true })).toBeVisible({ timeout: 20_000 });
    await treasuryPage.locator('#workspace-content:visible').getByLabel(/Amount \(THB\)/).fill("10.00");
    const plannedRefundDate = treasuryPage.locator('#workspace-content:visible').getByLabel("Planned payment date", { exact: true });
    await plannedRefundDate.fill(
      (await plannedRefundDate.getAttribute("min")) ?? new Date().toISOString().slice(0, 10),
    );
    await treasuryPage.locator("#workspace-content:visible").locator('select[name="cash"]').selectOption({ index: 1 });
    const refundPrepare = treasuryPage.waitForResponse((item) => item.request().method() === "POST" && /\/finance\/refunds\/[^/]+\/instructions$/.test(new URL(item.url()).pathname));
    await treasuryPage.getByRole("button", { name: "Reserve amount and prepare instruction", exact: true }).click();
    expect((await refundPrepare).ok()).toBe(true);
    await expect(treasuryPage.getByRole("button", { name: "Export prepared instructions", exact: true })).toBeVisible({ timeout: 20_000 });
    const refundExport = treasuryPage.waitForResponse((item) => item.request().method() === "POST" && /\/finance\/refund-instructions\/export$/.test(new URL(item.url()).pathname));
    await treasuryPage.getByRole("button", { name: "Export prepared instructions", exact: true }).click();
    expect((await refundExport).ok()).toBe(true);
    await treasuryPage.locator("#workspace-content:visible").locator('select[name="bank"]').selectOption(state.fixture.customerRefundBankTransactionId);
    const refundConfirmation = treasuryPage.locator('#workspace-content:visible form').filter({ has: treasuryPage.getByRole('button', { name: 'Confirm bank result and post refund', exact: true }) });
    await refundConfirmation.locator('select[name="period"]').selectOption(state.fixture.periodId);
    await treasuryPage.locator('#workspace-content:visible').getByLabel("Actual bank reference", { exact: true }).fill("DEMO-REFUND-10");
    await treasuryPage.locator('#workspace-content:visible').getByLabel("Verification evidence", { exact: true }).fill("Exact customer refund bank sandbox row matched to the exported instruction.");
    const refundConfirm = treasuryPage.waitForResponse((item) => item.request().method() === "POST" && /\/finance\/refund-instructions\/[^/]+\/confirm$/.test(new URL(item.url()).pathname));
    await treasuryPage.getByRole("button", { name: "Confirm bank result and post refund", exact: true }).click();
    expect((await refundConfirm).ok()).toBe(true);
    await treasuryPage.reload({ waitUntil: "domcontentloaded" });
    await expect(treasuryPage.locator("#workspace-content:visible").getByText(/Pay customer · settled/)).toBeVisible();
    const refundCheck = await treasuryPage.request.get(`/api/v1/holdings/simulation/finance/refunds/${refundPath.split("/").at(-1)}`);
    expect(refundCheck.ok(), await refundCheck.text()).toBe(true);
    expect((await refundCheck.json()).data.owner).toMatchObject({ status: "settled", outstanding_minor: 0 });
  } finally {
    await Promise.all([salesContext.close(), managerContext.close(), warehouseContext.close(), preparerContext.close(), approverContext.close(), treasuryContext.close(), ownerContext.close()]);
  }
});
