import { expect, test, type BrowserContext, type Locator, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; roles?: string[]; label: string; userId: string };
type Credentials = { password: string; users: DemoUser[] };
type DemoState = { fixture: { hospitality: { purchaseRequestId: string; warehouseId: string } } };

async function signInRole(context: BrowserContext, page: Page, credentials: Credentials, role: string, user: DemoUser, path: string, waitUntil: "domcontentloaded" = "domcontentloaded") {
  let login = await context.request.post("/api/auth/sign-in/email", { data: { email: user.email, password: credentials.password } });
  for (let attempt = 1; !login.ok() && attempt < 5; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, 1500));
    login = await context.request.post("/api/auth/sign-in/email", { data: { email: user.email, password: credentials.password } });
  }
  expect(login.ok()).toBe(true);
  const endpoint = "/api/v1/holdings/riverside-lantern/assignments/active";
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
  const workspace = page.locator("#workspace-content");
  await workspace.getByLabel("Message or decision evidence", { exact: true }).fill(note);
  const response = page.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/api/v1/holdings/riverside-lantern/requests/${requestId}/actions`));
  await workspace.getByRole("button", { name: button, exact: true }).click();
  const result = await response;
  expect(result.ok(), await result.text()).toBe(true);
  await page.goto(`${page.url().split("?")[0]}?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
}

async function submitRequest(page: Page, requestId: string) {
  await page.getByRole("button", { name: "Review & submit", exact: true }).click();
  await expect(page.getByRole("region", { name: "Review request" })).toBeVisible();
  const response = page.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/api/v1/holdings/riverside-lantern/requests/${requestId}/actions`));
  await page.getByRole("button", { name: "Confirm & submit", exact: true }).click();
  const result = await response;
  expect(result.ok(), await result.text()).toBe(true);
  await page.goto(`${page.url().split("?")[0]}?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
}

function actionSelect(page: Page) {
  return page.locator("label").filter({ hasText: "Action" }).locator("select").first();
}

function field(page: Page, label: string | RegExp, options?: { exact?: boolean }) {
  return page.locator("#workspace-content").getByLabel(label, options).first();
}

async function clickInVisualViewport(page: Page, target: Locator) {
  await page.evaluate(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
  const point = await target.evaluate((element) => {
    const rect = element.getBoundingClientRect();
    const x = rect.left + rect.width / 2;
    const y = rect.top + rect.height / 2;
    const hit = document.elementFromPoint(x, y);
    const viewport = window.visualViewport;
    return {
      x: x - (viewport?.offsetLeft ?? 0),
      y: y - (viewport?.offsetTop ?? 0),
      hitTarget: hit === element || Boolean(hit && element.contains(hit)),
    };
  });
  expect(point.hitTarget, "The intended mobile tap point must land on the visible control.").toBe(true);
  await page.mouse.click(point.x, point.y);
}

async function assignWorkflowOwner(page: Page, path: string, kind: "order" | "journal" | "invoice", action: string, user: DemoUser, reason: string) {
  await page.goto(path, { waitUntil: "domcontentloaded" });
  const step = page.locator("#workflow-owners li").filter({ hasText: action }).first();
  await expect(step).toBeVisible();
  const ownerDisclosure = step.locator("details summary");
  await ownerDisclosure.evaluate((element) => element.scrollIntoView({ block: "center", inline: "nearest", behavior: "instant" }));
  await clickInVisualViewport(page, ownerDisclosure);
  await step.locator('select[name="userId"]').selectOption(user.userId);
  await step.locator('input[name="reason"]').fill(reason);
  const id = path.split("?")[0].split("/").at(-1)!;
  const response = page.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/flows/${kind}/${id}`));
  const saveOwner = step.getByRole("button", { name: "Save owner", exact: true });
  await saveOwner.evaluate((element) => element.scrollIntoView({ block: "center", inline: "nearest", behavior: "instant" }));
  await clickInVisualViewport(page, saveOwner);
  const result = await response;
  expect(result.ok(), await result.text()).toBe(true);
  await expect(step).toContainText(user.label);
}

test("isolated demo carries an approved purchase request through split RFQs, receipts, one consolidated AP invoice and supplier payment", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  expect(run, "The Thai Hotel mock runner must supply an exact prepared simulation run.").toMatch(/^[a-f0-9]{12}$/);
  test.setTimeout(300_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const hospitality = state.fixture.hospitality;
  const requestId = hospitality.purchaseRequestId;
  const employee = credentials.users.find((user) => user.email === "employee@riverside-lantern.example");
  const manager = credentials.users.find((user) => user.email === "people-manager@riverside-lantern.example");
  const preparer = credentials.users.find((user) => user.email === "finance-preparer@riverside-lantern.example");
  const approver = credentials.users.find((user) => user.email === "finance-approver@riverside-lantern.example");
  const buyer = credentials.users.find((user) => user.email === "purchasing-operator@riverside-lantern.example");
  const awardApprover = credentials.users.find((user) => user.email === "purchasing-reviewer@riverside-lantern.example");
  const administrator = credentials.users.find((user) => user.email === "admin@riverside-lantern.example");
  const warehouseManager = credentials.users.find((user) => user.email === "warehouse-manager@riverside-lantern.example");
  for (const [label, user] of Object.entries({ employee, manager, preparer, approver, buyer, awardApprover, administrator, warehouseManager })) expect(user, `${label} Riverside account`).toBeTruthy();
  const path = `/holdings/riverside-lantern/requests/${requestId}`;
  const employeeContext = await browser.newContext(testInfo.project.use);
  const managerContext = await browser.newContext(testInfo.project.use);
  const preparerContext = await browser.newContext(testInfo.project.use);
  const approverContext = await browser.newContext(testInfo.project.use);
  const buyerContext = await browser.newContext(testInfo.project.use);
  const awardApproverContext = await browser.newContext(testInfo.project.use);
  const ownerContext = await browser.newContext(testInfo.project.use);
  const warehouseContext = await browser.newContext(testInfo.project.use);
  const treasuryContext = await browser.newContext(testInfo.project.use);
  [employeeContext, managerContext, preparerContext, approverContext, buyerContext, awardApproverContext, ownerContext, warehouseContext, treasuryContext].forEach((context) => context.setDefaultTimeout(15_000));
  try {
    const employeePage = await employeeContext.newPage();
    await signInRole(employeeContext, employeePage, credentials, "employee", employee!, path);
    await submitRequest(employeePage, requestId);

    const managerPage = await managerContext.newPage();
    await signInRole(managerContext, managerPage, credentials, "people_manager", manager!, path);
    await requestCommand(managerPage, requestId, "Approve request", "Line manager reviewed the business need and approved the request.");

    const preparerPage = await preparerContext.newPage();
    await signInRole(preparerContext, preparerPage, credentials, "finance_preparer", preparer!, path);
    await requestCommand(preparerPage, requestId, "Approve request", "Finance preparer reviewed the planned spending and source evidence.");

    const approverPage = await approverContext.newPage();
    await signInRole(approverContext, approverPage, credentials, "finance_approver", approver!, path);
    await requestCommand(approverPage, requestId, "Approve request", "Finance approver independently approved the planned spending.");

    const buyerPage = await buyerContext.newPage();
    await signInRole(buyerContext, buyerPage, credentials, "purchasing_operator", buyer!, path);
    await buyerPage.getByRole("combobox", { name: "Handler", exact: true }).selectOption(buyer!.userId);
    await requestCommand(buyerPage, requestId, "Assign handler", "Purchasing assigned the approved demand to the responsible buyer.");
    await requestCommand(buyerPage, requestId, "Start work", "Purchasing started sourcing against the approved request.");
    await buyerPage.goto(`/holdings/riverside-lantern/procurement-rfqs?request=${requestId}`, { waitUntil: "domcontentloaded" });
    await expect(buyerPage.getByRole("link", { name: /Approved requisition:/ }).first()).toBeVisible();
    await buyerPage.locator("label").filter({ hasText: "Receiving warehouse" }).locator("select").first().selectOption(hospitality.warehouseId);
    const productSelect = buyerPage.locator("label").filter({ hasText: "Demand product 1" }).locator("select").first();
    const productValue = await productSelect.locator("option").filter({ hasText: /^RLH-RICE\s·/ }).getAttribute("value");
    expect(productValue).toBeTruthy();
    await productSelect.selectOption(productValue!);
    await field(buyerPage, /Allocate request quantity/).fill("6");
    await field(buyerPage, /Purchase quantity/).fill("6");
    await field(buyerPage, /kg per one/).fill("1");
    await buyerPage.getByRole("button", { name: "Create quotation request", exact: true }).click();
    await buyerPage.waitForURL(/\/procurement-rfqs\/rfq-/);
    const rfqId = new URL(buyerPage.url()).pathname.split("/").at(-1)!;
    await field(buyerPage, "Supplier").selectOption({ index: 1 });
    await field(buyerPage, "Quotation reference", { exact: true }).fill("DEMO-REQUEST-RFQ-6");
    await field(buyerPage, "Quoted arrival", { exact: true }).fill("2026-10-10");
    await field(buyerPage, "Quote valid through", { exact: true }).fill("2026-10-20");
    await field(buyerPage, "Quotation evidence and terms", { exact: true }).fill("Supplier quotation for six of the ten approved kitchen rice.");
    await buyerPage.locator("#workspace-content").locator('input[name^="price-"]').first().fill("12.50");
    const offerResponse = buyerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/api/v1/holdings/riverside-lantern/procurement-rfqs/${rfqId}`));
    await buyerPage.getByRole("button", { name: "Record quotation", exact: true }).click();
    expect((await offerResponse).ok()).toBe(true);
    await buyerPage.reload({ waitUntil: "domcontentloaded" });
    await expect(buyerPage.locator("td").filter({ hasText: "DEMO-REQUEST-RFQ-6" }).first()).toBeVisible();
    await field(buyerPage, "Supplier").selectOption({ index: 2 });
    await field(buyerPage, "Quotation reference", { exact: true }).fill("DEMO-REQUEST-RFQ-6-COMPARE");
    await field(buyerPage, "Quoted arrival", { exact: true }).fill("2026-10-10");
    await field(buyerPage, "Quote valid through", { exact: true }).fill("2026-10-20");
    await field(buyerPage, "Quotation evidence and terms", { exact: true }).fill("Second fictional supplier quote for comparison; higher unit cost than the first offer.");
    await buyerPage.locator("#workspace-content").locator('input[name^="price-"]').first().fill("13.50");
    const comparisonOffer = buyerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/api/v1/holdings/riverside-lantern/procurement-rfqs/${rfqId}`));
    await buyerPage.getByRole("button", { name: "Record quotation", exact: true }).click();
    expect((await comparisonOffer).ok()).toBe(true);
    await buyerPage.reload({ waitUntil: "domcontentloaded" });
    await expect(buyerPage.locator("td").filter({ hasText: "DEMO-REQUEST-RFQ-6-COMPARE" }).first()).toBeVisible();
    const proposedOffer = buyerPage.locator("#workspace-content").locator('select[name="offer"]').first();
    const cheapestOffer = await proposedOffer.locator("option").filter({ hasText: /DEMO-REQUEST-RFQ-6$/ }).first().getAttribute("value");
    expect(cheapestOffer).toBeTruthy();
    await proposedOffer.selectOption(cheapestOffer!);
    await actionSelect(buyerPage).selectOption("submit");
    await field(buyerPage, "Decision and delivery exception reason", { exact: true }).fill("Proposed the lower comparable supplier quote for the first six kilograms.");
    const submitResponse = buyerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/api/v1/holdings/riverside-lantern/procurement-rfqs/${rfqId}`));
    await buyerPage.getByRole("button", { name: "Record decision", exact: true }).click();
    expect((await submitResponse).ok()).toBe(true);
    await buyerPage.reload({ waitUntil: "domcontentloaded" });
    const buyerWorkContext = await buyerPage.locator("[data-work-revision]").first().evaluate((node) => {
      const element = node as HTMLElement;
      return {
        "x-erpx-context-revision": element.dataset.workRevision ?? "",
        "x-erpx-context-company": element.dataset.workCompany ?? "",
        "x-erpx-context-assignment": element.dataset.workAssignment ?? "",
      };
    });
    const submittedRfq = (await (await buyerContext.request.get(`/api/v1/holdings/riverside-lantern/procurement-rfqs/${rfqId}`)).json()).data;
    const buyerSelfApproval = await buyerContext.request.post(`/api/v1/holdings/riverside-lantern/procurement-rfqs/${rfqId}`, {
      headers: { Origin: process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100", "Idempotency-Key": crypto.randomUUID(), ...buyerWorkContext },
      data: { action: "approve", expectedVersion: Number(submittedRfq.rfq.version), reason: "The purchasing operator cannot approve their own supplier award." },
    });
    expect([403, 409], "an award author cannot approve their own supplier comparison").toContain(buyerSelfApproval.status());
    const afterBuyerSelfApproval = (await (await buyerContext.request.get(`/api/v1/holdings/riverside-lantern/procurement-rfqs/${rfqId}`)).json()).data;
    expect(afterBuyerSelfApproval.rfq.status).toBe(submittedRfq.rfq.status);
    expect(Number(afterBuyerSelfApproval.rfq.version)).toBe(Number(submittedRfq.rfq.version));

    const awardPage = await awardApproverContext.newPage();
    await signInRole(awardApproverContext, awardPage, credentials, "purchasing_manager", awardApprover!, `/holdings/riverside-lantern/procurement-rfqs/${rfqId}`);
    const ownerPage = await ownerContext.newPage();
    await signInRole(ownerContext, ownerPage, credentials, "holding_owner", administrator!, "/holdings/riverside-lantern/home");
    await actionSelect(awardPage).selectOption("approve");
    await awardPage.getByLabel("Decision and delivery exception reason", { exact: true }).first().fill("Independent purchasing reviewer approved the lower-price supplier comparison.");
    const approveResponse = awardPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/api/v1/holdings/riverside-lantern/procurement-rfqs/${rfqId}`));
    await awardPage.getByRole("button", { name: "Record decision", exact: true }).click();
    expect((await approveResponse).ok()).toBe(true);
    await awardPage.reload({ waitUntil: "domcontentloaded" });
    await expect(awardPage.locator("p").filter({ hasText: "Required" }).first()).toContainText("approved");

    // Allocate the remaining four units as a second independently reviewed RFQ/PO.
    await buyerPage.goto(`/holdings/riverside-lantern/procurement-rfqs?request=${requestId}&refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
    await expect(buyerPage.getByRole("link", { name: /Approved requisition:/ }).first()).toBeVisible();
    await expect(buyerPage.getByText("Already allocated", { exact: true }).first()).toBeVisible();
    await buyerPage.locator("label").filter({ hasText: "Receiving warehouse" }).locator("select").first().selectOption(hospitality.warehouseId);
    const secondProductSelect = buyerPage.locator("label").filter({ hasText: "Demand product 1" }).locator("select").first();
    const secondProductValue = await secondProductSelect.locator("option").filter({ hasText: /^RLH-RICE\s·/ }).getAttribute("value");
    expect(secondProductValue).toBeTruthy();
    await secondProductSelect.selectOption(secondProductValue!);
    await field(buyerPage, /Allocate request quantity/).fill("4");
    await field(buyerPage, /Purchase quantity/).fill("4");
    await field(buyerPage, /kg per one/).fill("1");
    await buyerPage.getByRole("button", { name: "Create quotation request", exact: true }).click();
    await buyerPage.waitForURL(/\/procurement-rfqs\/rfq-/);
    const secondRfqId = new URL(buyerPage.url()).pathname.split("/").at(-1)!;
    await field(buyerPage, "Supplier").selectOption({ index: 1 });
    await field(buyerPage, "Quotation reference", { exact: true }).fill("DEMO-REQUEST-RFQ-4");
    await field(buyerPage, "Quoted arrival", { exact: true }).fill("2026-10-10");
    await field(buyerPage, "Quote valid through", { exact: true }).fill("2026-10-20");
    await field(buyerPage, "Quotation evidence and terms", { exact: true }).fill("Supplier quotation for the remaining four of the ten approved kitchen rice.");
    await buyerPage.locator("#workspace-content").locator('input[name^="price-"]').first().fill("12.50");
    const secondOfferResponse = buyerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/api/v1/holdings/riverside-lantern/procurement-rfqs/${secondRfqId}`));
    await buyerPage.getByRole("button", { name: "Record quotation", exact: true }).click();
    expect((await secondOfferResponse).ok()).toBe(true);
    await buyerPage.reload({ waitUntil: "domcontentloaded" });
    await expect(buyerPage.locator("td").filter({ hasText: "DEMO-REQUEST-RFQ-4" }).first()).toBeVisible();
    await actionSelect(buyerPage).selectOption("submit");
    await field(buyerPage, "Decision and delivery exception reason", { exact: true }).fill("Awarded the remaining four units as the second allocation of the approved request.");
    const secondSubmitResponse = buyerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/api/v1/holdings/riverside-lantern/procurement-rfqs/${secondRfqId}`));
    await buyerPage.getByRole("button", { name: "Record decision", exact: true }).click();
    expect((await secondSubmitResponse).ok()).toBe(true);
    await buyerPage.reload({ waitUntil: "domcontentloaded" });
    await awardPage.goto(`/holdings/riverside-lantern/procurement-rfqs/${secondRfqId}`, { waitUntil: "domcontentloaded" });
    await actionSelect(awardPage).selectOption("approve");
    await awardPage.getByLabel("Decision and delivery exception reason", { exact: true }).first().fill("Independent purchasing manager approved the remaining supplier award.");
    const secondApproveResponse = awardPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes(`/api/v1/holdings/riverside-lantern/procurement-rfqs/${secondRfqId}`));
    await awardPage.getByRole("button", { name: "Record decision", exact: true }).click();
    expect((await secondApproveResponse).ok()).toBe(true);
    await awardPage.reload({ waitUntil: "domcontentloaded" });
    await expect(awardPage.locator("p").filter({ hasText: "Required" }).first()).toContainText("approved");

    const createApprovedOrder = async (rfqPath: string) => {
      await buyerPage.goto(`${rfqPath}?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
      await expect(buyerPage.getByRole("heading", { name: "Create purchase order", exact: true })).toBeVisible();
      await buyerPage.getByLabel("Purchase order date", { exact: true }).first().fill("2026-09-22");
      await Promise.all([buyerPage.waitForURL(/\/commerce\/orders\//), buyerPage.getByRole("button", { name: "Create awarded draft PO", exact: true }).click()]);
      const orderPath = new URL(buyerPage.url()).pathname;
      const orderSubmit = buyerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes("/api/v1/holdings/riverside-lantern/commerce/orders/") && item.url().endsWith("/transition"));
      await buyerPage.getByRole("button", { name: "Submit for approval", exact: true }).click();
      expect((await orderSubmit).ok()).toBe(true);
      await assignWorkflowOwner(ownerPage, orderPath, "order", "Approve order", awardApprover!, "Assign the independent purchasing reviewer for this approved supplier comparison.");
      await awardPage.goto(orderPath, { waitUntil: "domcontentloaded" });
      await expect(awardPage.getByRole("button", { name: "Approve order", exact: true })).toBeVisible();
      const orderApproval = awardPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes("/api/v1/holdings/riverside-lantern/commerce/orders/") && item.url().endsWith("/transition"));
      await awardPage.getByRole("button", { name: "Approve order", exact: true }).click();
      expect((await orderApproval).ok()).toBe(true);
      return orderPath;
    };
    const purchaseOrderPath = await createApprovedOrder(`/holdings/riverside-lantern/procurement-rfqs/${rfqId}`);
    const secondPurchaseOrderPath = await createApprovedOrder(`/holdings/riverside-lantern/procurement-rfqs/${secondRfqId}`);

    for (const orderPath of [purchaseOrderPath, secondPurchaseOrderPath]) {
      await assignWorkflowOwner(ownerPage, orderPath, "order", "Receive goods", warehouseManager!, "Assign the independent receiving operator for this purchase order.");
    }

    const warehousePage = await warehouseContext.newPage();
    await signInRole(warehouseContext, warehousePage, credentials, "warehouse_manager", warehouseManager!, `${purchaseOrderPath}?refresh=${Date.now()}`);
    await expect(warehousePage.getByRole("heading", { name: "Receive goods", exact: true }).first()).toBeVisible();
    await warehousePage.getByLabel("RLH-RICE quantity", { exact: true }).first().fill("3");
    const firstReceiptCommand = warehousePage.waitForRequest((item) => item.method() === "POST" && item.url().includes("/api/v1/holdings/riverside-lantern/commerce/orders/") && item.url().endsWith("/fulfill"));
    const firstReceipt = warehousePage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes("/api/v1/holdings/riverside-lantern/commerce/orders/") && item.url().endsWith("/fulfill"));
    await warehousePage.getByRole("button", { name: "Record receipt", exact: true }).click();
    const firstReceiptResponse = await firstReceipt;
    const firstReceiptRequest = await firstReceiptCommand;
    expect(firstReceiptResponse.ok(), await firstReceiptResponse.text()).toBe(true);
    const firstReceiptResult = (await firstReceiptResponse.json()).data;
    const receiptHeaders = await firstReceiptRequest.allHeaders();
    const receiptReplayHeaders = Object.fromEntries(Object.entries(receiptHeaders).filter(([name]) => name === "idempotency-key" || name.startsWith("x-erpx-context-")));
    const receiptReplay = await warehousePage.evaluate(async ({ path, headers, body }) => {
      const response = await fetch(path, { method: "POST", headers, body: JSON.stringify(body) });
      return { ok: response.ok, text: await response.text() };
    }, { path: new URL(firstReceiptRequest.url()).pathname, headers: receiptReplayHeaders, body: firstReceiptRequest.postDataJSON() });
    expect(receiptReplay.ok, receiptReplay.text).toBe(true);
    expect(JSON.parse(receiptReplay.text).data.fulfillmentId).toBe(firstReceiptResult.fulfillmentId);
    await warehousePage.goto(`${purchaseOrderPath}?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
    await expect(warehousePage.getByRole("heading", { name: "Receive goods", exact: true }).first()).toBeVisible();
    await expect(warehousePage.getByLabel("RLH-RICE quantity", { exact: true }).first()).toHaveValue("3");
    const secondPartialReceipt = warehousePage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes("/api/v1/holdings/riverside-lantern/commerce/orders/") && item.url().endsWith("/fulfill"));
    await warehousePage.getByRole("button", { name: "Record receipt", exact: true }).click();
    expect((await secondPartialReceipt).ok()).toBe(true);
    await warehousePage.goto(`${secondPurchaseOrderPath}?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
    await expect(warehousePage.getByRole("heading", { name: "Receive goods", exact: true }).first()).toBeVisible();
    await warehousePage.getByLabel("RLH-RICE quantity", { exact: true }).first().fill("4");
    const secondReceipt = warehousePage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes("/api/v1/holdings/riverside-lantern/commerce/orders/") && item.url().endsWith("/fulfill"));
    await warehousePage.getByRole("button", { name: "Record receipt", exact: true }).click();
    expect((await secondReceipt).ok()).toBe(true);

    for (const orderPath of [purchaseOrderPath, secondPurchaseOrderPath]) {
      await approverPage.goto(`${orderPath}?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
      const goodsJournalLinks = approverPage.getByRole("link", { name: "Review goods movement GL", exact: true });
      const expectedReceipts = orderPath === purchaseOrderPath ? 2 : 1;
      await expect(goodsJournalLinks).toHaveCount(expectedReceipts, { timeout: 20_000 });
      const goodsJournalPaths = (await goodsJournalLinks.evaluateAll((links) => links.map((link) => link.getAttribute("href"))))
        .filter((path): path is string => Boolean(path));
      for (const goodsJournalPath of goodsJournalPaths) {
        await assignWorkflowOwner(ownerPage, goodsJournalPath, "journal", "Review Debit / Credit and post GL", approver!, "Assign the independent Finance reviewer for this goods movement journal.");
        await approverPage.goto(goodsJournalPath, { waitUntil: "domcontentloaded" });
        await expect(approverPage.getByRole("button", { name: "Approve and post journal", exact: true })).toBeVisible();
        const goodsJournalPost = approverPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes("/api/v1/holdings/riverside-lantern/finance/journals/") && item.url().endsWith("/post"));
        await approverPage.getByRole("button", { name: "Approve and post journal", exact: true }).click();
        expect((await goodsJournalPost).ok()).toBe(true);
      }
    }

    for (const orderPath of [purchaseOrderPath, secondPurchaseOrderPath]) {
      await assignWorkflowOwner(ownerPage, orderPath, "order", "Prepare invoice for independent GL review", preparer!, "Assign Finance preparation after all linked goods receipts are posted.");
    }

    const createAndPostBill = async () => {
      await preparerPage.goto(`/holdings/riverside-lantern/supplier-bills?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
      await preparerPage.getByText(/Match one supplier invoice to/, { exact: false }).first().click();
      await field(preparerPage, "Supplier invoice reference", { exact: true }).fill("DEMO-REQUEST-INVOICE-CONSOLIDATED");
      const invoiceDate = field(preparerPage, "Invoice date", { exact: true });
      const earliestInvoiceDate = await invoiceDate.getAttribute("min");
      expect(earliestInvoiceDate, "invoice cannot predate the latest matched receipt").toBeTruthy();
      await invoiceDate.fill(earliestInvoiceDate!);
      await expect(invoiceDate).toHaveValue(earliestInvoiceDate!);
      await field(preparerPage, "Payment due date", { exact: true }).fill("2026-10-22");
      const billQuantities = preparerPage.locator('input[name^="quantity-"]');
      await expect(billQuantities).toHaveCount(3);
      // The capture form is keyed by receipt-line id and may order lines by
      // database id. Its defaults are the exact posted receipt quantities and
      // unit prices, so keep those values instead of assuming PO order.
      expect((await billQuantities.evaluateAll((inputs) => inputs.map((input) => (input as HTMLInputElement).value).sort()))).toEqual(["3", "3", "4"]);
      await field(preparerPage, "Supplier invoice evidence", { exact: true }).fill("One supplier invoice covers the six-unit and four-unit receipts from the approved request.");
      const billResponse = preparerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith("/api/v1/holdings/riverside-lantern/supplier-bills"));
      await preparerPage.getByRole("button", { name: "Send invoice for independent matching review", exact: true }).click();
      const billResult = await billResponse;
      expect(billResult.ok(), await billResult.text()).toBe(true);
      const billId = String((await billResult.json()).data.id);
      await approverPage.goto(`/holdings/riverside-lantern/supplier-bills/${billId}`, { waitUntil: "domcontentloaded" });
      await expect(approverPage.getByRole("button", { name: "Record review", exact: true })).toBeVisible();
      await field(approverPage, "Review reason", { exact: true }).fill("Finance independently matched the posted receipt and the supplier invoice.");
      const billReview = approverPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/api/v1/holdings/riverside-lantern/supplier-bills/${billId}`));
      await approverPage.getByRole("button", { name: "Record review", exact: true }).click();
      expect((await billReview).ok()).toBe(true);
      await preparerPage.goto(`/holdings/riverside-lantern/supplier-bills/${billId}?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
      await expect(preparerPage.getByRole("button", { name: "Prepare matched AP invoice", exact: true })).toBeVisible();
      const invoiceResponse = preparerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes("/api/v1/holdings/riverside-lantern/commerce/fulfillments/") && item.url().endsWith("/invoice"));
      await preparerPage.getByRole("button", { name: "Prepare matched AP invoice", exact: true }).click();
      const invoiceResult = await invoiceResponse;
      expect(invoiceResult.ok(), await invoiceResult.text()).toBe(true);
      const invoiceId = String((await invoiceResult.json()).data.invoiceId);
      const invoicePath = `/holdings/riverside-lantern/finance/invoices/${invoiceId}`;
      await assignWorkflowOwner(ownerPage, invoicePath, "invoice", "Review invoice Debit / Credit and post GL", approver!, "Assign the independent Finance approver to post the supplier invoice.");
      await approverPage.goto(invoicePath, { waitUntil: "domcontentloaded" });
      await expect(approverPage.getByRole("button", { name: "Approve and post invoice", exact: true })).toBeVisible();
      await approverPage.locator("#workspace-content").locator('select[name="period"]').first().selectOption({ label: "2026" });
      const invoicePost = approverPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/api/v1/holdings/riverside-lantern/finance/invoices/${invoiceId}/post`));
      await approverPage.getByRole("button", { name: "Approve and post invoice", exact: true }).click();
      const invoicePostResult = await invoicePost;
      expect(invoicePostResult.ok(), await invoicePostResult.text()).toBe(true);
      await approverPage.reload({ waitUntil: "domcontentloaded" });
      await expect(approverPage.getByText("posted", { exact: true }).first()).toBeVisible();
      return invoiceId;
    };

    const invoiceId = await createAndPostBill();
    // Continue the same source-linked obligation through the controlled supplier payment path.
    await preparerPage.goto("/holdings/riverside-lantern/finance/supplier-batches", { waitUntil: "domcontentloaded" });
    await expect(preparerPage.getByRole("heading", { name: "Prepare supplier instructions", exact: true })).toBeVisible();
    const firstInvoiceRow = preparerPage.locator("fieldset").filter({ hasText: "Available THB 125.00" }).first();
    await expect(firstInvoiceRow).toBeVisible();
    await firstInvoiceRow.getByRole("checkbox").check();
    const paymentBank = preparerPage.locator("#workspace-content").locator('select[name="bank"]').first();
    await paymentBank.selectOption({ index: 1 });
    const paymentBankAccountId = await paymentBank.inputValue();
    await field(preparerPage, "Planned payment date", { exact: true }).fill("2026-10-01");
    await firstInvoiceRow.getByLabel("Verified beneficiary account", { exact: true }).fill("SANDBOX-ACCOUNT-001");
    const batchCreate = preparerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith("/api/v1/holdings/riverside-lantern/finance/supplier-batches"));
    await preparerPage.getByRole("button", { name: "Reserve selected amounts", exact: true }).click();
    const batchResult = await batchCreate;
    expect(batchResult.ok(), await batchResult.text()).toBe(true);
    await preparerPage.waitForURL(/\/finance\/supplier-batches\/supplier-batch-/);
    const batchPath = new URL(preparerPage.url()).pathname;
    const batchId = batchPath.split("/").at(-1)!;
    const preparedBatchResponse = await preparerContext.request.get(`/api/v1/holdings/riverside-lantern/finance/supplier-batches/${batchId}`);
    expect(preparedBatchResponse.ok()).toBe(true);
    expect((await preparedBatchResponse.json()).data.batch.bank_account_id).toBe(paymentBankAccountId);

    await approverPage.goto(batchPath, { waitUntil: "domcontentloaded" });
    await expect(approverPage.getByRole("heading", { name: "Review the batch", exact: true })).toBeVisible();
    await field(approverPage, "Review evidence or cancellation reason", { exact: true }).fill("Finance independently reviewed the posted AP invoice, amount and beneficiary.");
    const batchApproval = approverPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith("/approve"));
    await approverPage.getByRole("button", { name: "Record decision", exact: true }).click();
    expect((await batchApproval).ok()).toBe(true);
    await expect(approverPage.getByText(/2026-10-01 · THB · approved/)).toBeVisible();

    const treasuryPage = await treasuryContext.newPage();
    const treasury = credentials.users.find((user) => user.email === "treasury@riverside-lantern.example");
    expect(treasury, "treasury demo account").toBeTruthy();
    await signInRole(treasuryContext, treasuryPage, credentials, "treasury", treasury!, batchPath);
    await expect(treasuryPage.getByRole("button", { name: "Export approved instructions", exact: true })).toBeVisible();
    const batchExport = treasuryPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith("/export"));
    await treasuryPage.getByRole("button", { name: "Export approved instructions", exact: true }).click();
    expect((await batchExport).ok()).toBe(true);
    await expect(treasuryPage.getByText("Export SHA-256", { exact: false })).toBeVisible();

    await preparerPage.goto("/holdings/riverside-lantern/finance/bank?new=1", { waitUntil: "domcontentloaded" });
    await preparerPage.getByRole("button", { name: "Import bank statement", exact: true }).click();
    // Demo account labels are configured data (and can change with the
    // chart fixture), so select the first real bank account after the
    // placeholder rather than relying on a hard-coded code/name.
    await preparerPage.locator("#workspace-content").locator('select[name="bankAccount"]:visible').first().selectOption(paymentBankAccountId);
    await field(preparerPage, "Statement number", { exact: true }).fill("DEMO-SUPPLIER-PAY-2026");
    await field(preparerPage, "Statement date", { exact: true }).fill("2026-10-02");
    await field(preparerPage, "Opening balance", { exact: true }).fill("125");
    await field(preparerPage, "Closing balance", { exact: true }).fill("0");
    await field(preparerPage, "Bank reference 1", { exact: true }).fill("DEMO-BANK-SUPPLIER-125");
    await field(preparerPage, "Transaction date 1", { exact: true }).fill("2026-10-02");
    await field(preparerPage, "Description 1", { exact: true }).fill("Consolidated supplier payment for both approved kitchen rice receipts");
    await field(preparerPage, "Signed amount 1", { exact: true }).fill("-125");
    const bankImport = preparerPage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith("/api/v1/holdings/riverside-lantern/finance/bank/statements/import"));
    await preparerPage.getByRole("button", { name: "Validate and import", exact: true }).click();
    expect((await bankImport).ok()).toBe(true);

    await treasuryPage.goto(`${batchPath}?refresh=${Date.now()}`, { waitUntil: "domcontentloaded" });
    const reconciledBatchResponse = await treasuryContext.request.get(`/api/v1/holdings/riverside-lantern/finance/supplier-batches/${batchId}`);
    expect(reconciledBatchResponse.ok()).toBe(true);
    const matchingBankRow = (await reconciledBatchResponse.json()).data.bank.find((row: { external_id: string; amount_minor: number; bank_account: string }) => row.external_id === "DEMO-BANK-SUPPLIER-125");
    expect(matchingBankRow).toMatchObject({ amount_minor: -12500, bank_account: paymentBankAccountId });
    const instructionCards = treasuryPage.locator("section.card").filter({ hasText: /BILL-/ });
    await expect(instructionCards).toHaveCount(1, { timeout: 20_000 });
    const instructionCard = instructionCards.first();
    await instructionCard.locator('select[name="bank"]').selectOption(String(matchingBankRow.id));
    await instructionCard.locator('select[name="period"]').selectOption({ label: "2026" });
    await instructionCard.locator('input[name="reference"]').fill("DEMO-BANK-SUPPLIER-125");
    await instructionCard.locator('textarea[name="evidence"]').first().fill("Exact sandbox bank debit matched to the exported consolidated supplier instruction.");
    const settleCommand = treasuryPage.waitForRequest((item) => item.method() === "POST" && item.url().includes("/supplier-instructions/") && item.url().endsWith("/confirm"));
    const settle = treasuryPage.waitForResponse((item) => item.request().method() === "POST" && item.url().includes("/supplier-instructions/") && item.url().endsWith("/confirm"));
    await instructionCard.getByRole("button", { name: "Confirm bank debit and settle invoice", exact: true }).click();
    const settledCommandResponse = await settle;
    const settledCommandRequest = await settleCommand;
    expect(settledCommandResponse.ok(), await settledCommandResponse.text()).toBe(true);
    const settlementHeaders = await settledCommandRequest.allHeaders();
    const settlementReplayHeaders = Object.fromEntries(Object.entries(settlementHeaders).filter(([name]) => name === "idempotency-key" || name.startsWith("x-erpx-context-")));
    const settlementReplay = await treasuryPage.evaluate(async ({ path, headers, body }) => {
      const response = await fetch(path, { method: "POST", headers, body: JSON.stringify(body) });
      return { ok: response.ok, text: await response.text() };
    }, { path: new URL(settledCommandRequest.url()).pathname, headers: settlementReplayHeaders, body: settledCommandRequest.postDataJSON() });
    expect(settlementReplay.ok, settlementReplay.text).toBe(true);
    await treasuryPage.reload({ waitUntil: "domcontentloaded" });
    await expect(treasuryPage.getByText(/· confirmed$/)).toHaveCount(1);

    const settledInvoice = await treasuryContext.request.get(`/api/v1/holdings/riverside-lantern/finance/invoices/${invoiceId}`);
    expect(settledInvoice.ok()).toBe(true);
    expect((await settledInvoice.json()).data.invoice).toMatchObject({ status: "paid", outstanding_minor: 0 });

    const detail = await buyerContext.request.get(`/api/v1/holdings/riverside-lantern/procurement-rfqs/${rfqId}`);
    expect(detail.ok()).toBe(true);
    const rfq = (await detail.json()).data.rfq;
    expect(rfq).toMatchObject({ status: "ordered", request_id: requestId });
  } finally {
    await Promise.all([employeeContext.close(), managerContext.close(), preparerContext.close(), approverContext.close(), buyerContext.close(), awardApproverContext.close(), ownerContext.close(), warehouseContext.close(), treasuryContext.close()]);
  }
});
