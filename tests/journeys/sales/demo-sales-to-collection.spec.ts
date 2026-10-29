import { captureInvestorCheckpoint } from "../../support/helpers/investor-capture";
import {createEngineeringBrowserContext} from "../../support/browser/engineering-browser-evidence";
import { customerDeliveryProofPdf } from "../../support/browser/customer-delivery-proof";
import { expect, test, type APIRequestContext, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { randomUUID } from "node:crypto";

type DemoUser = { email: string; role: string; roles?: string[]; label: string; userId: string };
type Credentials = { password: string; users: DemoUser[] };
type DemoState = { fixture: { quotationId: string; periodId: string; closeBankAccountId: string; customerRefundBankTransactionId: string; customerId: string; sourceWarehouseId: string; productId: string } };
type SalesOrderSnapshot = {
  order: { id: string; number: string; version: number; state: string; party_id: string; warehouse_id: string; approved_by: string | null };
  lines: Array<{ id: string; product_id: string; quantity_milli: number; price_minor: number }>;
  fulfillments: Array<{ id: string }>;
  history: Array<{ action: string; actor_id: string; record_version: number }>;
  fulfillmentEvidence: Array<{ fulfillment_id: string; attachment_id: string; checksum_sha256: string }>;
  pendingDeliveryEvidence: unknown[];
};
type DeliveryEvidenceSet = {
  id: string;
  targetType: string;
  targetId: string;
  purposeKey: string;
  expectedTargetVersion: number;
  state: "draft" | "bound" | "withdrawn" | "expired";
  links: Array<{
    id: string;
    sourceId: string | null;
    filename: string;
    mediaType: string;
    bytes: number | null;
    sourceState: string | null;
    linkState: string;
    boundAt: number | null;
  }>;
};

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

async function createSalesOrder(page: Page, state: DemoState, note: string) {
  await page.goto("/holdings/simulation/customers-sales/sales/documents/sales?new=1", { waitUntil: "domcontentloaded" });
  const drawer = page.getByRole("dialog", { name: "New sales order", exact: true });
  await expect(drawer).toBeVisible();
  const form = drawer.locator("form");
  const customer = form.getByRole("combobox", { name: "Customer", exact: true });
  await expect(customer).toHaveAccessibleName("Customer");
  await expect(customer).toBeEnabled();
  await expect(customer.locator(`option[value="${state.fixture.customerId}"]`)).toHaveCount(1);
  await customer.selectOption(state.fixture.customerId, { timeout: 15_000 });
  await expect(customer).toHaveValue(state.fixture.customerId);
  await form.getByRole("button", { name: "Add item", exact: true }).click();
  await form.getByRole("combobox", { name: "Warehouse", exact: true }).selectOption(state.fixture.sourceWarehouseId);
  await form.getByRole("combobox", { name: "Product 1", exact: true }).selectOption(state.fixture.productId);
  await form.getByRole("spinbutton", { name: "Quantity 1", exact: true }).fill("1");
  await form.getByRole("textbox", { name: "Order notes", exact: true }).fill(note);
  const response = page.waitForResponse((item) => item.request().method() === "POST" && new URL(item.url()).pathname === "/api/v1/holdings/simulation/commerce/orders");
  await form.getByRole("button", { name: "Save draft", exact: true }).click();
  const result = await response;
  expect(result.status(), await result.text()).toBe(200);
  const orderId = String((await result.json()).data.id);
  expect(orderId).toBeTruthy();
  const orderPath = `/holdings/simulation/customers-sales/sales/documents/sales/${orderId}`;
  await page.goto(orderPath, { waitUntil: "domcontentloaded" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
  await expect(page.getByRole("heading", { name: /^SO-/ }).first()).toBeVisible();
  return { id: orderId, path: orderPath };
}

async function readSalesOrder(request: APIRequestContext, id: string) {
  const response = await request.get(`/api/v1/holdings/simulation/commerce/orders/${id}`);
  expect(response.ok(), await response.text()).toBe(true);
  return (await response.json()).data as SalesOrderSnapshot;
}

async function readDeliveryEvidence(request: APIRequestContext, orderId: string) {
  const query = new URLSearchParams({
    targetType: "commerce_order_delivery",
    targetId: orderId,
    purposeKey: "proof_of_delivery",
    componentKind: "action",
  });
  const response = await request.get(`/api/v1/holdings/simulation/attachment-sets?${query}`);
  expect(response.ok(), await response.text()).toBe(true);
  return (await response.json()).data as DeliveryEvidenceSet[];
}

async function attemptOrderCancellation(page: Page, id: string, expectedVersion: number) {
  const contextNode = page.locator("[data-work-revision]").first();
  const workContext = {
    revision: await contextNode.getAttribute("data-work-revision"),
    company: await contextNode.getAttribute("data-work-company"),
    assignment: await contextNode.getAttribute("data-work-assignment"),
  };
  if (!workContext.revision || !workContext.company || !workContext.assignment)
    throw new Error("SALES_CANCELLATION_WORK_CONTEXT_REQUIRED");
  return page.request.post(`/api/v1/holdings/simulation/commerce/orders/${id}/transition`, {
    headers: {
      Origin: process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3102",
      "Idempotency-Key": randomUUID(),
      "x-erpx-context-revision": workContext.revision,
      "x-erpx-context-company": workContext.company,
      "x-erpx-context-assignment": workContext.assignment,
    },
    data: { action: "cancel", expectedVersion },
  });
}

test("isolated Sales cancels before stock movement and requires a controlled return after delivery", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(300_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const salesOperator = credentials.users.find((user) => user.roles?.includes("sales_operator"));
  const salesManager = credentials.users.find((user) => user.roles?.includes("sales_manager"));
  const warehouse = credentials.users.find((user) => user.roles?.includes("warehouse_manager"));
  for (const [label, user] of Object.entries({ salesOperator, salesManager, warehouse })) expect(user, `${label} demo account`).toBeTruthy();

  const salesContext = await createEngineeringBrowserContext(browser, testInfo);
  const managerContext = await createEngineeringBrowserContext(browser, testInfo);
  const warehouseContext = await createEngineeringBrowserContext(browser, testInfo);
  try {
    const salesPage = await salesContext.newPage();
    const managerPage = await managerContext.newPage();
    const warehousePage = await warehouseContext.newPage();

    await signInRole(salesContext, salesPage, credentials, "sales_operator", salesOperator!, "/holdings/simulation/customers-sales/sales/documents/sales");
    const cancellable = await createSalesOrder(salesPage, state, `Pre-movement cancellation witness ${testInfo.project.name}`);
    await clickCommand(salesPage, `/commerce/orders/${cancellable.id}/transition`, "Submit for approval");
    await signInRole(managerContext, managerPage, credentials, "sales_manager", salesManager!, cancellable.path);
    await clickCommand(managerPage, `/commerce/orders/${cancellable.id}/transition`, "Approve order");
    await waitForOrderState(managerPage, cancellable.id, "confirmed");
    const confirmed = await readSalesOrder(managerContext.request, cancellable.id);
    expect(confirmed.order).toMatchObject({ state: "confirmed", version: 3, party_id: state.fixture.customerId, warehouse_id: state.fixture.sourceWarehouseId });
    expect(confirmed.order.approved_by).toBe(salesManager!.userId);
    expect(confirmed.fulfillments).toHaveLength(0);
    expect(confirmed.fulfillmentEvidence).toHaveLength(0);

    await clickCommand(managerPage, `/commerce/orders/${cancellable.id}/transition`, "Cancel approved order");
    const cancelled = await readSalesOrder(managerContext.request, cancellable.id);
    expect(cancelled.order).toMatchObject({ id: confirmed.order.id, number: confirmed.order.number, state: "cancelled", version: 4, party_id: confirmed.order.party_id, warehouse_id: confirmed.order.warehouse_id, approved_by: confirmed.order.approved_by });
    expect(cancelled.lines).toEqual(confirmed.lines);
    expect(cancelled.fulfillments).toHaveLength(0);
    expect(cancelled.history.map((event) => event.action)).toEqual(expect.arrayContaining(["commerce.order.create", "commerce.order.submit", "commerce.order.approve", "commerce.order.cancel"]));

    const delivered = await createSalesOrder(salesPage, state, `Post-delivery cancellation witness ${testInfo.project.name}`);
    await clickCommand(salesPage, `/commerce/orders/${delivered.id}/transition`, "Submit for approval");
    await managerPage.goto(delivered.path, { waitUntil: "domcontentloaded" });
    await expect(managerPage.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
    await clickCommand(managerPage, `/commerce/orders/${delivered.id}/transition`, "Approve order");
    await waitForOrderState(managerPage, delivered.id, "confirmed");

    await signInRole(warehouseContext, warehousePage, credentials, "warehouse_manager", warehouse!, delivered.path);
    await expect(warehousePage.getByRole("heading", { name: "Deliver goods", exact: true })).toBeVisible();
    const orderDrawer = warehousePage.getByTestId("record-detail-drawer");
    await orderDrawer.getByRole("button", { name: "Record delivery", exact: true }).click();
    let deliveryEditor = orderDrawer.locator("[data-record-editor]");
    await expect(deliveryEditor).toBeVisible();
    let deliveryForm = deliveryEditor.locator("form");
    let proofPanel = deliveryForm.getByRole("region", { name: "Proof of delivery", exact: true });
    const chooseFiles = proofPanel.getByText("Choose files", { exact: true });
    const fileChooserPromise = warehousePage.waitForEvent("filechooser");
    await chooseFiles.click();
    const fileChooser = await fileChooserPromise;
    await fileChooser.setFiles({ name: "cancellation-delivery-proof.pdf", mimeType: "application/pdf", buffer: customerDeliveryProofPdf() });
    await expect(proofPanel).toContainText("cancellation-delivery-proof.pdf");
    await expect(proofPanel).toContainText(/1\/\d+ ready/, { timeout: 90_000 });
    await warehousePage.reload({ waitUntil: "domcontentloaded" });
    await expect(warehousePage.getByRole("heading", { name: "Deliver goods", exact: true })).toBeVisible();
    await orderDrawer.getByRole("button", { name: "Record delivery", exact: true }).click();
    deliveryEditor = orderDrawer.locator("[data-record-editor]");
    await expect(deliveryEditor).toBeVisible();
    deliveryForm = deliveryEditor.locator("form");
    proofPanel = deliveryForm.getByRole("region", { name: "Proof of delivery", exact: true });
    await expect(proofPanel).toContainText("cancellation-delivery-proof.pdf");
    await expect(proofPanel).toContainText(/1\/\d+ ready/);
    await deliveryForm.getByRole("textbox", { name: "Receipt confirmation note", exact: true }).fill("Customer signed the retained cancellation-boundary delivery proof.");
    await deliveryForm.getByRole("spinbutton", { name: "SIM-ITEM quantity", exact: true }).fill("1");
    const deliveryResponse = warehousePage.waitForResponse((item) => item.request().method() === "POST" && new URL(item.url()).pathname === `/api/v1/holdings/simulation/commerce/orders/${delivered.id}/fulfill`);
    await deliveryForm.getByRole("button", { name: "Record delivery", exact: true }).click();
    const deliveryResult = await deliveryResponse;
    expect(deliveryResult.status(), await deliveryResult.text()).toBe(200);
    await waitForOrderState(warehousePage, delivered.id, "fulfilled");

    const beforeRejectedCancellation = await readSalesOrder(managerContext.request, delivered.id);
    expect(beforeRejectedCancellation.order.state).toBe("fulfilled");
    expect(beforeRejectedCancellation.fulfillments).toHaveLength(1);
    const beforeRejectedEvidence = await readDeliveryEvidence(warehouseContext.request, delivered.id);
    expect(beforeRejectedEvidence).toHaveLength(1);
    expect(beforeRejectedEvidence[0]).toMatchObject({
      targetType: "commerce_order_delivery",
      targetId: delivered.id,
      purposeKey: "proof_of_delivery",
      state: "bound",
      links: [expect.objectContaining({
        filename: "cancellation-delivery-proof.pdf",
        mediaType: "application/pdf",
        sourceState: "available",
        linkState: "bound",
        boundAt: expect.any(Number),
      })],
    });
    const evidenceLink = beforeRejectedEvidence[0].links[0];
    expect(evidenceLink.sourceId).toBeTruthy();
    const evidenceContentPath = `/api/v1/holdings/simulation/attachment-links/${evidenceLink.id}/content`;
    const evidenceBefore = await warehouseContext.request.get(evidenceContentPath);
    expect(evidenceBefore.status()).toBe(200);
    const proofBytes = customerDeliveryProofPdf();
    expect(await evidenceBefore.body()).toEqual(proofBytes);
    const rejectedCancellation = await attemptOrderCancellation(managerPage, delivered.id, beforeRejectedCancellation.order.version);
    expect(rejectedCancellation.status()).toBe(409);
    expect((await rejectedCancellation.json()).error.code).toBe("ORDER_REVERSAL_REQUIRED");
    const afterRejectedCancellation = await readSalesOrder(managerContext.request, delivered.id);
    expect(afterRejectedCancellation.order).toMatchObject({ id: beforeRejectedCancellation.order.id, number: beforeRejectedCancellation.order.number, state: "fulfilled", version: beforeRejectedCancellation.order.version });
    expect(afterRejectedCancellation.lines).toEqual(beforeRejectedCancellation.lines);
    expect(afterRejectedCancellation.fulfillments).toEqual(beforeRejectedCancellation.fulfillments);
    expect(afterRejectedCancellation.history).toEqual(beforeRejectedCancellation.history);
    const afterRejectedEvidence = await readDeliveryEvidence(warehouseContext.request, delivered.id);
    expect(afterRejectedEvidence).toEqual(beforeRejectedEvidence);
    const evidenceAfter = await warehouseContext.request.get(evidenceContentPath);
    expect(evidenceAfter.status()).toBe(200);
    expect(await evidenceAfter.body()).toEqual(proofBytes);
  } finally {
    await Promise.allSettled([salesContext.close(), managerContext.close(), warehouseContext.close()]);
  }
});

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
  const quotePath = `/holdings/simulation/customers-sales/sales/documents/quote/${quotationId}/overview`;
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
    await expect(salesPage.getByTestId("record-detail-drawer").getByText("draft", { exact: true }).first()).toBeVisible();
    await clickCommand(salesPage, `/commerce/orders/${quotationId}/transition`, "Submit for approval");

    const managerPage = await managerContext.newPage();
    await signInRole(managerContext, managerPage, credentials, "sales_manager", salesManager!, quotePath);
    await clickCommand(managerPage, `/commerce/orders/${quotationId}/transition`, "Approve order");

    await salesPage.goto(quotePath, { waitUntil: "domcontentloaded" });
    await expect(salesPage.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
    await expect(salesPage.getByTestId("record-detail-drawer").locator("h1").filter({ hasText: "QT-" }).first()).toBeVisible();
    await expect(salesPage.getByRole("button", { name: "Create sales order", exact: true })).toBeVisible();
    await captureInvestorCheckpoint(salesPage, "sales-quote", salesPage.getByTestId("record-detail-drawer").locator("h1").first());
    const convertRequest = salesPage.waitForRequest((item) => item.method() === "POST" && item.url().includes(`/commerce/orders/${quotationId}/transition`));
    await salesPage.getByRole("button", { name: "Create sales order", exact: true }).click();
    await convertRequest;
    await salesPage.waitForURL(url => /^\/holdings\/simulation\/customers-sales\/sales\/documents\/sales\/order-[^/]+\/overview$/.test(url.pathname));
    const salesOrderPath = new URL(salesPage.url()).pathname;
    const salesOrderId = decodeURIComponent(salesOrderPath.split("/").at(-2)!);
    await clickCommand(salesPage, `/commerce/orders/${salesOrderId}/transition`, "Submit for approval");

    await managerPage.goto(salesOrderPath, { waitUntil: "domcontentloaded" });
    await expect(managerPage.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
    await expect(managerPage.getByTestId("record-detail-drawer").locator("h1").filter({ hasText: "SO-" }).first()).toBeVisible();
    await expect(managerPage.getByRole("button", { name: "Approve order", exact: true })).toBeVisible();
    await clickCommand(managerPage, `/commerce/orders/${salesOrderId}/transition`, "Approve order");
    await waitForOrderState(managerPage, salesOrderId, "confirmed");

    const warehousePage = await warehouseContext.newPage();
    await signInRole(warehouseContext, warehousePage, credentials, "warehouse_manager", warehouse!, salesOrderPath);
    await expect(warehousePage.getByRole("heading", { name: "Deliver goods", exact: true })).toBeVisible();
    const orderDrawer = warehousePage.getByTestId("record-detail-drawer");
    await orderDrawer.getByRole("button", { name: "Record delivery", exact: true }).click();
    let deliveryEditor = orderDrawer.locator("[data-record-editor]");
    await expect(deliveryEditor).toBeVisible();
    let deliveryForm = deliveryEditor.locator("form");
    let proofPanel = deliveryForm.getByRole('region', { name: 'Proof of delivery', exact: true });
    await expect(proofPanel.getByLabel("Choose files", { exact: true })).toBeEnabled();
    const fileChooserPromise = warehousePage.waitForEvent("filechooser");
    await proofPanel.getByText("Choose files", { exact: true }).click();
    const fileChooser = await fileChooserPromise;
    const pdf = customerDeliveryProofPdf();
    await fileChooser.setFiles({ name: "customer-delivery.pdf", mimeType: "application/pdf", buffer: pdf });
    await expect(proofPanel).toContainText('customer-delivery.pdf');
    await expect(proofPanel).toContainText(/1\/\d+ ready/, { timeout: 90_000 });
    // Confirm an uploaded proof survives a reload, then submit against the
    // freshly rendered order version so delayed workflow refreshes do not
    // leave the delivery form holding stale state.
    await warehousePage.reload({ waitUntil: "domcontentloaded" });
    await expect(warehousePage.getByRole("heading", { name: "Deliver goods", exact: true })).toBeVisible();
    await orderDrawer.getByRole("button", { name: "Record delivery", exact: true }).click();
    deliveryEditor = orderDrawer.locator("[data-record-editor]");
    await expect(deliveryEditor).toBeVisible();
    deliveryForm = deliveryEditor.locator("form");
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
      if (await warehousePage.getByTestId("record-detail-drawer").getByText("Delivery confirmation:", { exact: true }).isVisible().catch(() => false)) break;
      await warehousePage.waitForTimeout(500);
    }
    await expect(warehousePage.getByTestId("record-detail-drawer").getByText("Delivery confirmation:", { exact: true })).toBeVisible({ timeout: 20_000 });
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
    await preparerPage.getByTestId("record-detail-drawer").getByRole("button", { name: "Prepare invoice for GL review", exact: true }).click();
    const invoiceEditor = preparerPage.getByTestId("record-detail-drawer").locator("[data-record-editor]");
    await expect(invoiceEditor).toBeVisible();
    const invoiceSubmit = invoiceEditor.getByRole("button", { name: "Prepare invoice for GL review", exact: true });
    await expect(invoiceSubmit).toBeVisible();
    const invoiceResponse = preparerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/commerce/fulfillments/`) && item.url().endsWith("/invoice"));
    await invoiceSubmit.click();
    const invoiceResult = await invoiceResponse;
    expect(invoiceResult.ok(), await invoiceResult.text()).toBe(true);
    const invoiceId = String((await invoiceResult.json()).data.invoiceId);
    const invoicePath = `/holdings/simulation/finance/documents/invoices/${invoiceId}/overview`;

    await signInRole(approverContext, approverPage, credentials, "finance_approver", financeApprover!, invoicePath);
    await approverPage.getByTestId("record-detail-drawer").locator("summary").filter({ hasText: /^Review invoice posting$/ }).click();
    const invoicePosting = approverPage.getByTestId("record-detail-drawer").locator("[data-record-editor] form");
    await expect(invoicePosting.getByRole("button", { name: "Approve and post invoice", exact: true })).toBeVisible();
    await invoicePosting.locator('select[name="period"]').selectOption(state.fixture.periodId);
    await clickCommand(approverPage, `/finance/invoices/${invoiceId}/post`, "Approve and post invoice");
    await expect(approverPage.getByTestId("record-detail-drawer").getByText("posted", { exact: true }).first()).toBeVisible();
    await captureInvestorCheckpoint(approverPage, "sales-invoice", approverPage.getByTestId("record-detail-drawer").locator("h1").first());

    const treasuryPage = await treasuryContext.newPage();
    await signInRole(treasuryContext, treasuryPage, credentials, "treasury", treasury!, invoicePath);
    await expect(treasuryPage.getByRole("heading", { name: "Record customer receipt", exact: true })).toBeVisible();
    const invoiceBeforeReceipt = await treasuryPage.request.get(`/api/v1/holdings/simulation/finance/invoices/${invoiceId}`);
    expect(invoiceBeforeReceipt.ok(), await invoiceBeforeReceipt.text()).toBe(true);
    const invoiceDate = String((await invoiceBeforeReceipt.json()).data.invoice.invoice_date);
    await treasuryPage.getByTestId('record-detail-drawer').getByRole('button',{name:'Record received payment',exact:true}).click();
    const receiptDrawer=treasuryPage.getByTestId('record-detail-drawer').locator('[data-record-editor]');await expect(receiptDrawer).toHaveCount(1);await expect(receiptDrawer).toBeVisible();
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
    await expect(treasuryPage.getByTestId("record-detail-drawer").getByText("paid", { exact: true }).first()).toBeVisible();
    await expect(treasuryPage.getByTestId("record-detail-drawer").getByText("THB 0.00", { exact: true }).first()).toBeVisible();
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
    await captureInvestorCheckpoint(treasuryPage, "sales-paid", testInfo.project.name === "mobile" ? outstanding : treasuryPage.getByTestId("record-detail-drawer").locator("h1").first());

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
    const returnDrawer=salesPage.getByTestId('record-detail-drawer').locator('[data-record-editor]');await expect(returnDrawer).toHaveCount(1);await expect(returnDrawer).toBeVisible();
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
    const returnReviewEvidence = managerPage.getByTestId('record-detail-drawer').getByLabel("Review evidence", { exact: true }).filter({ visible: true }).first();
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
    await preparerPage.getByTestId("record-detail-drawer").getByRole("button", { name: "Record inspection", exact: true }).click();
    const inspectionDrawer = preparerPage.getByTestId("record-detail-drawer").locator("[data-record-editor]");
    await expect(inspectionDrawer).toBeVisible({ timeout: 20_000 });
    const inspectionForm = inspectionDrawer.locator("form");
    await inspectionForm.locator('select[name="result"]').selectOption("pass");
    await inspectionForm.getByLabel("Inspection evidence", { exact: true }).fill("Returned unit inspected; packaging and function are acceptable for resale.");
    const inspectionResponse = preparerPage.waitForResponse((item) => item.request().method() === "POST" && /\/quality\/[^/]+\/inspect$/.test(new URL(item.url()).pathname));
    await inspectionForm.getByRole("button", { name: "Record inspection", exact: true }).click();
    const inspected = await inspectionResponse;
    expect(inspected.ok()).toBe(true);
    expect((await inspected.json()).data.status).toBe("awaiting_release");
    await preparerPage.reload({ waitUntil: "domcontentloaded" });
    const inspectionStatus = preparerPage.getByTestId("record-detail-drawer").getByRole("complementary", { name: "Quality workflow", exact: true }).getByRole("region", { name: "Status, owner and next action", exact: true }).locator('[data-slot="badge"]');
    await expect(inspectionStatus).toHaveCount(1);
    await expect(inspectionStatus.filter({ hasText: /^awaiting release$/ })).toBeVisible({ timeout: 20_000 });

    await signInRole(treasuryContext, treasuryPage, credentials, "quality_manager", treasury!, qualityPath);
    await treasuryPage.getByTestId("record-detail-drawer").getByRole("button", { name: "Approve release", exact: true }).click();
    const releaseDrawer = treasuryPage.getByTestId("record-detail-drawer").locator("[data-record-editor]");
    await expect(releaseDrawer).toBeVisible({ timeout: 20_000 });
    const releaseForm = releaseDrawer.locator("form");
    await releaseForm.getByLabel("Release reason", { exact: true }).fill("Independent review confirms the returned unit is resalable.");
    const releaseResponse = treasuryPage.waitForResponse((item) => item.request().method() === "POST" && /\/quality\/[^/]+\/release$/.test(new URL(item.url()).pathname));
    await releaseForm.getByRole("button", { name: "Release this hold", exact: true }).click();
    const releaseResult = await releaseResponse;
    expect(releaseResult.ok()).toBe(true);
    expect((await releaseResult.json()).data.status).toBe("released");
    await treasuryPage.reload({ waitUntil: "domcontentloaded" });
    const releaseStatus = treasuryPage.getByTestId("record-detail-drawer").getByRole("complementary", { name: "Quality workflow", exact: true }).getByRole("region", { name: "Status, owner and next action", exact: true }).locator('[data-slot="badge"]');
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
    await ownerPage.getByTestId("record-detail-drawer").locator("summary").filter({ hasText: /^Review refund obligation$/ }).click();
    const refundApproval = ownerPage.getByTestId("record-detail-drawer").locator("[data-record-editor]");
    await expect(refundApproval).toBeVisible();
    await refundApproval.getByLabel("Verified counterparty bank account", { exact: true }).fill("SIM-CUSTOMER-BANK-001");
    await refundApproval.getByLabel("Source and beneficiary review evidence", { exact: true }).fill("Customer identity and posted return credit reviewed independently.");
    const refundApprove = ownerPage.waitForResponse((item) => item.request().method() === "POST" && /\/finance\/refunds\/[^/]+\/approve$/.test(new URL(item.url()).pathname));
    await ownerPage.getByRole("button", { name: "Approve refund obligation", exact: true }).click();
    expect((await refundApprove).ok()).toBe(true);

    await signInRole(treasuryContext, treasuryPage, credentials, "treasury", treasury!, refundPath);
    await expect(treasuryPage.getByRole("heading", { name: "Prepare partial or final instruction", exact: true })).toBeVisible({ timeout: 20_000 });
    await treasuryPage.getByTestId("record-detail-drawer").locator("summary").filter({ hasText: /^Prepare partial or final instruction$/ }).click();
    const refundPreparation = treasuryPage.getByTestId("record-detail-drawer").locator("[data-record-editor]");
    await expect(refundPreparation).toBeVisible();
    await refundPreparation.getByLabel(/Amount \(THB\)/).fill("10.00");
    const plannedRefundDate = refundPreparation.getByLabel("Planned payment date", { exact: true });
    await plannedRefundDate.fill(
      (await plannedRefundDate.getAttribute("min")) ?? new Date().toISOString().slice(0, 10),
    );
    await refundPreparation.locator('select[name="cash"]').selectOption({ index: 1 });
    const refundPrepare = treasuryPage.waitForResponse((item) => item.request().method() === "POST" && /\/finance\/refunds\/[^/]+\/instructions$/.test(new URL(item.url()).pathname));
    await treasuryPage.getByRole("button", { name: "Reserve amount and prepare instruction", exact: true }).click();
    expect((await refundPrepare).ok()).toBe(true);
    await expect(treasuryPage.getByRole("button", { name: "Export prepared instructions", exact: true })).toBeVisible({ timeout: 20_000 });
    const refundExport = treasuryPage.waitForResponse((item) => item.request().method() === "POST" && /\/finance\/refund-instructions\/export$/.test(new URL(item.url()).pathname));
    await treasuryPage.getByRole("button", { name: "Export prepared instructions", exact: true }).click();
    expect((await refundExport).ok()).toBe(true);
    await treasuryPage.getByTestId("record-detail-drawer").locator("summary").filter({ hasText: /^Review bank result and proof$/ }).click();
    const refundConfirmationDrawer = treasuryPage.getByTestId("record-detail-drawer").locator("[data-record-editor]");
    await expect(refundConfirmationDrawer).toBeVisible();
    await refundConfirmationDrawer.locator('select[name="bank"]').selectOption(state.fixture.customerRefundBankTransactionId);
    const refundConfirmation = refundConfirmationDrawer.locator("form");
    await refundConfirmation.locator('select[name="period"]').selectOption(state.fixture.periodId);
    await refundConfirmation.getByLabel("Actual bank reference", { exact: true }).fill("DEMO-REFUND-10");
    await refundConfirmation.getByLabel("Verification evidence", { exact: true }).fill("Exact customer refund bank sandbox row matched to the exported instruction.");
    const refundConfirm = treasuryPage.waitForResponse((item) => item.request().method() === "POST" && /\/finance\/refund-instructions\/[^/]+\/confirm$/.test(new URL(item.url()).pathname));
    await treasuryPage.getByRole("button", { name: "Confirm bank result and post refund", exact: true }).click();
    expect((await refundConfirm).ok()).toBe(true);
    await treasuryPage.reload({ waitUntil: "domcontentloaded" });
    await expect(treasuryPage.getByTestId("record-detail-drawer").getByText(/Pay customer · settled/)).toBeVisible();
    const refundMatch = /^\/holdings\/simulation\/finance\/cash\/refunds\/([^/]+)\/overview$/.exec(refundPath);
    expect(refundMatch, "canonical refund record path").toBeTruthy();
    const refundCheck = await treasuryPage.request.get(`/api/v1/holdings/simulation/finance/refunds/${refundMatch![1]}`);
    expect(refundCheck.ok(), await refundCheck.text()).toBe(true);
    expect((await refundCheck.json()).data.owner).toMatchObject({ status: "settled", outstanding_minor: 0 });
  } finally {
    await Promise.all([salesContext.close(), managerContext.close(), warehouseContext.close(), preparerContext.close(), approverContext.close(), treasuryContext.close(), ownerContext.close()]);
  }
});
