import { captureInvestorCheckpoint } from "../helpers/investor-capture";
import {createEngineeringBrowserContext} from './helpers/engineering-browser-evidence';
import { expect, test, type BrowserContext, type Locator, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; roles?: string[]; label: string; userId: string };
type Credentials = { password: string; users: DemoUser[] };
type DemoState = { fixture: { purchaseRequestId: string } };

async function signInRole(context: BrowserContext, page: Page, credentials: Credentials, role: string, user: DemoUser, path: string, waitUntil: "domcontentloaded" | "commit" = "domcontentloaded") {
  let login = await context.request.post("/api/auth/sign-in/email", { data: { email: user.email, password: credentials.password } });
  for (let attempt = 1; !login.ok() && attempt < 5; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, 1500));
    login = await context.request.post("/api/auth/sign-in/email", { data: { email: user.email, password: credentials.password } });
  }
  expect(login.ok()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  let active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${user.label} ${role} assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, { data: { assignmentId: assignment.id, revision: active.revision } });
  expect(selected.ok()).toBe(true);
  active = (await (await context.request.get(endpoint)).json()).data;
  expect(active.assignmentId).toBe(assignment.id);
  await page.goto(path, { waitUntil });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
}

async function requestCommand(page: Page, requestId: string, button: string, note: string) {
  const workspace = page.locator("#workspace-content:visible");
  let action: Locator;
  if (button === "Approve") {
    await expect(workspace.getByLabel("Message or decision evidence", { exact: true })).toBeEnabled();
    const decision = workspace.getByRole("region", { name: "Your decision", exact: true });
    await expect(decision).toHaveCount(1);
    await decision.getByRole("button", { name: "Approve", exact: true }).click();
    const confirmation = decision.locator("form");
    await expect(confirmation).toHaveCount(1);
    action = confirmation.getByRole("button", { name: "Confirm Approve", exact: true });
    await expect(action).toHaveAttribute("data-client-managed-ready", "true");
    await confirmation.getByLabel("Reason", { exact: true }).fill(note);
  } else {
    await workspace.getByLabel("Message or decision evidence", { exact: true }).fill(note);
    action = workspace.getByRole("button", { name: button, exact: true });
  }
  await expect(action).toHaveCount(1);
  await expect(action).toBeVisible();
  const response = page.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/api/v1/holdings/simulation/requests/${requestId}/actions`));
  await action.click();
  const result = await response;
  expect(result.ok(), await result.text()).toBe(true);
  await page.goto(`${page.url().split("?")[0]}?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
}

async function submitRequest(page: Page, requestId: string) {
  const editor = page.locator("#workspace-content:visible").locator("section").filter({ has: page.getByRole("heading", { name: "Request details", exact: true }) });
  await editor.getByRole("button", { name: "Review & submit", exact: true }).click();
  await expect(editor.getByRole("button", { name: "Confirm & submit", exact: true })).toBeEnabled();
  const response = page.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/api/v1/holdings/simulation/requests/${requestId}/actions`));
  await editor.getByRole("button", { name: "Confirm & submit", exact: true }).click();
  const result = await response;
  expect(result.ok(), await result.text()).toBe(true);
  await page.goto(`${page.url().split("?")[0]}?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
}

function actionSelect(page: Page) {
  return page.locator("#workspace-content:visible label").filter({ hasText: "Action" }).locator("select").first();
}

test("isolated demo carries an approved purchase request through split RFQs, receipts, one consolidated AP invoice and supplier payment", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(300_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const requestId = state.fixture.purchaseRequestId;
  const employee = credentials.users.find((user) => user.roles?.includes("employee"));
  const manager = credentials.users.find((user) => user.roles?.includes("people_manager"));
  const preparer = credentials.users.find((user) => user.role === "finance_preparer");
  const approver = credentials.users.find((user) => user.role === "finance_approver");
  const buyer = credentials.users.find((user) => user.roles?.includes("purchasing_operator"));
  const awardApprover = credentials.users.find((user) => user.label === "Support manager");
  for (const [label, user] of Object.entries({ employee, manager, preparer, approver, buyer, awardApprover })) expect(user, `${label} demo account`).toBeTruthy();
  const path = `/holdings/simulation/requests/${requestId}`;
  const employeeContext = await createEngineeringBrowserContext(browser,testInfo);
  const managerContext = await createEngineeringBrowserContext(browser,testInfo);
  const preparerContext = await createEngineeringBrowserContext(browser,testInfo);
  const approverContext = await createEngineeringBrowserContext(browser,testInfo);
  const buyerContext = await createEngineeringBrowserContext(browser,testInfo);
  const awardApproverContext = await createEngineeringBrowserContext(browser,testInfo);
  const warehouseContext = await createEngineeringBrowserContext(browser,testInfo);
  const treasuryContext = await createEngineeringBrowserContext(browser,testInfo);
  try {
    const employeePage = await employeeContext.newPage();
    await signInRole(employeeContext, employeePage, credentials, "employee", employee!, path);
    await submitRequest(employeePage, requestId);

    const managerPage = await managerContext.newPage();
    await signInRole(managerContext, managerPage, credentials, "people_manager", manager!, path);
    await requestCommand(managerPage, requestId, "Approve", "Line manager reviewed the business need and approved the request.");

    const preparerPage = await preparerContext.newPage();
    await signInRole(preparerContext, preparerPage, credentials, "finance_preparer", preparer!, path);
    await requestCommand(preparerPage, requestId, "Approve", "Finance preparer reviewed the planned spending and source evidence.");

    const approverPage = await approverContext.newPage();
    await signInRole(approverContext, approverPage, credentials, "finance_approver", approver!, path);
    await requestCommand(approverPage, requestId, "Approve", "Finance approver independently approved the planned spending.");

    const buyerPage = await buyerContext.newPage();
    await signInRole(buyerContext, buyerPage, credentials, "purchasing_operator", buyer!, path);
    await buyerPage.getByRole("combobox", { name: "Handler", exact: true }).selectOption(buyer!.userId);
    await requestCommand(buyerPage, requestId, "Assign handler", "Purchasing assigned the approved demand to the responsible buyer.");
    await requestCommand(buyerPage, requestId, "Start work", "Purchasing started sourcing against the approved request.");
    await buyerPage.goto(`/holdings/simulation/procurement-rfqs?request=${requestId}`, { waitUntil: "domcontentloaded" });
    await expect(buyerPage.getByRole("link", { name: /Approved requisition:/ }).first()).toBeVisible();
    await captureInvestorCheckpoint(buyerPage, "purchase-approved", buyerPage.getByRole("link", { name: /Approved requisition:/ }).first());
    await buyerPage.locator("#workspace-content:visible label").filter({ hasText: "Receiving warehouse" }).locator("select").selectOption({ index: 2 });
    const productSelect = buyerPage.locator("#workspace-content:visible label").filter({ hasText: "Demand product 1" }).locator("select");
    const productValue = await productSelect.locator("option").filter({ hasText: "SIM-LAP-EACH" }).getAttribute("value");
    expect(productValue).toBeTruthy();
    await productSelect.selectOption(productValue!);
    await buyerPage.locator("#workspace-content:visible").getByLabel(/Allocate request quantity/).fill("6");
    await buyerPage.locator("#workspace-content:visible").getByLabel(/Purchase quantity/).fill("6");
    await buyerPage.locator("#workspace-content:visible").getByLabel(/each per one/).fill("1");
    await buyerPage.getByRole("button", { name: "Create quotation request", exact: true }).click();
    await buyerPage.waitForURL(/\/procurement-rfqs\/rfq-/);
    const rfqId = new URL(buyerPage.url()).pathname.split("/").at(-1)!;
    await buyerPage.locator("#workspace-content:visible").getByLabel("Supplier", { exact: false }).selectOption({ index: 1 });
    await buyerPage.locator("#workspace-content:visible").getByLabel("Quotation reference", { exact: true }).fill("DEMO-REQUEST-RFQ-6");
    await buyerPage.locator("#workspace-content:visible").getByLabel("Quoted arrival", { exact: true }).fill("2026-10-10");
    await buyerPage.locator("#workspace-content:visible").getByLabel("Quote valid through", { exact: true }).fill("2026-10-20");
    await buyerPage.locator("#workspace-content:visible").getByLabel("Quotation evidence and terms", { exact: true }).fill("Supplier quotation for six of the ten approved laptops.");
    await buyerPage.locator("#workspace-content:visible").locator('input[name^="price-"]').fill("25000");
    const offerResponse = buyerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/api/v1/holdings/simulation/procurement-rfqs/${rfqId}`));
    await buyerPage.getByRole("button", { name: "Record quotation", exact: true }).click();
    expect((await offerResponse).ok()).toBe(true);
    await buyerPage.reload({ waitUntil: "domcontentloaded" });
    await expect(buyerPage.locator("#workspace-content:visible").locator("td").filter({ hasText: "DEMO-REQUEST-RFQ-6" }).first()).toBeVisible();
    await actionSelect(buyerPage).selectOption("submit");
    await buyerPage.locator("#workspace-content:visible").getByLabel("Decision and delivery exception reason", { exact: true }).first().fill("Awarded six units as a partial allocation of the approved request.");
    const submitResponse = buyerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/api/v1/holdings/simulation/procurement-rfqs/${rfqId}`));
    await buyerPage.getByRole("button", { name: "Record decision", exact: true }).click();
    expect((await submitResponse).ok()).toBe(true);
    await buyerPage.reload({ waitUntil: "domcontentloaded" });

    const awardPage = await awardApproverContext.newPage();
    await signInRole(awardApproverContext, awardPage, credentials, "purchasing_manager", awardApprover!, `/holdings/simulation/procurement-rfqs/${rfqId}`);
    await actionSelect(awardPage).selectOption("approve");
    await awardPage.locator("#workspace-content:visible").getByLabel("Decision and delivery exception reason", { exact: true }).first().fill("Independent purchasing manager approved the partial supplier award.");
    const approveResponse = awardPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/api/v1/holdings/simulation/procurement-rfqs/${rfqId}`));
    await awardPage.getByRole("button", { name: "Record decision", exact: true }).click();
    expect((await approveResponse).ok()).toBe(true);
    await awardPage.reload({ waitUntil: "domcontentloaded" });
    await expect(awardPage.locator("#workspace-content:visible").locator("p").filter({ hasText: "Required" }).first()).toContainText("approved");

    // Allocate the remaining four units as a second independently reviewed RFQ/PO.
    await buyerPage.goto(`/holdings/simulation/procurement-rfqs?request=${requestId}&refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
    await expect(buyerPage.getByRole("link", { name: /Approved requisition:/ }).first()).toBeVisible();
    await expect(buyerPage.locator("#workspace-content:visible").getByText("Already allocated", { exact: true }).first()).toBeVisible();
    await buyerPage.locator("#workspace-content:visible label").filter({ hasText: "Receiving warehouse" }).locator("select").selectOption({ index: 2 });
    const secondProductSelect = buyerPage.locator("#workspace-content:visible label").filter({ hasText: "Demand product 1" }).locator("select");
    const secondProductValue = await secondProductSelect.locator("option").filter({ hasText: "SIM-LAP-EACH" }).getAttribute("value");
    expect(secondProductValue).toBeTruthy();
    await secondProductSelect.selectOption(secondProductValue!);
    await buyerPage.locator("#workspace-content:visible").getByLabel(/Allocate request quantity/).fill("4");
    await buyerPage.locator("#workspace-content:visible").getByLabel(/Purchase quantity/).fill("4");
    await buyerPage.locator("#workspace-content:visible").getByLabel(/each per one/).fill("1");
    await buyerPage.getByRole("button", { name: "Create quotation request", exact: true }).click();
    await buyerPage.waitForURL(/\/procurement-rfqs\/rfq-/);
    const secondRfqId = new URL(buyerPage.url()).pathname.split("/").at(-1)!;
    await buyerPage.locator("#workspace-content:visible").getByLabel("Supplier", { exact: false }).selectOption({ index: 1 });
    await buyerPage.locator("#workspace-content:visible").getByLabel("Quotation reference", { exact: true }).fill("DEMO-REQUEST-RFQ-4");
    await buyerPage.locator("#workspace-content:visible").getByLabel("Quoted arrival", { exact: true }).fill("2026-10-10");
    await buyerPage.locator("#workspace-content:visible").getByLabel("Quote valid through", { exact: true }).fill("2026-10-20");
    await buyerPage.locator("#workspace-content:visible").getByLabel("Quotation evidence and terms", { exact: true }).fill("Supplier quotation for the remaining four of the ten approved laptops.");
    await buyerPage.locator("#workspace-content:visible").locator('input[name^="price-"]').fill("25000");
    const secondOfferResponse = buyerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/api/v1/holdings/simulation/procurement-rfqs/${secondRfqId}`));
    await buyerPage.getByRole("button", { name: "Record quotation", exact: true }).click();
    expect((await secondOfferResponse).ok()).toBe(true);
    await buyerPage.reload({ waitUntil: "domcontentloaded" });
    await expect(buyerPage.locator("#workspace-content:visible").locator("td").filter({ hasText: "DEMO-REQUEST-RFQ-4" }).first()).toBeVisible();
    await actionSelect(buyerPage).selectOption("submit");
    await buyerPage.locator("#workspace-content:visible").getByLabel("Decision and delivery exception reason", { exact: true }).first().fill("Awarded the remaining four units as the second allocation of the approved request.");
    const secondSubmitResponse = buyerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/api/v1/holdings/simulation/procurement-rfqs/${secondRfqId}`));
    await buyerPage.getByRole("button", { name: "Record decision", exact: true }).click();
    expect((await secondSubmitResponse).ok()).toBe(true);
    await buyerPage.reload({ waitUntil: "domcontentloaded" });
    await awardPage.goto(`/holdings/simulation/procurement-rfqs/${secondRfqId}`, { waitUntil: "domcontentloaded" });
    await actionSelect(awardPage).selectOption("approve");
    await awardPage.locator("#workspace-content:visible").getByLabel("Decision and delivery exception reason", { exact: true }).first().fill("Independent purchasing manager approved the remaining supplier award.");
    const secondApproveResponse = awardPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/api/v1/holdings/simulation/procurement-rfqs/${secondRfqId}`));
    await awardPage.getByRole("button", { name: "Record decision", exact: true }).click();
    expect((await secondApproveResponse).ok()).toBe(true);
    await awardPage.reload({ waitUntil: "domcontentloaded" });
    await expect(awardPage.locator("#workspace-content:visible").locator("p").filter({ hasText: "Required" }).first()).toContainText("approved");

    const createApprovedOrder = async (rfqPath: string) => {
      await buyerPage.goto(`${rfqPath}?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
      await expect(buyerPage.getByRole("heading", { name: "Create purchase order", exact: true })).toBeVisible();
      await buyerPage.locator("#workspace-content:visible").getByLabel("Purchase order date", { exact: true }).first().fill("2026-09-22");
      await Promise.all([buyerPage.waitForURL(/\/commerce\/orders\//), buyerPage.getByRole("button", { name: "Create awarded draft PO", exact: true }).click()]);
      const orderPath = new URL(buyerPage.url()).pathname;
      const orderSubmit = buyerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes("/api/v1/holdings/simulation/commerce/orders/") && item.url().endsWith("/transition"));
      await buyerPage.getByRole("button", { name: "Submit for approval", exact: true }).click();
      expect((await orderSubmit).ok()).toBe(true);
      await awardPage.goto(orderPath, { waitUntil: "domcontentloaded" });
      await expect(awardPage.getByRole("button", { name: "Approve order", exact: true })).toBeVisible();
      const orderApproval = awardPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes("/api/v1/holdings/simulation/commerce/orders/") && item.url().endsWith("/transition"));
      await awardPage.getByRole("button", { name: "Approve order", exact: true }).click();
      expect((await orderApproval).ok()).toBe(true);
      return orderPath;
    };
    const purchaseOrderPath = await createApprovedOrder(`/holdings/simulation/procurement-rfqs/${rfqId}`);
    const secondPurchaseOrderPath = await createApprovedOrder(`/holdings/simulation/procurement-rfqs/${secondRfqId}`);

    const warehousePage = await warehouseContext.newPage();
    await signInRole(warehouseContext, warehousePage, credentials, "warehouse_manager", manager!, `${purchaseOrderPath}?refresh=${Date.now()}`);
    await expect(warehousePage.getByRole("heading", { name: "Receive goods", exact: true }).first()).toBeVisible();
    await warehousePage.locator("#workspace-content:visible").getByLabel("Movement date", { exact: true }).fill("2026-09-22");
    await warehousePage.locator("#workspace-content:visible").getByLabel("SIM-LAP-EACH quantity", { exact: true }).first().fill("6");
    const firstReceipt = warehousePage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes("/api/v1/holdings/simulation/commerce/orders/") && item.url().endsWith("/fulfill"));
    await warehousePage.getByRole("button", { name: "Record receipt", exact: true }).click();
    expect((await firstReceipt).ok()).toBe(true);
    await warehousePage.goto(`${secondPurchaseOrderPath}?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
    await expect(warehousePage.getByRole("heading", { name: "Receive goods", exact: true }).first()).toBeVisible();
    await warehousePage.locator("#workspace-content:visible").getByLabel("Movement date", { exact: true }).fill("2026-09-22");
    await warehousePage.locator("#workspace-content:visible").getByLabel("SIM-LAP-EACH quantity", { exact: true }).first().fill("4");
    const secondReceipt = warehousePage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes("/api/v1/holdings/simulation/commerce/orders/") && item.url().endsWith("/fulfill"));
    await warehousePage.getByRole("button", { name: "Record receipt", exact: true }).click();
    expect((await secondReceipt).ok()).toBe(true);

    for (const orderPath of [purchaseOrderPath, secondPurchaseOrderPath]) {
      await approverPage.goto(`${orderPath}?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
      const goodsJournalLinks = approverPage.getByRole("link", { name: "Review goods movement GL", exact: true });
      await expect(goodsJournalLinks).toHaveCount(1, { timeout: 20_000 });
      const goodsJournalPath = await goodsJournalLinks.first().getAttribute("href");
      expect(goodsJournalPath).toBeTruthy();
      await approverPage.goto(goodsJournalPath!, { waitUntil: "domcontentloaded" });
      await expect(approverPage.getByRole("button", { name: "Approve and post journal", exact: true })).toBeVisible();
      const goodsJournalPost = approverPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes("/api/v1/holdings/simulation/finance/journals/") && item.url().endsWith("/post"));
      await approverPage.getByRole("button", { name: "Approve and post journal", exact: true }).click();
      expect((await goodsJournalPost).ok()).toBe(true);
    }

    const createAndPostBill = async () => {
      await preparerPage.goto(`/holdings/simulation/supplier-bills?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
      await preparerPage.locator("#workspace-content:visible").getByText(/Match one supplier invoice to/, { exact: false }).first().click();
      const billDrawer = preparerPage.getByRole("dialog", { name: "Match one supplier invoice to one or more receipts", exact: true });
      await expect(billDrawer).toHaveCount(1);
      await expect(billDrawer).toBeVisible();
      const billForm = billDrawer.locator("form");
      await expect(billForm).toHaveCount(1);
      const sendInvoice = billForm.getByRole("button", { name: "Send invoice for independent matching review", exact: true });
      await expect(sendInvoice).toHaveAttribute("data-client-managed-ready", "true");
      await billForm.getByLabel("Supplier invoice reference", { exact: true }).fill("DEMO-REQUEST-INVOICE-CONSOLIDATED");
      await billForm.getByLabel("Invoice date", { exact: true }).fill("2026-09-22");
      await billForm.getByLabel("Payment due date", { exact: true }).fill("2026-10-22");
      const billQuantities = billForm.locator('input[name^="quantity-"]');
      await expect(billQuantities).toHaveCount(2);
      // The capture form is keyed by receipt-line id and may order lines by
      // database id. Its defaults are the exact posted receipt quantities and
      // unit prices, so keep those values instead of assuming PO order.
      expect((await billQuantities.evaluateAll((inputs) => inputs.map((input) => (input as HTMLInputElement).value).sort()))).toEqual(["4", "6"]);
      await captureInvestorCheckpoint(preparerPage, "purchase-receipts", billDrawer);
      await billForm.getByLabel("Supplier invoice evidence", { exact: true }).fill("One supplier invoice covers the six-unit and four-unit receipts from the approved request.");
      const billResponse = preparerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith("/api/v1/holdings/simulation/supplier-bills"));
      await sendInvoice.click();
      const billResult = await billResponse;
      expect(billResult.ok(), await billResult.text()).toBe(true);
      const billId = String((await billResult.json()).data.id);
      await approverPage.goto(`/holdings/simulation/supplier-bills/${billId}`, { waitUntil: "domcontentloaded" });
      await expect(approverPage.getByRole("button", { name: "Record review", exact: true })).toBeVisible();
      await approverPage.locator("#workspace-content:visible").getByLabel("Review reason", { exact: true }).first().fill("Finance independently matched the posted receipt and the supplier invoice.");
      const billReview = approverPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/api/v1/holdings/simulation/supplier-bills/${billId}`));
      await approverPage.getByRole("button", { name: "Record review", exact: true }).click();
      expect((await billReview).ok()).toBe(true);
      await preparerPage.goto(`/holdings/simulation/supplier-bills/${billId}?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
      await expect(preparerPage.getByRole("button", { name: "Prepare matched AP invoice", exact: true })).toBeVisible();
      const invoiceResponse = preparerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes("/api/v1/holdings/simulation/commerce/fulfillments/") && item.url().endsWith("/invoice"));
      await preparerPage.getByRole("button", { name: "Prepare matched AP invoice", exact: true }).click();
      const invoiceResult = await invoiceResponse;
      expect(invoiceResult.ok(), await invoiceResult.text()).toBe(true);
      const invoiceId = String((await invoiceResult.json()).data.invoiceId);
      const invoicePath = `/holdings/simulation/finance/invoices/${invoiceId}`;
      await approverPage.goto(invoicePath, { waitUntil: "domcontentloaded" });
      await expect(approverPage.getByRole("button", { name: "Approve and post invoice", exact: true })).toBeVisible();
      const invoicePosting = approverPage.locator("#workspace-content:visible").locator('form').filter({ has: approverPage.getByRole('button', { name: 'Approve and post invoice', exact: true }) });
      await invoicePosting.locator('select[name="period"]').selectOption({ label: "2026" });
      const invoicePost = approverPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/api/v1/holdings/simulation/finance/invoices/${invoiceId}/post`));
      await approverPage.getByRole("button", { name: "Approve and post invoice", exact: true }).click();
      const invoicePostResult = await invoicePost;
      expect(invoicePostResult.ok(), await invoicePostResult.text()).toBe(true);
      await approverPage.reload({ waitUntil: "domcontentloaded" });
      await expect(approverPage.locator("#workspace-content:visible").getByText("posted", { exact: true }).first()).toBeVisible();
      return invoiceId;
    };

    const invoiceId = await createAndPostBill();
    // Continue the same source-linked obligation through the controlled supplier payment path.
    await preparerPage.goto("/holdings/simulation/finance/supplier-batches", { waitUntil: "domcontentloaded" });
    await preparerPage.locator("#workspace-content:visible").getByRole("link", { name: "Prepare batch", exact: true }).click();
    await expect(preparerPage.getByRole("heading", { name: "Prepare supplier instructions", exact: true })).toBeVisible();
    const firstInvoiceRow = preparerPage.locator("#workspace-content:visible").locator("fieldset").filter({ hasText: "Available THB 250000.00" }).first();
    await expect(firstInvoiceRow).toBeVisible();
    await firstInvoiceRow.getByRole("checkbox").check();
    const batchBank = preparerPage.locator('#workspace-content:visible select[name="bank"]');
    await expect(batchBank).toHaveCount(1);
    await batchBank.selectOption({ index: 1 });
    const supplierBankAccountId = await batchBank.inputValue();
    expect(supplierBankAccountId).toBeTruthy();
    await preparerPage.locator("#workspace-content:visible").getByLabel("Planned payment date", { exact: true }).fill("2026-10-01");
    await firstInvoiceRow.getByLabel("Verified beneficiary account", { exact: true }).fill("SANDBOX-ACCOUNT-001");
    const batchCreate = preparerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith("/api/v1/holdings/simulation/finance/supplier-batches"));
    await preparerPage.getByRole("button", { name: "Reserve selected amounts", exact: true }).click();
    const batchResult = await batchCreate;
    expect(batchResult.ok(), await batchResult.text()).toBe(true);
    await preparerPage.waitForURL(/\/finance\/supplier-batches\/supplier-batch-/);
    const batchPath = new URL(preparerPage.url()).pathname;

    await approverPage.goto(batchPath, { waitUntil: "domcontentloaded" });
    await expect(approverPage.getByRole("heading", { name: "Review the batch", exact: true })).toBeVisible();
    await approverPage.locator("#workspace-content:visible").getByLabel("Review evidence or cancellation reason", { exact: true }).first().fill("Finance independently reviewed the posted AP invoice, amount and beneficiary.");
    const batchApproval = approverPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith("/approve"));
    await approverPage.getByRole("button", { name: "Record decision", exact: true }).click();
    expect((await batchApproval).ok()).toBe(true);
    await expect(approverPage.locator("#workspace-content:visible").getByText(/2026-10-01 · THB · approved/)).toBeVisible();

    const treasuryPage = await treasuryContext.newPage();
    const treasury = credentials.users.find((user) => user.role === "treasury" || user.roles?.includes("treasury"));
    expect(treasury, "treasury demo account").toBeTruthy();
    await signInRole(treasuryContext, treasuryPage, credentials, "treasury", treasury!, batchPath);
    await expect(treasuryPage.getByRole("button", { name: "Export approved instructions", exact: true })).toBeVisible();
    const batchExport = treasuryPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith("/export"));
    await treasuryPage.getByRole("button", { name: "Export approved instructions", exact: true }).click();
    expect((await batchExport).ok()).toBe(true);
    await expect(treasuryPage.locator("#workspace-content:visible").getByText("Export SHA-256", { exact: false })).toBeVisible();

    await preparerPage.goto("/holdings/simulation/finance/bank?new=1", { waitUntil: "domcontentloaded" });
    await expect(preparerPage.getByRole("heading", { name: "Import bank statement", exact: true })).toBeVisible();
    // Import the debit into the exact ledger selected for this batch. The
    // bank workspace also lists other Company assets, in a different order.
    const bankImportAccount = preparerPage.locator('#workspace-content:visible select[name="bankAccount"]:visible');
    await expect(bankImportAccount).toHaveCount(1);
    await bankImportAccount.selectOption(supplierBankAccountId);
    await preparerPage.locator("#workspace-content:visible").getByLabel("Statement number", { exact: true }).fill("DEMO-SUPPLIER-PAY-2026");
    await preparerPage.locator("#workspace-content:visible").getByLabel("Statement date", { exact: true }).fill("2026-10-02");
    await preparerPage.locator("#workspace-content:visible").getByLabel("Opening balance", { exact: true }).fill("250000");
    await preparerPage.locator("#workspace-content:visible").getByLabel("Closing balance", { exact: true }).fill("0");
    await preparerPage.locator("#workspace-content:visible").getByLabel("Bank reference 1", { exact: true }).fill("DEMO-BANK-SUPPLIER-250000");
    await preparerPage.locator("#workspace-content:visible").getByLabel("Transaction date 1", { exact: true }).fill("2026-10-02");
    await preparerPage.locator("#workspace-content:visible").getByLabel("Description 1", { exact: true }).fill("Consolidated supplier payment for both approved laptop receipts");
    await preparerPage.locator("#workspace-content:visible").getByLabel("Signed amount 1", { exact: true }).fill("-250000");
    const bankImport = preparerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith("/api/v1/holdings/simulation/finance/bank/statements/import"));
    await preparerPage.getByRole("button", { name: "Validate and import", exact: true }).click();
    expect((await bankImport).ok()).toBe(true);

    await treasuryPage.goto(`${batchPath}?refresh=${Date.now()}`, { waitUntil: "commit" });
    const instructionCards = treasuryPage.locator("#workspace-content:visible section.ui-panel:visible").filter({ hasText: /BILL-/ });
    await expect(instructionCards).toHaveCount(1, { timeout: 20_000 });
    const instructionCard = instructionCards.first();
    await instructionCard.locator('select[name="bank"]').selectOption({ index: 1 });
    await instructionCard.locator('select[name="period"]').selectOption({ label: "2026" });
    await instructionCard.locator('input[name="reference"]').fill("DEMO-BANK-SUPPLIER-250000");
    await instructionCard.locator('textarea[name="evidence"]').first().fill("Exact sandbox bank debit matched to the exported consolidated supplier instruction.");
    const settle = treasuryPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes("/supplier-instructions/") && item.url().endsWith("/confirm"));
    await instructionCard.getByRole("button", { name: "Confirm bank debit and settle invoice", exact: true }).click();
    expect((await settle).ok()).toBe(true);
    await treasuryPage.reload({ waitUntil: "commit" });
    await expect(treasuryPage.locator("#workspace-content:visible").getByText(/· confirmed$/)).toHaveCount(1);

    const settledInvoice = await treasuryContext.request.get(`/api/v1/holdings/simulation/finance/invoices/${invoiceId}`);
    expect(settledInvoice.ok()).toBe(true);
    expect((await settledInvoice.json()).data.invoice).toMatchObject({ status: "paid", outstanding_minor: 0 });
    await captureInvestorCheckpoint(treasuryPage, "purchase-paid", treasuryPage.locator("#workspace-content:visible h1").first());

    const detail = await buyerContext.request.get(`/api/v1/holdings/simulation/procurement-rfqs/${rfqId}`);
    expect(detail.ok()).toBe(true);
    const rfq = (await detail.json()).data.rfq;
    expect(rfq).toMatchObject({ status: "ordered", request_id: requestId });
  } finally {
    await Promise.all([employeeContext.close(), managerContext.close(), preparerContext.close(), approverContext.close(), buyerContext.close(), awardApproverContext.close(), warehouseContext.close(), treasuryContext.close()]);
  }
});
