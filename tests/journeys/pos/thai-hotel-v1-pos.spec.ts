import { captureInvestorCheckpoint } from "../../support/helpers/investor-capture";
import { expect, test } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { Pool } from "pg";
import { frontOrigin, frontWorkspacePath, signInFrontRole, type SimulatedFrontCredentials } from "../../support/helpers/front-simulation-session";

type DemoState = { fixture: { hospitality: { companyId: string; propertyId: string; registerId: string; warehouseId: string; productIds: Record<string, string> } } };

async function assertInvestorMenuImages(page: import("@playwright/test").Page) {
  if (process.env.ERPX_INVESTOR_CAPTURE !== "1") return;
  const productsTab = page.getByRole("button", { name: "Products", exact: true });
  if (await productsTab.isVisible()) await productsTab.click();
  for (const name of ["Basil chicken rice", "Vegetable fried rice", "Stir-fried noodles", "Thai iced tea", "Iced coffee", "Bottled water"]) {
    const product = page.getByRole("button", { name: new RegExp(name) });
    await product.scrollIntoViewIfNeeded();
    const image = product.getByRole("img", { name, exact: true });
    await expect(image).toBeVisible();
    await expect.poll(() => image.evaluate(element => (element as HTMLImageElement).complete && (element as HTMLImageElement).naturalWidth > 0), { message: `${name}: authenticated product thumbnail must load` }).toBe(true);
  }
}

async function closeInspectedSale(page: import("@playwright/test").Page, saleNumber: string) {
  const drawer = page.getByTestId("record-detail-drawer");
  await expect(drawer).toBeVisible();
  await expect(drawer.getByRole("heading", { name: saleNumber, level: 1, exact: true })).toBeVisible();
  await drawer.getByRole("button", { name: "Close dialog", exact: true }).click();
  await expect(drawer).toBeHidden();
}

async function signInHoldingRole(context: import("@playwright/test").BrowserContext, credentials: SimulatedFrontCredentials, role: string) {
  const rootOrigin = process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100";
  const user = credentials.users.find((item) => item.email.endsWith("@riverside-lantern.example") && (item.role === role || item.roles?.includes(role)));
  expect(user, `${role} Riverside account`).toBeTruthy();
  const login = await context.request.post(new URL("/api/auth/sign-in/email", rootOrigin).toString(), {
    headers: { Origin: rootOrigin },
    data: { email: user!.email, password: credentials.password },
  });
  expect(login.ok(), `${role} holding session`).toBe(true);
  const endpoint = new URL("/api/v1/holdings/riverside-lantern/assignments/active", rootOrigin).toString();
  const activeResponse = await context.request.get(endpoint);
  expect(activeResponse.ok(), await activeResponse.text()).toBe(true);
  const active = (await activeResponse.json()).data;
  const assignment = active.options.find((item: { role: string }) => item.role === role);
  expect(assignment, `${role} Riverside assignment`).toBeTruthy();
  if (active.assignmentId !== assignment.id) {
    const selected = await context.request.post(endpoint, {
      headers: { Origin: rootOrigin },
      data: { assignmentId: assignment.id, revision: active.revision },
    });
    expect(selected.ok(), await selected.text()).toBe(true);
  }
}

test("H002 restaurant posts a cash sale, records waste, reviews a refund and closes its cash session", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  expect(run, "The Thai Hotel mock runner must supply an exact prepared simulation run.").toMatch(/^[a-f0-9]{12}$/);
  test.setTimeout(240_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as SimulatedFrontCredentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const fixture = state.fixture.hospitality;
  const companyId = fixture.companyId;
  const posPath = (query = "") => frontWorkspacePath(companyId, query ? `?${query}` : "");
  const posUrl = (query = "") => `${frontOrigin("pos")}${posPath(query)}`;
  const pmsPath = (section: string) => frontWorkspacePath(companyId, section);
  const pmsUrl = (section: string) => `${frontOrigin("pms")}${pmsPath(section)}`;
  const cashier = await browser.newContext(testInfo.project.use);
  const kitchen = await browser.newContext(testInfo.project.use);
  const manager = await browser.newContext(testInfo.project.use);
  const pmsFrontDesk = await browser.newContext(testInfo.project.use);
  const pmsCashier = await browser.newContext(testInfo.project.use);
  [cashier, kitchen, manager, pmsFrontDesk, pmsCashier].forEach((context) => context.setDefaultTimeout(15_000));
  const pool = new Pool({ connectionString: (JSON.parse(await readFile(resolve(directory, "databases.json"), "utf8")) as { databases: { H002_DB: string } }).databases.H002_DB, max: 1 });
  try {
    const cashierPage = await cashier.newPage();
    const kitchenPage = await kitchen.newPage();
    const managerPage = await manager.newPage();
    const pmsFrontDeskPage = await pmsFrontDesk.newPage();
    const pmsCashierPage = await pmsCashier.newPage();
    await Promise.all([
      signInFrontRole(cashier, cashierPage, credentials, "pos_cashier", "pos", companyId, posPath("tab=shifts")),
      signInFrontRole(kitchen, kitchenPage, credentials, "pos_kitchen", "pos", companyId, posPath("tab=kitchen")),
      signInFrontRole(manager, managerPage, credentials, "pos_manager", "pos", companyId, posPath("tab=kitchen")),
      signInFrontRole(pmsFrontDesk, pmsFrontDeskPage, credentials, "pms_frontdesk", "pms", companyId, pmsPath(`front-desk?property=${fixture.propertyId}`)),
      signInFrontRole(pmsCashier, pmsCashierPage, credentials, "pms_cashier", "pms", companyId, pmsPath(`cashier?property=${fixture.propertyId}`)),
    ]);
    const posApi = "/api/v1/holdings/riverside-lantern/pos";
    const pmsApi = `/api/v1/holdings/riverside-lantern/pms?property=${fixture.propertyId}`;
    const pmsCommandPath = `/api/v1/front/riverside-lantern/${companyId}/pms`;
    const sendPms = async (page: typeof pmsFrontDeskPage, button: string) => {
      const response = page.waitForResponse((row) => row.request().method() === "POST" && row.url().endsWith(pmsCommandPath));
      await page.getByRole("button", { name: button, exact: true }).click();
      const result = await response;
      expect(result.ok(), `${button}: ${await result.text()}`).toBe(true);
      return result.json() as Promise<{ data: { id?: string; [key: string]: unknown } }>;
    };
    await cashierPage.goto(posUrl("tab=shifts"), { waitUntil: "domcontentloaded" });
    const initialShifts = (await (await cashier.request.get(`${posApi}?tab=shifts`)).json()).data.shifts as Array<{ id: string; state: string; register_id: string }>;
    let cashShift = initialShifts.find((item) => item.state === "open" && item.register_id === fixture.registerId);
    if (!cashShift) {
      const openResponse = cashierPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/shift/open"));
      await cashierPage.getByRole("button", { name: "Open cash shift", exact: true }).click();
      expect((await openResponse).ok()).toBe(true);
      const reopened = (await (await cashier.request.get(`${posApi}?tab=shifts`)).json()).data.shifts as typeof initialShifts;
      cashShift = reopened.find((item) => item.state === "open" && item.register_id === fixture.registerId);
    }
    expect(cashShift, "one open H002 restaurant cash session").toBeTruthy();
    await expect(cashierPage.getByRole("button", { name: "Close cash shift", exact: true })).toBeVisible();

    // A cook starts and wastes a fictional ticket. Ingredient movement remains in the inventory ledger.
    const stockBeforeWaste = await pool.query("SELECT p.sku,s.quantity_milli FROM commerce_stock s JOIN commerce_products p ON p.id=s.product_id WHERE s.warehouse_id=$1 AND p.sku IN ('RLH-CHICKEN','RLH-RICE') ORDER BY p.sku", [fixture.warehouseId]);
    await cashierPage.goto(posUrl(), { waitUntil: "domcontentloaded" });
    const restaurantTables = (await (await cashier.request.get(posApi)).json()).data.tables as Array<{ id: string; name: string }>;
    const tableId = restaurantTables.find((table) => table.name === "T01")?.id;
    expect(tableId, "the seeded restaurant floor includes table T01").toBeTruthy();
    const createWasteSale = cashierPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/sale/create"));
    await cashierPage.locator("details").filter({ has: cashierPage.locator("summary").getByText("Tables", { exact: true }) }).getByRole("button", { name: "T01 Available", exact: true }).click();
    expect((await createWasteSale).ok()).toBe(true);
    await cashierPage.getByRole("button", { name: /Basil chicken rice/ }).click();
    const addWasteLine = cashierPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/sale/add"));
    await cashierPage.getByRole("button", { name: "Add to cart", exact: true }).click();
    expect((await addWasteLine).ok()).toBe(true);
    let sales = (await (await cashier.request.get(posApi)).json()).data.sales as Array<{ id: string; state: string; number: string }>;
    const wasteSale = sales.find((item) => item.state === "open");
    expect(wasteSale).toBeTruthy();
    await cashierPage.locator("summary").filter({ hasText: /^Open \/ held sales \(\d+\)$/ }).click();
    const activeWasteSale = cashierPage.getByRole("button", { name: new RegExp(wasteSale!.number) });
    await expect(activeWasteSale).toBeVisible();
    await activeWasteSale.click();
    await closeInspectedSale(cashierPage, wasteSale!.number);
    const wasteDetail = (await (await cashier.request.get(`${posApi}?sale=${wasteSale!.id}`)).json()).data;
    expect(wasteDetail.sale).toMatchObject({ table_id: tableId, service_mode: "dine_in" });
    const fulfillmentId = String(wasteDetail.lines[0].fulfillment_id);
    await kitchenPage.goto(posUrl("tab=kitchen"), { waitUntil: "domcontentloaded" });
    const ticket = kitchenPage.locator("article").filter({ has: kitchenPage.getByRole("heading", { name: "Basil chicken rice × 1", exact: true }) });
    const sendTicket = kitchenPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/kitchen/send"));
    await ticket.getByRole("button", { name: "Send to kitchen", exact: true }).click();
    expect((await sendTicket).ok()).toBe(true);
    await kitchenPage.reload({ waitUntil: "domcontentloaded" });
    const cookingTicket = kitchenPage.locator("article").filter({ has: kitchenPage.getByRole("heading", { name: "Basil chicken rice × 1", exact: true }) });
    const startTicket = kitchenPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/kitchen/start"));
    await cookingTicket.getByRole("button", { name: "Start · consume ingredients", exact: true }).click();
    expect((await startTicket).ok()).toBe(true);
    await managerPage.goto(posUrl("tab=kitchen"), { waitUntil: "domcontentloaded" });
    const wasteTicket = managerPage.locator("article").filter({ has: managerPage.getByRole("heading", { name: "Basil chicken rice × 1", exact: true }) });
    await wasteTicket.getByText("Cancel / record waste", { exact: true }).click();
    const wasteReason = `Synthetic kitchen waste evidence ${run}`;
    await wasteTicket.getByLabel("Reason", { exact: true }).fill(wasteReason);
    const wasteResponse = managerPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/kitchen/cancel"));
    await wasteTicket.getByRole("button", { name: "Confirm cancellation", exact: true }).click();
    expect((await wasteResponse).ok()).toBe(true);
    await cashierPage.goto(posUrl(`sale=${wasteSale!.id}`), { waitUntil: "domcontentloaded" });
    const cancelWasteBill = cashierPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/sale/cancel"));
    await cashierPage.getByRole("button", { name: "Cancel sale", exact: true }).click();
    expect((await cancelWasteBill).ok()).toBe(true);
    const wasteMoves = await pool.query("SELECT p.sku,m.quantity_milli,m.value_minor FROM pos_stock_moves m JOIN commerce_products p ON p.id=m.product_id WHERE m.company_id=$1 AND m.fulfillment_id=$2 AND m.kind='kitchen' ORDER BY p.sku", [fixture.companyId, fulfillmentId]);
    expect(wasteMoves.rows).toHaveLength(2);
    expect(wasteMoves.rows.every((row) => Number(row.quantity_milli) < 0)).toBe(true);
    const wastePostings = await pool.query("SELECT COUNT(*)::int count FROM pos_postings WHERE company_id=$1 AND source_key=$2", [fixture.companyId, `waste:${fulfillmentId}`]);
    expect(wastePostings.rows[0].count).toBe(1);
    const stockAfterWaste = await pool.query("SELECT p.sku,s.quantity_milli FROM commerce_stock s JOIN commerce_products p ON p.id=s.product_id WHERE s.warehouse_id=$1 AND p.sku IN ('RLH-CHICKEN','RLH-RICE') ORDER BY p.sku", [fixture.warehouseId]);
    expect(stockAfterWaste.rows.map((row, index) => Number(row.quantity_milli) - Number(stockBeforeWaste.rows[index].quantity_milli))).toEqual(wasteMoves.rows.map((row) => Number(row.quantity_milli)));

    // A Front cashier can post the unpaid restaurant bill to the selected in-house hotel guest account.
    await cashierPage.goto(posUrl(), { waitUntil: "domcontentloaded" });
    const createRoomChargeSale = cashierPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/sale/create"));
    await cashierPage.getByRole("button", { name: "New sale / takeaway", exact: true }).click();
    expect((await createRoomChargeSale).ok()).toBe(true);
    await cashierPage.getByRole("button", { name: /Bottled water/ }).click();
    const addRoomChargeLine = cashierPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/sale/add"));
    await cashierPage.getByRole("button", { name: "Add to cart", exact: true }).click();
    expect((await addRoomChargeLine).ok()).toBe(true);
    sales = (await (await cashier.request.get(posApi)).json()).data.sales as typeof sales;
    const roomChargeSale = sales.find((item) => item.state === "open");
    expect(roomChargeSale).toBeTruthy();
    await cashierPage.locator("summary").filter({ hasText: /^Open \/ held sales \(\d+\)$/ }).click();
    await cashierPage.getByRole("button", { name: new RegExp(roomChargeSale!.number) }).click();
    await closeInspectedSale(cashierPage, roomChargeSale!.number);
    await cashierPage.locator("summary").getByText("Charge to room", { exact: true }).click();
    const roomChoices = cashierPage.locator('select[name="stay"]');
    expect(await roomChoices.locator("option").allTextContents()).toContain("202 · Existing Demo Guest");
    await roomChoices.selectOption({ label: "202 · Existing Demo Guest" });
    const roomChargeSaleBefore = (await (await cashier.request.get(`${posApi}?sale=${roomChargeSale!.id}`)).json()).data.sale;
    const hotelBeforeRoomCharge = (await (await pmsFrontDesk.request.get(pmsApi)).json()).data.snapshot;
    const inHouseRoomChargeStay = hotelBeforeRoomCharge.records.find((record: { kind: string; data: { state?: string } }) => record.kind === "stay" && record.data.state === "in_house");
    expect(inHouseRoomChargeStay, "the room charge targets a current in-house H002 stay").toBeTruthy();
    const roomChargeInput = {
      id: roomChargeSale!.id,
      expectedVersion: Number(roomChargeSaleBefore.version),
      propertyId: fixture.propertyId,
      stayId: String(inHouseRoomChargeStay.id),
      expectedPropertyVersion: Number(hotelBeforeRoomCharge.property.version),
    };
    const sendRoomChargeCommand = (input: typeof roomChargeInput, key: string) => cashierPage.evaluate(async ({ company, body, idempotencyKey }) => {
      const response = await fetch(`/api/v1/front/riverside-lantern/${encodeURIComponent(company)}/pos/sale/charge_to_room`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": idempotencyKey, "x-erpx-front-location": "holding" },
        body: JSON.stringify(body),
      });
      return { status: response.status, body: await response.json() };
    }, { company: companyId, body: input, idempotencyKey: key });
    const roomChargeBeforeRejectedCommands = await pool.query(
      "SELECT s.state,s.version,f.state fulfillment_state,cs.quantity_milli FROM pos_sales s JOIN pos_lines l ON l.sale_id=s.id JOIN pos_fulfillments f ON f.id=l.fulfillment_id JOIN commerce_stock cs ON cs.warehouse_id=$2 AND cs.product_id=l.product_id WHERE s.id=$1",
      [roomChargeSale!.id, fixture.warehouseId],
    );
    expect(roomChargeBeforeRejectedCommands.rows).toHaveLength(1);
    const wrongRoom = await sendRoomChargeCommand({ ...roomChargeInput, stayId: "missing-h002-stay" }, `wrong-room-${run}`);
    expect(wrongRoom.status, `wrong-room command is rejected: ${JSON.stringify(wrongRoom.body)}`).toBeGreaterThanOrEqual(400);
    const staleRoomCharge = await sendRoomChargeCommand({ ...roomChargeInput, expectedVersion: roomChargeInput.expectedVersion + 1 }, `stale-room-charge-${run}`);
    expect(staleRoomCharge.status, `stale room-charge command is rejected: ${JSON.stringify(staleRoomCharge.body)}`).toBeGreaterThanOrEqual(400);
    expect(await pool.query("SELECT count(*)::int count FROM pms_sources WHERE property_id=$1 AND source_key=$2", [fixture.propertyId, `pos:${roomChargeSale!.id}:${roomChargeSale!.id}`])).toMatchObject({ rows: [{ count: 0 }] });
    expect((await pool.query("SELECT state,version FROM pos_sales WHERE id=$1", [roomChargeSale!.id])).rows.map((row) => ({ state: row.state, version: Number(row.version) }))).toEqual([{ state: "open", version: roomChargeInput.expectedVersion }]);
    expect((await pool.query(
      "SELECT s.state,s.version,f.state fulfillment_state,cs.quantity_milli FROM pos_sales s JOIN pos_lines l ON l.sale_id=s.id JOIN pos_fulfillments f ON f.id=l.fulfillment_id JOIN commerce_stock cs ON cs.warehouse_id=$2 AND cs.product_id=l.product_id WHERE s.id=$1",
      [roomChargeSale!.id, fixture.warehouseId],
    )).rows).toEqual(roomChargeBeforeRejectedCommands.rows);
    const chargeResponse = cashierPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/sale/charge_to_room"));
    const chargeRequestPromise = cashierPage.waitForRequest((request) => request.method() === "POST" && request.url().endsWith("/pos/sale/charge_to_room"));
    await cashierPage.getByRole("button", { name: "Charge full bill", exact: true }).click();
    const [charged, chargeRequest] = await Promise.all([chargeResponse, chargeRequestPromise]);
    expect(charged.ok(), await charged.text()).toBe(true);
    const chargeResult = (await charged.json()).data;
    expect(chargeResult).toMatchObject({ id: roomChargeSale!.id, state: "completed", tender: "room_charge" });
    const roomChargeReplayHeaders = Object.fromEntries(Object.entries(await chargeRequest.allHeaders()).filter(([name]) => name === "idempotency-key" || name.startsWith("x-erpx-context-") || name === "x-erpx-front-location"));
    const roomChargeReplayInput = chargeRequest.postDataJSON() as Record<string, unknown>;
    const roomChargeReplayPath = new URL(chargeRequest.url()).pathname;
    const beforeRoomChargeReplay = {
      sources: await pool.query("SELECT count(*)::int count FROM pms_sources WHERE property_id=$1 AND source_key=$2", [fixture.propertyId, `pos:${roomChargeSale!.id}:${roomChargeSale!.id}`]),
      postings: await pool.query("SELECT count(*)::int count FROM pms_postings WHERE property_id=$1 AND record_id=$2", [fixture.propertyId, chargeResult.guestAccountEntryId]),
      moves: await pool.query("SELECT count(*)::int count FROM pos_stock_moves WHERE company_id=$1 AND kind='sale' AND fulfillment_id IN (SELECT fulfillment_id FROM pos_lines WHERE sale_id=$2)", [companyId, roomChargeSale!.id]),
    };
    const roomChargeReplay = await cashierPage.evaluate(async ({ path, headers, body }) => {
      const response = await fetch(path, { method: "POST", headers, body: JSON.stringify(body) });
      return { status: response.status, body: await response.json() };
    }, { path: roomChargeReplayPath, headers: roomChargeReplayHeaders, body: roomChargeReplayInput });
    expect(roomChargeReplay.status, JSON.stringify(roomChargeReplay.body)).toBe(charged.status());
    expect(roomChargeReplay.body.data).toEqual(chargeResult);
    expect(await pool.query("SELECT count(*)::int count FROM pms_sources WHERE property_id=$1 AND source_key=$2", [fixture.propertyId, `pos:${roomChargeSale!.id}:${roomChargeSale!.id}`])).toEqual(beforeRoomChargeReplay.sources);
    expect(await pool.query("SELECT count(*)::int count FROM pms_postings WHERE property_id=$1 AND record_id=$2", [fixture.propertyId, chargeResult.guestAccountEntryId])).toEqual(beforeRoomChargeReplay.postings);
    expect(await pool.query("SELECT count(*)::int count FROM pos_stock_moves WHERE company_id=$1 AND kind='sale' AND fulfillment_id IN (SELECT fulfillment_id FROM pos_lines WHERE sale_id=$2)", [companyId, roomChargeSale!.id])).toEqual(beforeRoomChargeReplay.moves);
    const persistedRoomCharge = (await (await cashier.request.get(`${posApi}?sale=${roomChargeSale!.id}`)).json()).data;
    expect(persistedRoomCharge.sale.state).toBe("completed");
    expect(persistedRoomCharge.roomCharge.guest_account_entry_id).toBe(chargeResult.guestAccountEntryId);
    await cashierPage.goto(posUrl(`sale=${roomChargeSale!.id}`), { waitUntil: "domcontentloaded" });
    await expect(cashierPage.getByRole("status").filter({ hasText: "Charged to hotel guest account" })).toBeVisible();
    const sourceKey = `pos:${roomChargeSale!.id}:${roomChargeSale!.id}`;
    const source = await pool.query("SELECT record_id FROM pms_sources WHERE property_id=$1 AND source_key=$2", [fixture.propertyId, sourceKey]);
    expect(source.rows).toEqual([{ record_id: chargeResult.guestAccountEntryId }]);
    const balancedPosting = await pool.query("SELECT COALESCE(SUM(l.debit_minor),0)::bigint debit,COALESCE(SUM(l.credit_minor),0)::bigint credit FROM pms_postings p JOIN finance_journal_lines l ON l.journal_id=p.journal_id WHERE p.property_id=$1 AND p.record_id=$2", [fixture.propertyId, chargeResult.guestAccountEntryId]);
    expect(balancedPosting.rows[0].debit).toBe(balancedPosting.rows[0].credit);
    expect(Number(balancedPosting.rows[0].debit)).toBeGreaterThan(0);
    expect(await pool.query("SELECT COUNT(*)::int count FROM pos_payments WHERE sale_id=$1", [roomChargeSale!.id])).toMatchObject({ rows: [{ count: 0 }] });
    expect(await pool.query("SELECT COUNT(*)::int count FROM pos_postings WHERE source_key IN ($1,$2)", [`sale:${roomChargeSale!.id}`, `receipt:${roomChargeSale!.id}`])).toMatchObject({ rows: [{ count: 0 }] });

    // Hotel cashier settles the same guestAccount, then front desk checks out the same in-house guest.
    await pmsFrontDeskPage.goto(pmsUrl(`front-desk?property=${fixture.propertyId}`), { waitUntil: "domcontentloaded" });
    const checkoutPrepHeading = pmsFrontDeskPage.getByRole("heading", { name: "Existing Demo Guest", exact: true }).first();
    await expect(checkoutPrepHeading).toBeVisible();
    const checkoutPrepCard = checkoutPrepHeading.locator("xpath=../..");
    const checkoutPrepDetails = checkoutPrepCard.locator("details").filter({ hasText: "Post room charge before check-out" });
    await checkoutPrepDetails.locator("summary").click();
    await sendPms(pmsFrontDeskPage, "Post room charge before check-out");
    const roomChargeSnapshot = (await (await pmsCashier.request.get(pmsApi)).json()).data.snapshot;
    const roomChargeEntry = roomChargeSnapshot.records.find((record: { kind: string; id: string }) => record.kind === "entry" && record.id === chargeResult.guestAccountEntryId);
    expect(roomChargeEntry, "the POS room charge appears in the PMS guest account ledger").toBeTruthy();
    const roomChargeGuestAccountId = String(roomChargeEntry.data.guestAccountId);
    const roomChargeGuestAccount = roomChargeSnapshot.records.find((record: { kind: string; id: string }) => record.kind === "guest_account" && record.id === roomChargeGuestAccountId);
    expect(roomChargeGuestAccount).toBeTruthy();
    expect(roomChargeGuestAccount.data.due).toBeGreaterThan(0);
    const roomChargeStay = roomChargeSnapshot.records.find((record: { kind: string; data: Record<string, unknown> }) => record.kind === "stay" && record.data.guestAccountId === roomChargeGuestAccountId);
    expect(roomChargeStay?.data.state).toBe("in_house");
    const chargedGuest = roomChargeSnapshot.records.find((record: { kind: string; id: string }) => record.kind === "guest" && record.id === roomChargeStay.data.guestId);
    expect(chargedGuest?.data.name).toBe("Existing Demo Guest");

    await pmsCashierPage.goto(pmsUrl(`cashier?property=${fixture.propertyId}`), { waitUntil: "domcontentloaded" });
    let cashierPmsSnapshot = (await (await pmsCashier.request.get(pmsApi)).json()).data.snapshot;
    const pmsCashierUserId = credentials.users.find((user) => user.role === "pms_cashier" && user.email.endsWith("@riverside-lantern.example"))!.userId;
    let pmsCashShift = cashierPmsSnapshot.records.find((record: { kind: string; data: Record<string, unknown> }) => record.kind === "shift" && record.data.userId === pmsCashierUserId && record.data.state === "open");
    if (!pmsCashShift) {
      const openShiftForm = pmsCashierPage.locator("details").filter({ hasText: "Open cash shift" }).first();
      await openShiftForm.locator("summary").click();
      await openShiftForm.locator('input[name="name"]').fill(`POS room charge checkout ${run}`);
      await openShiftForm.locator('input[name="opening"]').fill("0");
      await sendPms(pmsCashierPage, "Open cash shift");
      cashierPmsSnapshot = (await (await pmsCashier.request.get(pmsApi)).json()).data.snapshot;
      pmsCashShift = cashierPmsSnapshot.records.find((record: { kind: string; data: Record<string, unknown> }) => record.kind === "shift" && record.data.userId === pmsCashierUserId && record.data.state === "open");
    }
    expect(pmsCashShift, "hotel cashier has an open cash drawer").toBeTruthy();
    await pmsCashierPage.goto(pmsUrl(`guestAccounts?property=${fixture.propertyId}&guestAccount=${roomChargeGuestAccountId}`), { waitUntil: "domcontentloaded" });
    const receiveRoomCharge = pmsCashierPage.locator("details").filter({ hasText: "Receive payment / advance" }).first();
    await receiveRoomCharge.locator("summary").click();
    const receiveDrawer = pmsCashierPage.getByRole("dialog", { name: "Receive payment / advance", exact: true });
    await expect(receiveDrawer).toBeVisible();
    const receiveForm = receiveDrawer.locator("form");
    await expect(receiveForm).toHaveCount(1);
    await expect(receiveForm.getByRole("button", { name: "Receive payment / advance", exact: true })).toHaveAttribute("data-client-managed-ready", "true");
    await receiveForm.locator('input[name="amount"]').fill((Number(roomChargeGuestAccount.data.due) / 100).toFixed(2));
    await receiveForm.locator('select[name="method"]').selectOption("cash");
    await receiveForm.locator('select[name="shiftId"]').selectOption(String(pmsCashShift!.id));
    await sendPms(pmsCashierPage, "Receive payment / advance");
    const paidRoomChargeGuestAccount = (await (await pmsCashier.request.get(pmsApi)).json()).data.snapshot.records.find((record: { kind: string; id: string }) => record.kind === "guest_account" && record.id === roomChargeGuestAccountId);
    expect(paidRoomChargeGuestAccount?.data.due).toBe(0);

    await pmsFrontDeskPage.goto(pmsUrl(`front-desk?property=${fixture.propertyId}`), { waitUntil: "domcontentloaded" });
    const guestStayHeading = pmsFrontDeskPage.getByRole("heading", { name: "Existing Demo Guest", exact: true }).first();
    await expect(guestStayHeading).toBeVisible();
    const guestStayCard = guestStayHeading.locator("xpath=../..");
    const checkOutDetails = guestStayCard.locator("details").filter({ hasText: "Check out" });
    await checkOutDetails.locator("summary").click();
    await sendPms(pmsFrontDeskPage, "Check out");
    const checkedOutSnapshot = (await (await pmsFrontDesk.request.get(pmsApi)).json()).data.snapshot;
    const checkedOutStay = checkedOutSnapshot.records.find((record: { kind: string; data: Record<string, unknown> }) => record.kind === "stay" && record.data.guestAccountId === roomChargeGuestAccountId);
    expect(checkedOutStay?.data.state).toBe("checked_out");
    const persistedPosEntry = checkedOutSnapshot.records.find((record: { kind: string; id: string }) => record.kind === "entry" && record.id === chargeResult.guestAccountEntryId);
    expect(persistedPosEntry, "checkout retains the source POS guest account charge").toBeTruthy();
    const settledGuestAccount = checkedOutSnapshot.records.find((record: { kind: string; id: string }) => record.kind === "guest_account" && record.id === roomChargeGuestAccountId);
    expect(settledGuestAccount?.data.due).toBe(0);

    // Cash sale is confirmed once, then an independent manager records a synthetic full return.
    const waterBefore = await pool.query("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2", [fixture.warehouseId, fixture.productIds.WATER]);
    await cashierPage.goto(posUrl(), { waitUntil: "domcontentloaded" });
    const createCashSaleRequest = cashierPage.waitForRequest((request) => request.method() === "POST" && request.url().endsWith("/pos/sale/create"));
    await cashierPage.getByRole("button", { name: "New sale / takeaway", exact: true }).click();
    const createCashSale = await createCashSaleRequest;
    expect(createCashSale.postDataJSON()).toBeTruthy();
    const saleCreateResponse = await createCashSale.response();
    if (!saleCreateResponse) throw new Error("CASH_SALE_RESPONSE_REQUIRED");
    expect(saleCreateResponse.ok()).toBe(true);
    await cashierPage.getByRole("button", { name: /Bottled water/ }).click();
    const addCashLine = cashierPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/sale/add"));
    await cashierPage.getByRole("button", { name: "Add to cart", exact: true }).click();
    expect((await addCashLine).ok()).toBe(true);
    sales = (await (await cashier.request.get(posApi)).json()).data.sales as typeof sales;
    const cashSale = sales.find((item) => item.state === "open");
    expect(cashSale).toBeTruthy();
    await cashierPage.locator("summary").filter({ hasText: /^Open \/ held sales \(\d+\)$/ }).click();
    const activeCashSale = cashierPage.getByRole("button", { name: new RegExp(cashSale!.number) });
    await expect(activeCashSale).toBeVisible();
    await activeCashSale.click();
    await closeInspectedSale(cashierPage, cashSale!.number);
    const cashSaleDetailBeforePayment = (await (await cashier.request.get(`${posApi}?sale=${cashSale!.id}`)).json()).data;
    await expect(cashierPage.getByLabel("Amount · split tender", { exact: true })).toHaveJSProperty("valueAsNumber", Number(cashSaleDetailBeforePayment.sale.total_minor) / 100);
    const paymentMethod = cashierPage.getByRole("combobox", { name: "Payment method", exact: true });
    await expect(paymentMethod).toBeVisible();
    await paymentMethod.selectOption("cash");
    await cashierPage.getByLabel("Cash tendered", { exact: true }).fill("");
    const paymentRequest = cashierPage.waitForRequest((request) => request.method() === "POST" && request.url().endsWith("/pos/payment/create"));
    await cashierPage.getByRole("button", { name: "Record payment", exact: true }).click();
    const paymentCommand = await paymentRequest;
    const paymentResponse = await paymentCommand.response();
    if (!paymentResponse) throw new Error("CASH_PAYMENT_RESPONSE_REQUIRED");
    expect(paymentResponse.ok()).toBe(true);
    const paymentHeaders = await paymentCommand.allHeaders();
    const replayHeaders = Object.fromEntries(Object.entries(paymentHeaders).filter(([name]) => name === "idempotency-key" || name.startsWith("x-erpx-context-")));
    const paymentBody = paymentCommand.postDataJSON() as Record<string, unknown>;
    const paymentPath = new URL(paymentCommand.url()).pathname;
    const replay = await cashierPage.evaluate(async ({ path, headers, body }) => {
      const response = await fetch(path, { method: "POST", headers, body: JSON.stringify(body) });
      return { ok: response.ok, text: await response.text() };
    }, { path: paymentPath, headers: replayHeaders, body: paymentBody });
    expect(replay.ok, replay.text).toBe(true);
    let saleDetail = (await (await cashier.request.get(`${posApi}?sale=${cashSale!.id}`)).json()).data;
    expect(saleDetail.payments).toHaveLength(1);
    expect(saleDetail.payments[0]).toMatchObject({ method: "cash", state: "succeeded" });
    await expect(cashierPage.getByRole("button", { name: "Complete sale", exact: true })).toBeEnabled();
    await assertInvestorMenuImages(cashierPage);
    if (testInfo.project.name === "mobile") {
      await cashierPage.getByRole("button", { name: /^Cart ·/ }).click();
      await cashierPage.getByRole("button", { name: "Complete sale", exact: true }).scrollIntoViewIfNeeded();
      await expect(cashierPage.getByRole("button", { name: "Complete sale", exact: true })).toBeInViewport();
    }
    await captureInvestorCheckpoint(cashierPage, "pos-sale", cashierPage.getByRole("button", { name: "Complete sale", exact: true }));
    const complete = cashierPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/sale/complete"));
    await cashierPage.getByRole("button", { name: "Complete sale", exact: true }).click();
    expect((await complete).ok()).toBe(true);
    saleDetail = (await (await cashier.request.get(`${posApi}?sale=${cashSale!.id}`)).json()).data;
    expect(saleDetail.sale.state).toBe("completed");
    const refundReason = `Synthetic Riverside cash return ${run}`;
    await cashierPage.locator("summary").getByText("Partial return / refund", { exact: true }).click();
    const refundDrawer = cashierPage.getByRole("dialog", { name: "Partial return / refund", exact: true });
    await expect(refundDrawer).toBeVisible();
    const refundForm = refundDrawer.locator("form");
    await expect(refundForm).toHaveCount(1);
    await refundForm.locator('select[name="shift"]').selectOption(cashShift!.id);
    await refundForm.locator('input[name="reason"]').fill(refundReason);
    expect(saleDetail.lines).toHaveLength(1);
    const returnedLine = saleDetail.lines[0];
    await refundForm.locator(`input[name="qty:${returnedLine.id}"]`).fill(String((Number(returnedLine.quantity_milli) - Number(returnedLine.returned_milli)) / 1000));
    await refundForm.locator(`input[name="restock:${returnedLine.id}"]`).check();
    const refundRequest = cashierPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/refund/request"));
    await refundForm.getByRole("button", { name: "Request independent approval", exact: true }).click();
    const refundCreated = await refundRequest;
    expect(refundCreated.ok(), await refundCreated.text()).toBe(true);
    const createdRefundId = String((await refundCreated.json()).data.id);
    await managerPage.goto(posUrl("tab=history"), { waitUntil: "domcontentloaded" });
    const managerHistory = (await (await manager.request.get(`${posApi}?tab=history`)).json()).data;
    expect(managerHistory.permissions["pos.approve"]).toBe(true);
    expect(managerHistory.refunds.find((item: { id: string; state: string; reason: string }) => item.reason === refundReason)).toMatchObject({ state: "requested" });
    const refundWidget = managerPage.locator("#pos-refund-requests");
    await expect(refundWidget).toBeVisible();
    const refundReasonText = refundWidget.getByText(refundReason, { exact: true });
    await expect(refundReasonText).toBeVisible();
    const refundRow = refundReasonText.locator("xpath=../..");
    const refundRowText = await refundRow.innerText();
    expect(refundRowText).toContain(refundReason);
    const reviewInErpx = refundRow.getByRole("link", { name: "Review in ERPX", exact: true });
    await expect(reviewInErpx).toBeVisible();
    await reviewInErpx.click();
    await expect(managerPage).toHaveURL(/\/holdings\/riverside-lantern\/pos\?tab=history&queue=approval/);
    const approvalRefundWidget = managerPage.locator("#workspace-content #pos-refund-requests");
    await expect(approvalRefundWidget).toBeVisible();
    const approvalRefundText = approvalRefundWidget.getByText(refundReason, { exact: true });
    await expect(approvalRefundText).toBeVisible();
    const approvalRefundRow = approvalRefundText.locator("xpath=../..");
    expect(await approvalRefundRow.locator("form").count()).toBe(1);
    const refundEvidence = approvalRefundRow.locator('input[name="reason"]');
    await expect(refundEvidence).toBeVisible();
    await captureInvestorCheckpoint(managerPage, "pos-refund", approvalRefundRow);
    await refundEvidence.fill("Returned fictional cash to the open Riverside drawer under manager review.");
    const refundApproval = managerPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/refund/approve"));
    await approvalRefundRow.getByRole("button", { name: "Approve and confirm refund", exact: true }).click();
    expect((await refundApproval).ok()).toBe(true);
    const completedRefunds = (await (await manager.request.get(`${posApi}?tab=history`)).json()).data.refunds as Array<{ id: string; state: string; amount_minor: number }>;
    expect(completedRefunds.find((item) => item.id === createdRefundId && item.state === "completed" && item.amount_minor === saleDetail.sale.total_minor)).toBeTruthy();
    saleDetail = (await (await cashier.request.get(`${posApi}?sale=${cashSale!.id}`)).json()).data;
    expect(saleDetail.payments[0].refunded_minor).toBe(saleDetail.sale.total_minor);
    const waterAfter = await pool.query("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2", [fixture.warehouseId, fixture.productIds.WATER]);
    expect(waterAfter.rows).toEqual(waterBefore.rows);
    const refundLedger = await pool.query("SELECT COUNT(*)::int count FROM pos_postings WHERE company_id=$1 AND source_key=$2", [fixture.companyId, `refund:${createdRefundId}`]);
    expect(refundLedger.rows[0].count).toBe(1);
    const refundPaymentLedger = await pool.query("SELECT COUNT(*)::int count FROM finance_journals WHERE company_id=$1 AND source_type='pos' AND source_id=$2 AND memo LIKE 'refund-payment:%' AND status='posted'", [fixture.companyId, createdRefundId]);
    expect(refundPaymentLedger.rows[0].count).toBe(1);

    await cashierPage.goto(posUrl("tab=shifts"), { waitUntil: "domcontentloaded" });
    const openShift = (await (await cashier.request.get(`${posApi}?tab=shifts`)).json()).data.shifts.find((item: { id: string }) => item.id === cashShift!.id);
    expect(openShift.state).toBe("open");
    await cashierPage.getByLabel("Enter counted total", { exact: true }).check();
    await cashierPage.getByLabel("Counted cash", { exact: true }).fill((Number(openShift.cash_expected_minor) / 100).toFixed(2));
    const closeShift = cashierPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/shift/close"));
    await cashierPage.getByRole("button", { name: "Close cash shift", exact: true }).click();
    expect((await closeShift).ok()).toBe(true);
    const closedShift = (await (await cashier.request.get(`${posApi}?tab=shifts`)).json()).data.shifts.find((item: { id: string }) => item.id === cashShift!.id);
    expect(closedShift).toMatchObject({ state: "closed", variance_minor: 0 });
    expect(Number(closedShift.counted_minor)).toBe(Number(openShift.cash_expected_minor));
    expect(Number(closedShift.expected_minor)).toBe(Number(closedShift.counted_minor));
    await cashierPage.reload({ waitUntil: "domcontentloaded" });
    await expect(cashierPage.getByRole("button", { name: "Close cash shift", exact: true })).toHaveCount(0);
    await captureInvestorCheckpoint(cashierPage, "pos-shift");
  } finally {
    await Promise.all([cashier.close(), kitchen.close(), manager.close(), pmsFrontDesk.close(), pmsCashier.close(), pool.end()]);
  }
});

test("H002 restaurant transfer stays unsettled until independent Finance matches its imported bank row", async ({ browser }, testInfo) => {
  const run = process.env.ERPX_DEMO_RUN;
  expect(run, "The Thai Hotel mock runner must supply an exact prepared simulation run.").toMatch(/^[a-f0-9]{12}$/);
  test.setTimeout(180_000);
  const directory = resolve(`.runtime/browser/${run}`);
  const credentials = JSON.parse(await readFile(resolve(directory, "credentials.json"), "utf8")) as SimulatedFrontCredentials;
  const state = JSON.parse(await readFile(resolve(directory, "state.json"), "utf8")) as DemoState;
  const fixture = state.fixture.hospitality;
  const companyId = fixture.companyId;
  const posPath = (query = "") => frontWorkspacePath(companyId, query ? `?${query}` : "");
  const posUrl = (query = "") => `${frontOrigin("pos")}${posPath(query)}`;
  const cashier = await browser.newContext(testInfo.project.use);
  const financePreparer = await browser.newContext(testInfo.project.use);
  const financeApprover = await browser.newContext(testInfo.project.use);
  [cashier, financePreparer, financeApprover].forEach((context) => context.setDefaultTimeout(15_000));
  const databaseUrl = (JSON.parse(await readFile(resolve(directory, "databases.json"), "utf8")) as { databases: { H002_DB: string } }).databases.H002_DB;
  const pool = new Pool({ connectionString: databaseUrl, max: 1 });
  try {
    const cashierPage = await cashier.newPage();
    await Promise.all([
      signInFrontRole(cashier, cashierPage, credentials, "pos_cashier", "pos", companyId, posPath("tab=shifts")),
      signInHoldingRole(financePreparer, credentials, "finance_preparer"),
      signInHoldingRole(financeApprover, credentials, "finance_approver"),
    ]);

    const posApi = "/api/v1/holdings/riverside-lantern/pos";
    const shiftData = (await (await cashier.request.get(`${posApi}?tab=shifts`)).json()).data;
    let shift = shiftData.shifts.find((item: { state: string; register_id: string }) => item.state === "open" && item.register_id === fixture.registerId);
    if (!shift) {
      const openShift = cashierPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/shift/open"));
      await cashierPage.getByRole("button", { name: "Open cash shift", exact: true }).click();
      expect((await openShift).ok()).toBe(true);
      shift = ((await (await cashier.request.get(`${posApi}?tab=shifts`)).json()).data.shifts as Array<{ id: string; state: string; register_id: string }>).find((item) => item.state === "open" && item.register_id === fixture.registerId);
    }
    expect(shift, "one open H002 restaurant cash session").toBeTruthy();

    const poolStockBefore = await pool.query("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2", [fixture.warehouseId, fixture.productIds.WATER]);
    await cashierPage.goto(posUrl(), { waitUntil: "domcontentloaded" });
    const createSale = cashierPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/sale/create"));
    await cashierPage.getByRole("button", { name: "New sale / takeaway", exact: true }).click();
    const created = await createSale;
    expect(created.ok(), await created.text()).toBe(true);
    const saleId = String((await created.json()).data.id);
    await cashierPage.getByRole("button", { name: /Bottled water/ }).click();
    const addLine = cashierPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/sale/add"));
    await cashierPage.getByRole("button", { name: "Add to cart", exact: true }).click();
    expect((await addLine).ok()).toBe(true);

    const saleBeforePayment = (await (await cashier.request.get(`${posApi}?sale=${saleId}`)).json()).data;
    const amount = Number(saleBeforePayment.sale.total_minor);
    expect(amount).toBeGreaterThan(0);
    await expect(cashierPage.getByLabel("Amount · split tender", { exact: true })).toHaveJSProperty("valueAsNumber", amount / 100);
    const transferReference = `SANDBOX-POS-${crypto.randomUUID()}`;
    const paymentMethod = cashierPage.getByRole("combobox", { name: "Payment method", exact: true });
    await paymentMethod.selectOption("transfer");
    await expect(paymentMethod).toHaveValue("transfer");
    await cashierPage.getByLabel("Reference", { exact: true }).fill(transferReference);
    await expect(paymentMethod).toHaveValue("transfer");
    const recordTransfer = cashierPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/payment/create"));
    await cashierPage.getByRole("button", { name: "Record payment", exact: true }).click();
    const transferResponse = await recordTransfer;
    expect(transferResponse.ok(), await transferResponse.text()).toBe(true);
    expect(transferResponse.request().postDataJSON()).toMatchObject({ method: "transfer", reference: transferReference });
    await expect(cashierPage.getByText("Bank transfer recorded — awaiting verification", { exact: true })).toBeVisible();
    await expect(cashierPage.getByRole("button", { name: "Verify transfer", exact: true })).toHaveCount(0);
    await expect(cashierPage.getByRole("button", { name: "Complete sale", exact: true })).toBeDisabled();
    await expect(cashierPage.getByText("Waiting for verified payment. Complete the sale when payment is confirmed.", { exact: true })).toBeVisible();
    await assertInvestorMenuImages(cashierPage);
    await captureInvestorCheckpoint(cashierPage, "pos-pending", cashierPage.getByText("Bank transfer recorded — awaiting verification", { exact: true }));
    const pending = (await (await cashier.request.get(`${posApi}?sale=${saleId}`)).json()).data;
    expect(pending.sale.state).toBe("payment_pending");
    expect(pending.payments).toHaveLength(1);
    expect(pending.payments[0]).toMatchObject({ method: "transfer", reference: transferReference, state: "pending", amount_minor: amount });
    const pendingPayment = pending.payments[0];
    const stockWhilePending = await pool.query("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2", [fixture.warehouseId, fixture.productIds.WATER]);
    expect(stockWhilePending.rows).toEqual(poolStockBefore.rows);

    const bankAccount = await pool.query("SELECT accounts_json::jsonb->>'bank' bank FROM pos_settings WHERE company_id=$1", [companyId]);
    expect(bankAccount.rows[0].bank).toBeTruthy();
    const statementNumber = `SANDBOX-POS-${crypto.randomUUID()}`;
    const preparerPage = await financePreparer.newPage();
    await preparerPage.goto("/holdings/riverside-lantern/finance/bank?new=1");
    const importForm = preparerPage.locator("form").filter({ has: preparerPage.getByRole("button", { name: "Validate and import", exact: true }) });
    await expect(importForm).toHaveCount(1);
    await expect(importForm).toBeVisible();
    await expect(importForm.getByRole("button", { name: "Validate and import", exact: true })).toHaveAttribute("data-client-managed-ready", "true");
    await preparerPage.locator('select[name="bankAccount"]:visible').selectOption(bankAccount.rows[0].bank);
    await preparerPage.getByLabel("Statement number", { exact: true }).fill(statementNumber);
    await preparerPage.getByLabel("Statement date", { exact: true }).fill(saleBeforePayment.sale.business_date);
    await preparerPage.getByLabel("Opening balance", { exact: true }).fill("0");
    await preparerPage.getByLabel("Closing balance", { exact: true }).fill((amount / 100).toFixed(2));
    await preparerPage.getByLabel("Bank reference 1", { exact: true }).fill(transferReference);
    await preparerPage.getByLabel("Transaction date 1", { exact: true }).fill(saleBeforePayment.sale.business_date);
    await preparerPage.getByLabel("Description 1", { exact: true }).fill("Fictional restaurant guest transfer");
    await preparerPage.getByLabel("Signed amount 1", { exact: true }).fill((amount / 100).toFixed(2));
    const importCommand = preparerPage.waitForRequest((request) => request.method() === "POST" && request.url().endsWith("/finance/bank/statements/import"));
    const importResponse = preparerPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/finance/bank/statements/import"));
    await preparerPage.getByRole("button", { name: "Validate and import", exact: true }).click();
    const importedResponse = await importResponse;
    const originalImport = await importCommand;
    expect(importedResponse.ok()).toBe(true);
    const importResult = (await importedResponse.json()).data;
    const importHeaders = await originalImport.allHeaders();
    const importReplayHeaders = Object.fromEntries(Object.entries(importHeaders).filter(([name]) => name === "idempotency-key" || name.startsWith("x-erpx-context-")));
    const importReplay = await preparerPage.evaluate(async ({ path, headers, body }) => {
      const response = await fetch(path, { method: "POST", headers, body: JSON.stringify(body) });
      return { ok: response.ok, text: await response.text() };
    }, { path: new URL(originalImport.url()).pathname, headers: importReplayHeaders, body: originalImport.postDataJSON() });
    expect(importReplay.ok, importReplay.text).toBe(true);
    expect(JSON.parse(importReplay.text).data.id).toBe(importResult.id);
    const imported = await pool.query("SELECT t.id,t.version FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE s.company_id=$1 AND t.external_id=$2", [companyId, transferReference]);
    expect(imported.rows).toHaveLength(1);
    const importCounts = await pool.query("SELECT (SELECT COUNT(*)::int FROM finance_bank_statements WHERE company_id=$1 AND bank_account=$2 AND statement_number=$3) statements,(SELECT COUNT(*)::int FROM finance_bank_transactions WHERE company_id=$1 AND external_id=$4) transactions", [companyId, bankAccount.rows[0].bank, statementNumber, transferReference]);
    expect(importCounts.rows[0]).toEqual({ statements: 1, transactions: 1 });

    const financePage = await financeApprover.newPage();
    await financePage.goto(`/holdings/riverside-lantern/pos?tab=history&queue=approval&sale=${saleId}`);
    const receiptRow = financePage.getByRole("row").filter({ hasText: saleBeforePayment.sale.number });
    await receiptRow.getByRole("button", { name: "Open receipt", exact: true }).click();
    const financeReceipt = financePage.getByTestId("record-detail-drawer");
    await expect(financeReceipt).toBeVisible();
    await expect(financeReceipt.getByRole("heading", { name: saleBeforePayment.sale.number, level: 1, exact: true })).toBeVisible();
    await expect(financeReceipt.getByRole("button", { name: "Verify transfer", exact: true })).toHaveAttribute("data-client-managed-ready", "true");
    const verifyRow = financeReceipt.locator('select[name="bank"]');
    await expect(verifyRow).toBeVisible();
    await verifyRow.selectOption(imported.rows[0].id);
    const verification = financePage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/payment/verify_transfer"));
    await financeReceipt.getByRole("button", { name: "Verify transfer", exact: true }).click();
    const verified = await verification;
    expect(verified.ok(), await verified.text()).toBe(true);
    expect((await verified.json()).data).toMatchObject({ id: pendingPayment.id, saleId, state: "succeeded", bankTransactionId: imported.rows[0].id });
    const paymentState = await pool.query("SELECT state,finance_payment_id FROM pos_payments WHERE id=$1", [pendingPayment.id]);
    expect(paymentState.rows[0].state).toBe("succeeded");
    expect(paymentState.rows[0].finance_payment_id).toBeTruthy();
    const bankState = await pool.query("SELECT match_status,matched_type,matched_id FROM finance_bank_transactions WHERE id=$1", [imported.rows[0].id]);
    expect(bankState.rows[0]).toMatchObject({ match_status: "matched", matched_type: "payment", matched_id: paymentState.rows[0].finance_payment_id });

    await cashierPage.goto(posUrl(`sale=${saleId}`), { waitUntil: "domcontentloaded" });
    await expect(cashierPage.getByText("succeeded", { exact: true })).toBeVisible();
    await expect(cashierPage.getByRole("button", { name: "Complete sale", exact: true })).toBeEnabled();
    await expect(cashierPage.getByText("Waiting for verified payment. Complete the sale when payment is confirmed.", { exact: true })).toHaveCount(0);
    await assertInvestorMenuImages(cashierPage);
    await captureInvestorCheckpoint(cashierPage, "pos-verified", cashierPage.getByRole("button", { name: "Complete sale", exact: true }));
    const closeSale = cashierPage.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/pos/sale/complete"));
    await cashierPage.getByRole("button", { name: "Complete sale", exact: true }).click();
    expect((await closeSale).ok()).toBe(true);
    const completed = (await (await cashier.request.get(`${posApi}?sale=${saleId}`)).json()).data;
    expect(completed.sale.state).toBe("completed");
    expect(completed.payments).toHaveLength(1);
    const finalStock = await pool.query("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2", [fixture.warehouseId, fixture.productIds.WATER]);
    expect(Number(poolStockBefore.rows[0].quantity_milli) - Number(finalStock.rows[0].quantity_milli)).toBe(1000);
    const movements = await pool.query("SELECT m.quantity_milli FROM pos_stock_moves m JOIN pos_lines l ON l.fulfillment_id=m.fulfillment_id WHERE m.company_id=$1 AND l.sale_id=$2 AND m.kind='sale'", [companyId, saleId]);
    expect(movements.rows).toHaveLength(1);
    expect(Number(movements.rows[0].quantity_milli)).toBe(-1000);
    const receiptEntries = await pool.query("SELECT COUNT(*)::int count FROM finance_payments WHERE company_id=$1 AND id=$2 AND status='settled'", [companyId, paymentState.rows[0].finance_payment_id]);
    expect(receiptEntries.rows[0].count).toBe(1);
    const saleJournals = await pool.query("SELECT COUNT(*)::int count FROM pos_postings WHERE company_id=$1 AND source_key IN ($2,$3,$4)", [companyId, `receipt:${pendingPayment.id}`, `sale:${saleId}`, `allocate:${saleId}`]);
    expect(saleJournals.rows[0].count).toBe(3);
  } finally {
    await Promise.all([cashier.close(), financePreparer.close(), financeApprover.close(), pool.end()]);
  }
});
