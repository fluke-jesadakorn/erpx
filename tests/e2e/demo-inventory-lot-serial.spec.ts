import {createEngineeringBrowserContext} from './helpers/engineering-browser-evidence';
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

type DemoUser = { email: string; role: string; roles?: string[]; label: string; userId: string };
type Credentials = { password: string; users: DemoUser[] };
type ReceiptDetail = {
  fulfillments: Array<{ id: string; journal_id: string; stock_journal_status: string }>;
  fulfillmentLines: Array<{ quantity_milli: number; value_minor: number }>;
  lotAllocations: Array<{ lot_code: string; serial_number: string; quantity_milli: number; disposition: string }>;
  lots: unknown[];
};

async function signInRole(
  context: BrowserContext,
  page: Page,
  credentials: Credentials,
  role: string,
  user: DemoUser,
  path: string,
) {
  let login = await context.request.post("/api/auth/sign-in/email", {
    data: { email: user.email, password: credentials.password },
  });
  for (let attempt = 1; !login.ok() && attempt < 5; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, 1_500));
    login = await context.request.post("/api/auth/sign-in/email", {
      data: { email: user.email, password: credentials.password },
    });
  }
  expect(login.ok(), await login.text()).toBe(true);
  const endpoint = "/api/v1/holdings/simulation/assignments/active";
  const active = (await (await context.request.get(endpoint)).json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${user.label} ${role} assignment`).toBeTruthy();
  const selected = await context.request.post(endpoint, {
    data: { assignmentId: assignment.id, revision: active.revision },
  });
  expect(selected.ok(), await selected.text()).toBe(true);
  await page.goto(path, { waitUntil: "domcontentloaded" });
  await expect(page.locator("[data-work-stale]")).toHaveAttribute("data-work-stale", "false");
}

async function submitOrder(page: Page, orderId: string) {
  const response = page.waitForResponse(
    (item) => item.request().method() === "POST" && item.url().endsWith(`/commerce/orders/${orderId}/transition`),
  );
  await page.getByRole("button", { name: "Submit for approval", exact: true }).click();
  expect((await response).ok()).toBe(true);
  await page.reload({ waitUntil: "domcontentloaded" });
}

async function approveOrder(page: Page, orderId: string) {
  const response = page.waitForResponse(
    (item) => item.request().method() === "POST" && item.url().endsWith(`/commerce/orders/${orderId}/transition`),
  );
  await page.getByRole("button", { name: "Approve order", exact: true }).click();
  expect((await response).ok()).toBe(true);
  await page.reload({ waitUntil: "domcontentloaded" });
}

test("isolated demo receives lot and serial stock with independent approval and quarantine evidence", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  test.skip(!run, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(180_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as Credentials;
  const buyer = credentials.users.find((user) => user.roles?.includes("purchasing_operator"));
  const approver = credentials.users.find((user) => user.label === "Support manager");
  const warehouse = credentials.users.find((user) => user.role === "warehouse_manager");
  const quality = credentials.users.find((user) => user.roles?.includes("quality_inspector"));
  const finance = credentials.users.find((user) => user.role === "finance_approver");
  expect(buyer).toBeTruthy();
  expect(approver).toBeTruthy();
  expect(warehouse).toBeTruthy();
  expect(quality).toBeTruthy();
  expect(finance).toBeTruthy();
  expect(finance!.userId).not.toBe(buyer!.userId);
  expect(finance!.userId).not.toBe(warehouse!.userId);

  const buyerContext = await createEngineeringBrowserContext(browser,testInfo);
  const approverContext = await createEngineeringBrowserContext(browser,testInfo);
  const warehouseContext = await createEngineeringBrowserContext(browser,testInfo);
  const qualityContext = await createEngineeringBrowserContext(browser,testInfo);
  const financeContext = await createEngineeringBrowserContext(browser,testInfo);
  try {
    const buyerPage = await buyerContext.newPage();
    buyerPage.setDefaultTimeout(30_000);
    await signInRole(buyerContext, buyerPage, credentials, "purchasing_operator", buyer!, "/holdings/simulation/commerce?view=purchase&new=1");
    await expect(buyerPage.getByRole("heading", { name: "Purchase orders", level: 1, exact: true })).toBeVisible();

    const createPurchaseOrder = async (sku: string, quantity: string, price: string, note: string) => {
      await buyerPage.goto("/holdings/simulation/commerce?view=purchase&new=1", { waitUntil: "domcontentloaded" });
      await buyerPage.waitForTimeout(750);
      const supplier = buyerPage.locator("#workspace-content").locator("label").filter({ hasText: "Supplier" }).locator("select");
      const warehouse = buyerPage.locator("#workspace-content").locator("label").filter({ hasText: "Warehouse" }).locator("select");
      await supplier.selectOption({ label: "Demo supplier A" });
      await warehouse.selectOption({ label: "Simulation receiving warehouse" });
      await buyerPage.getByRole("button", { name: "Add product", exact: true }).click();
      const product = buyerPage.locator("#workspace-content").locator("label").filter({ hasText: "Product 1" }).locator("select");
      const productValue = await product.locator("option").filter({ hasText: sku }).getAttribute("value");
      expect(productValue, `${sku} product option`).toBeTruthy();
      await product.selectOption(productValue!);
      await buyerPage.locator("#workspace-content").locator("label").filter({ hasText: "Quantity 1" }).locator("input").fill(quantity);
      await buyerPage.locator("#workspace-content").locator("label").filter({ hasText: "Unit price 1" }).locator("input").fill(price);
      await buyerPage.locator("#workspace-content").locator("label").filter({ hasText: "Tax rate 1" }).locator("input").fill("0");
      await buyerPage.locator("#workspace-content").locator("label").filter({ hasText: "Order notes" }).locator("input").fill(note);
      await Promise.all([
        buyerPage.waitForURL(/\/holdings\/simulation\/commerce\/orders\/order-/),
        buyerPage.getByRole("button", { name: "Save draft", exact: true }).click(),
      ]);
      const orderId = new URL(buyerPage.url()).pathname.split("/").at(-1)!;
      await expect(buyerPage.getByRole("heading", { name: "Receive goods", exact: true })).toHaveCount(0);
      await submitOrder(buyerPage, orderId);
      await expect(buyerPage.locator("#workspace-content").getByText("submitted", { exact: true }).first()).toBeVisible();
      return orderId;
    };

    const lotOrderId = await createPurchaseOrder("SIM-LOT-MED", "2", "1200", "Lot-tracked medical supply receipt for isolated inventory acceptance.");
    const serialOrderId = await createPurchaseOrder("SIM-SERIAL-DEVICE", "1", "35000", "Serial-tracked service device receipt for isolated inventory acceptance.");

    const approverPage = await approverContext.newPage();
    await signInRole(approverContext, approverPage, credentials, "purchasing_manager", approver!, `/holdings/simulation/commerce/orders/${lotOrderId}`);
    await approveOrder(approverPage, lotOrderId);
    await approverPage.goto(`/holdings/simulation/commerce/orders/${serialOrderId}`, { waitUntil: "domcontentloaded" });
    await approveOrder(approverPage, serialOrderId);

    const warehousePage = await warehouseContext.newPage();
    await signInRole(warehouseContext, warehousePage, credentials, "warehouse_manager", warehouse!, `/holdings/simulation/commerce/orders/${lotOrderId}`);
    await expect(warehousePage.getByRole("heading", { name: "Receive goods", exact: true }).first()).toBeVisible();
    await warehousePage.getByRole("button", { name: "Add lot", exact: true }).click();
    await warehousePage.locator("#workspace-content").locator("label").filter({ hasText: "Lot code" }).locator("input").fill("DEMO-LOT-2026-01");
    await warehousePage.locator("#workspace-content").locator("label").filter({ hasText: "Quantity" }).locator("input").fill("2");
    await warehousePage.locator("#workspace-content").locator("label").filter({ hasText: "Receipt disposition" }).locator("select").selectOption("quarantine");
    await warehousePage.locator("#workspace-content").locator("label").filter({ hasText: "Quarantine reason" }).locator("input").fill("Medical supply lot requires independent Quality inspection before use.");
    await warehousePage.locator("#workspace-content").locator("label").filter({ hasText: "Review due date" }).locator("input").fill("2026-10-15");
    const lotReceipt = warehousePage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/commerce/orders/${lotOrderId}/fulfill`));
    await warehousePage.getByRole("button", { name: "Record receipt", exact: true }).click();
    expect((await lotReceipt).ok()).toBe(true);
    await warehousePage.goto(`/holdings/simulation/commerce/orders/${lotOrderId}?refresh=${Date.now()}`, { waitUntil: "commit" });
    const lotDetail = await warehouseContext.request.get(`/api/v1/holdings/simulation/commerce/orders/${lotOrderId}`);
    expect(lotDetail.ok(), await lotDetail.text()).toBe(true);
    const lotData = (await lotDetail.json()).data as ReceiptDetail;
    expect(lotData.lotAllocations).toEqual(expect.arrayContaining([expect.objectContaining({ lot_code: "DEMO-LOT-2026-01", quantity_milli: 2000, disposition: "quarantine" })]));

    const serialBefore = await warehouseContext.request.get(`/api/v1/holdings/simulation/commerce/orders/${serialOrderId}`);
    expect(serialBefore.ok(), await serialBefore.text()).toBe(true);
    await warehousePage.goto(`/holdings/simulation/commerce/orders/${serialOrderId}`, { waitUntil: "commit" });
    await warehousePage.waitForTimeout(1_000);
    const addSerial = warehousePage.getByRole("button", { name: "Add serial", exact: true });
    await addSerial.first().click();
    const serialNumberInput = warehousePage.locator("#workspace-content").locator("label").filter({ hasText: "Serial number" }).locator("input");
    const serialQuantityInput = warehousePage.locator("#workspace-content").locator("label").filter({ hasText: "Quantity" }).locator("input");
    await serialNumberInput.fill("DEMO-SERIAL-2026-01");
    await expect(serialQuantityInput).toHaveValue("1");
    const serialReceipt = warehousePage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/commerce/orders/${serialOrderId}/fulfill`));
    await warehousePage.getByRole("button", { name: "Record receipt", exact: true }).click();
    expect((await serialReceipt).ok()).toBe(true);
    await warehousePage.goto(`/holdings/simulation/commerce/orders/${serialOrderId}?refresh=${Date.now()}`, { waitUntil: "commit" });
    const serialDetail = await warehouseContext.request.get(`/api/v1/holdings/simulation/commerce/orders/${serialOrderId}`);
    expect(serialDetail.ok(), await serialDetail.text()).toBe(true);
    const serialData = (await serialDetail.json()).data as ReceiptDetail;
    expect(serialData.lotAllocations).toEqual(expect.arrayContaining([expect.objectContaining({ serial_number: "DEMO-SERIAL-2026-01", quantity_milli: 1000, disposition: "available" })]));

    await warehousePage.goto("/holdings/simulation/commerce?view=stock", { waitUntil: "domcontentloaded" });
    const lotRow = warehousePage.getByRole("row").filter({ hasText: "SIM-LOT-MED" });
    await expect(lotRow).toContainText("lot");
    await expect(lotRow).toContainText("2");
    const serialRow = warehousePage.getByRole("row").filter({ hasText: "SIM-SERIAL-DEVICE" });
    await expect(serialRow).toContainText("serial");
    await expect(serialRow).toContainText("1");

    const qualityPage = await qualityContext.newPage();
    await signInRole(qualityContext, qualityPage, credentials, "quality_inspector", quality!, "/holdings/simulation/quality");
    await expect(qualityPage.getByRole("heading", { name: "Quality holds", exact: true })).toBeVisible();
    const qualityResponse = await qualityContext.request.get("/api/v1/holdings/simulation/quality");
    expect(qualityResponse.ok(), await qualityResponse.text()).toBe(true);
    const qualityData = (await qualityResponse.json()).data as { holds: Array<{ lot_code: string; held_quantity_milli: number; status: string }> };
    expect(qualityData.holds).toEqual(expect.arrayContaining([expect.objectContaining({ lot_code: "DEMO-LOT-2026-01", held_quantity_milli: 2000, status: "held" })]));

    const financePage = await financeContext.newPage();
    await signInRole(financeContext, financePage, credentials, "finance_approver", finance!, `/holdings/simulation/commerce/orders/${lotOrderId}`);
    const postedReceipts: Array<{ orderId: string; fulfillmentId: string; journalId: string; valueMinor: number }> = [];
    for (const { orderId, before, quantityMilli, valueMinor, amount } of [
      { orderId: lotOrderId, before: lotData, quantityMilli: 2000, valueMinor: 240000, amount: "2400.00" },
      { orderId: serialOrderId, before: serialData, quantityMilli: 1000, valueMinor: 3500000, amount: "35000.00" },
    ]) {
      expect(before.fulfillments).toHaveLength(1);
      expect(before.fulfillmentLines).toHaveLength(1);
      expect(before.fulfillmentLines[0]).toMatchObject({ quantity_milli: quantityMilli, value_minor: valueMinor });
      const receipt = before.fulfillments[0];
      expect(receipt.stock_journal_status).toBe("draft");
      await financePage.goto(`/holdings/simulation/commerce/orders/${orderId}`, { waitUntil: "domcontentloaded" });
      const journalLink = financePage.getByRole("main").getByRole("link", { name: "Review goods movement GL", exact: true });
      await expect(journalLink).toHaveCount(1);
      await expect(journalLink).toHaveAttribute("href", `/holdings/simulation/finance/journals/${receipt.journal_id}`);
      await journalLink.click();
      await expect(financePage).toHaveURL(url => url.searchParams.get('detailKind') === 'journal' && url.searchParams.get('detailId') === receipt.journal_id);
      const journalDrawer = financePage.getByTestId('record-detail-drawer');
      await expect(journalDrawer).toHaveCount(1);
      await expect(journalDrawer).toBeVisible();
      const fullJournal = journalDrawer.getByRole('link', { name: 'Open full page ↗', exact: true });
      await expect(fullJournal).toHaveAttribute('href', `/holdings/simulation/finance/journals/${receipt.journal_id}`);
      await fullJournal.click();
      await expect(financePage).toHaveURL(new RegExp(`/finance/journals/${receipt.journal_id}$`));
      await expect(journalDrawer).toBeHidden();
      const main = financePage.getByRole("main");
      const expectJournalAmounts = async () => {
        for (const [account, debit, credit] of [
          ["INV · Simulation inventory", amount, "0.00"],
          ["GRNI · Simulation goods received clearing", "0.00", amount],
        ]) {
          const row = main.getByRole("cell", { name: account, exact: true }).locator("xpath=ancestor::tr[1]");
          await expect(row).toHaveCount(1);
          await expect(row.locator('xpath=ancestor::table[1]').getByRole('columnheader')).toHaveText(['Account','Debit (THB)','Credit (THB)','Details']);
          await expect(row.getByRole("cell")).toHaveCount(4);
          await expect(row.getByRole("cell").nth(1)).toHaveText(debit);
          await expect(row.getByRole("cell").nth(2)).toHaveText(credit);
          const accountLink=row.getByRole('cell').nth(0).getByRole('link',{name:account,exact:true});
          await expect(accountLink).toHaveCount(1);
          await expect(accountLink).toHaveAttribute('href',/^\/holdings\/simulation\/record-details\/finance_account\/[^/?]+$/);
          const detailsLink=row.getByRole('cell').nth(3).getByRole('link',{name:'View details',exact:true});
          await expect(detailsLink).toHaveCount(1);
          await expect(detailsLink).toHaveAttribute('href',(await accountLink.getAttribute('href'))!);
        }
      };
      await expectJournalAmounts();
      const postButton = main.getByRole("button", { name: "Approve and post journal", exact: true });
      await expect(postButton).toHaveCount(1);
      const posted = financePage.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith(`/finance/journals/${receipt.journal_id}/post`));
      await postButton.click();
      expect((await posted).ok()).toBe(true);
      await financePage.reload({ waitUntil: "domcontentloaded" });
      await expect(postButton).toHaveCount(0);
      await expectJournalAmounts();
      const journalsResponse = await financeContext.request.get("/api/v1/holdings/simulation/finance/journals?status=posted");
      expect(journalsResponse.ok(), await journalsResponse.text()).toBe(true);
      const journals = (await journalsResponse.json()).data as Array<{ id: string; status: string; source_type: string; source_id: string; created_by: string; posted_by: string }>;
      const journal = journals.filter((item) => item.id === receipt.journal_id);
      expect(journal).toHaveLength(1);
      expect(journal[0]).toMatchObject({ status: "posted", source_type: "goods_receipt", source_id: receipt.id, created_by: warehouse!.userId, posted_by: finance!.userId });
      const afterResponse = await warehouseContext.request.get(`/api/v1/holdings/simulation/commerce/orders/${orderId}`);
      expect(afterResponse.ok(), await afterResponse.text()).toBe(true);
      const after = (await afterResponse.json()).data as ReceiptDetail;
      expect(after.fulfillments).toHaveLength(1);
      expect(after.fulfillments[0]).toMatchObject({ id: receipt.id, journal_id: receipt.journal_id, stock_journal_status: "posted" });
      expect(after.fulfillmentLines).toEqual(before.fulfillmentLines);
      expect(after.lotAllocations).toEqual(before.lotAllocations);
      expect(after.lots).toEqual(before.lots);
      postedReceipts.push({ orderId, fulfillmentId: receipt.id, journalId: receipt.journal_id, valueMinor });
    }
    await warehousePage.reload({ waitUntil: "domcontentloaded" });
    await expect(lotRow).toContainText("lot");
    await expect(lotRow).toContainText("2");
    await expect(serialRow).toContainText("serial");
    await expect(serialRow).toContainText("1");
    await qualityPage.reload({ waitUntil: "domcontentloaded" });
    const qualityAfterResponse = await qualityContext.request.get("/api/v1/holdings/simulation/quality");
    expect(qualityAfterResponse.ok(), await qualityAfterResponse.text()).toBe(true);
    expect((await qualityAfterResponse.json()).data.holds).toEqual(qualityData.holds);
    await testInfo.attach("inventory-lot-serial-result", {
      contentType: "application/json",
      body: JSON.stringify({ run, lotOrderId, serialOrderId, lotCode: "DEMO-LOT-2026-01", serialNumber: "DEMO-SERIAL-2026-01", lotDisposition: "quarantine", serialDisposition: "available", postedReceipts }, null, 2),
    });
  } finally {
    await Promise.all([buyerContext.close(), approverContext.close(), warehouseContext.close(), qualityContext.close(), financeContext.close()]);
  }
});
