import { fulfillmentSchema } from "../src/server/commerce";
import { afterAll, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { createHash } from "node:crypto";
import { applyMigrations, env } from "./runtime-env";
import { expenseDb as db, expenseFixture, key } from "./helpers/expense-fixture";
import type { WorkContext } from "../src/server/authorization";
import { loadIamAuthority } from "../src/server/iam-store";
import { assignFlowPrimary, resolveFlow } from "../src/server/flow-routing";
import { saveMaster, saveOrder, orderSchema, transitionOrder, getOrder, fulfillOrder } from "../src/server/commerce";
import { attachmentContent, attachmentLinkStatus, completeAttachmentUpload, createAttachmentSet, drainAttachmentScans, listAttachmentSets, reserveAttachmentFile } from "../src/server/attachments/service";
import { attachmentBindingState } from "./helpers/attachment-v2";

vi.mock("../src/server/runtime", () => ({ holdingResources: async () => ({ write: db, files: env.H001_FILES }), runtimeEnv: async () => env }));
// Declared scanner adapter; intake, MIME sniffing, preview, source binding and
// stock/GL commands are native. Compiled browser journeys use actual ClamAV.
vi.mock("../src/server/modules/files/public/attachments/scanner", () => ({
  attachmentScannerReadiness: async () => ({ enabled: true, ready: true, engine: "Declared delivery scanner fixture", definitionUpdatedAt: Date.now(), error: null }),
  scanAttachmentBytes: async () => ({ clean: true, signature: null, engine: "Declared delivery scanner fixture", definitionUpdatedAt: Date.now() }),
}));
const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=", "base64");
const bytes = png.buffer.slice(png.byteOffset, png.byteOffset + png.byteLength);
const priorGate = process.env.ERPX_ATTACHMENTS_V2;
let f: Awaited<ReturnType<typeof expenseFixture>>, warehouse: WorkContext, sales: WorkContext, orderId: string, lineId: string, warehouseId: string, productId: string, partyId: string;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); process.env.ERPX_ATTACHMENTS_V2 = "true"; });
afterAll(() => { if (priorGate === undefined) delete process.env.ERPX_ATTACHMENTS_V2; else process.env.ERPX_ATTACHMENTS_V2 = priorGate; });
beforeEach(async () => {
  f = await expenseFixture();
  warehouse = { ...f.claimant, role: "warehouse_manager", assignmentId: await f.grant(f.claimant, "warehouse_manager"), modules: ["inventory", "operations"] };
  sales = { ...f.head, role: "sales_manager", assignmentId: await f.grant(f.head, "sales_manager"), modules: ["sales", "inventory"] };
  for (const actor of [warehouse, sales]) {
    await db.prepare("UPDATE assignments SET modules_json=? WHERE id=?").bind(JSON.stringify(actor.modules), actor.assignmentId).run();
    actor.iam = await loadIamAuthority(db, actor.companyId, actor.userId, actor.role);
  }
  warehouseId = String((await saveMaster(f.owner, "warehouses", null, { branchId: f.branch, code: key(), name: "Current delivery warehouse" }, key())).id);
  productId = String((await saveMaster(f.owner, "products", null, { sku: key(), name: "Counted delivery product", price: "10" }, key())).id);
  partyId = String((await saveMaster(f.owner, "parties", null, { kind: "customer", code: key(), name: "Fictional delivery recipient" }, key())).id);
  // Declared opening stock; Sales approval and physical fulfillment are native.
  await db.prepare("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) VALUES(?,?,2000,1200)").bind(warehouseId, productId).run();
  orderId = String((await saveOrder(f.owner, null, orderSchema.parse({ kind: "sales", partyId, warehouseId, orderDate: "2026-09-21", dueDate: "2026-09-22", lines: [{ productId, quantity: "1", price: "10", taxBps: 0 }] }), key())).id);
  await transitionOrder(f.owner, orderId, { action: "submit", expectedVersion: 1 }, key());
  await transitionOrder(sales, orderId, { action: "approve", expectedVersion: 2 }, key());
  const flow = await resolveFlow(db, f.owner, "order", orderId);
  await assignFlowPrimary(db, f.owner, "order", orderId, { stepKey: "confirmed", userId: warehouse.userId, reason: "Route the independently approved Sales order to its current warehouse operator.", saveDefault: false, expectedVersion: flow.version, recordVersion: flow.recordVersion }, key());
  lineId = String((await getOrder(warehouse, orderId)).lines[0].id);
});
const input = () => ({ targetType: "commerce_order_delivery", targetId: orderId, componentKind: "action" as const, componentRef: "delivery", purposeKey: "proof_of_delivery", expectedTargetVersion: 3 });
const file = () => ({ filename: "customer-delivery.png", mediaType: "image/png", bytes: png.byteLength });

it("lets the actual warehouse role upload scan and consume delivery evidence once while retaining exact stock GL and reloadable source bytes", async () => {
  expect(warehouse.iam!.permissions).toContain("inventory.prepare");
  expect(warehouse.iam!.permissions).not.toContain("commerce.sales");
  const set = await createAttachmentSet(db, warehouse, input(), key());
  const slot = await reserveAttachmentFile(db, warehouse, set.id, file(), key());
  const pending = await reserveAttachmentFile(db, warehouse, set.id, { ...file(), filename: "late-delivery-evidence.png" }, key());
  const uploaded = await completeAttachmentUpload(db, env.H001_FILES, warehouse, slot.uploadIntentId, bytes);
  expect(uploaded.state).toBe("quarantined");
  expect(await drainAttachmentScans(db, env.H001_FILES, uploaded.sourceId)).toBe(true);
  expect(await attachmentLinkStatus(db, warehouse, slot.attachmentLinkId)).toMatchObject({ sourceState: "available", previewAvailable: true, bindable: true });
  // A second open intent is excluded from this immutable delivery set.
  await (await import("../src/server/attachments/service")).withdrawAttachmentLink(db, warehouse, pending.attachmentLinkId, "Only the receiver's clean final proof is used.", key());
  const command = key(), fulfillmentInput = { expectedVersion: 3, date: "2026-09-22", lines: [{ lineId, quantity: "1" }], evidenceIds: [], evidenceNote: "The fictional customer signed the warehouse delivery confirmation.", attachmentSetIds: [set.id] };
  // Fail after every owner write and its audit, proving a single commit boundary.
  const retainedTables=["commerce_orders","commerce_order_lines","commerce_stock","commerce_stock_moves","commerce_fulfillments","commerce_fulfillment_lines","commerce_fulfillment_evidence","inventory_ledger","inventory_order_reservations","inventory_lots","inventory_lot_balances","inventory_lot_moves","inventory_fulfillment_lot_allocations","finance_journals","finance_journal_lines","money_transaction_snapshots","attachment_sets","attachment_links","attachments","attachment_events","command_receipts","audit_events","outbox_events","command_assertions"];
  const retainedState=async()=>{
    const rows:Record<string,unknown[]>={};
    for(const table of retainedTables)rows[table]=(await db.prepare(`SELECT to_jsonb(t)::text row FROM ${table} t ORDER BY to_jsonb(t)::text`).all<{row:string}>()).results.map(row=>row.row);
    return rows;
  };
  const before=await retainedState(),fault=`commerce_delivery_late_${command.replaceAll("-","")}`;
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'COMMERCE_DELIVERY_LATE_OUTBOX';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.command_id='${command}') EXECUTE FUNCTION ${fault}();`);
  try {
    await expect(fulfillOrder(warehouse,orderId,fulfillmentSchema.parse(fulfillmentInput),command)).rejects.toThrow("COMMERCE_DELIVERY_LATE_OUTBOX");
    expect(await retainedState()).toEqual(before);
    expect(await attachmentBindingState(db,set.id)).toMatchObject({set_state:"draft",link_state:"draft"});
    for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT command_id FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();
  } finally {await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
  const delivered = await fulfillOrder(warehouse, orderId, fulfillmentSchema.parse(fulfillmentInput), command);
  expect(await fulfillOrder(warehouse, orderId, fulfillmentSchema.parse(fulfillmentInput), command)).toEqual(delivered);
  const reloaded = await getOrder(warehouse, orderId);
  expect(reloaded.order).toMatchObject({ state: "fulfilled", version: 4 });
  expect(reloaded.lines[0].fulfilled_milli).toBe(1000);
  expect(reloaded.fulfillments).toHaveLength(1);
  expect(await attachmentBindingState(db, set.id)).toMatchObject({ set_state: "bound", link_state: "bound", binding_type: "commerce_fulfillment", bound_command_id: command });
  expect(await completeAttachmentUpload(db, env.H001_FILES, warehouse, slot.uploadIntentId, bytes)).toMatchObject({ sourceId: uploaded.sourceId, state: "available" });
  const retained = await attachmentContent(db, env.H001_FILES, warehouse, slot.attachmentLinkId, "original");
  expect(Buffer.from(await retained.object.arrayBuffer())).toEqual(png);
  expect(await db.prepare("SELECT checksum_sha256 FROM attachments WHERE id=?").bind(uploaded.sourceId).first("checksum_sha256")).toBe(createHash("sha256").update(png).digest("hex"));
  expect((await listAttachmentSets(db, warehouse, { targetType: "commerce_order_delivery", targetId: orderId })).find(row => row.id === set.id)).toMatchObject({ state: "bound" });
  expect(await db.prepare("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?").bind(warehouseId, productId).first()).toEqual({ quantity_milli: 1000, value_minor: 600 });
  expect(await db.prepare("SELECT SUM(base_debit_minor) debit,SUM(base_credit_minor) credit FROM finance_journal_lines WHERE journal_id=?").bind(reloaded.fulfillments[0].journal_id).first()).toEqual({ debit: 600, credit: 600 });
  expect(await db.prepare("SELECT COUNT(*) n FROM audit_events WHERE command_id=?").bind(command).first("n")).toBe(1);
  expect(await db.prepare("SELECT COUNT(*) n FROM outbox_events WHERE command_id=?").bind(command).first("n")).toBe(1);
  await db.prepare("UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?").bind(Date.now(), warehouse.assignmentId).run();
  await expect(attachmentContent(db, env.H001_FILES, warehouse, slot.attachmentLinkId, "original")).rejects.toMatchObject({ status: 404, code: "ATTACHMENT_TARGET_NOT_FOUND" });
  await expect(completeAttachmentUpload(db, env.H001_FILES, warehouse, slot.uploadIntentId, bytes)).rejects.toMatchObject({ status: 404, code: "ATTACHMENT_TARGET_NOT_FOUND" });
  expect(await attachmentBindingState(db, set.id)).toMatchObject({ set_state: "bound", link_state: "bound", bound_command_id: command });
});

it("denies Sales-only preparation wrong Company Branch and source kinds without granting warehouse authority through a cached role", async () => {
  await expect(createAttachmentSet(db, sales, input(), key())).rejects.toMatchObject({ status: 403, code: "ATTACHMENT_FORBIDDEN" });
  const foreign = await expenseFixture(), foreignWarehouse = { ...foreign.owner, modules: ["inventory"] };
  await db.prepare("UPDATE assignments SET modules_json='[\"inventory\"]' WHERE id=?").bind(foreignWarehouse.assignmentId).run();
  await expect(createAttachmentSet(db, foreignWarehouse, input(), key())).rejects.toMatchObject({ status: 404, code: "ATTACHMENT_TARGET_NOT_FOUND" });
  await expect(createAttachmentSet(db, { ...warehouse, branchId: "other-branch" }, input(), key())).rejects.toMatchObject({ status: 403, code: "ATTACHMENT_SCOPE_FORBIDDEN" });
  const quotation = await saveOrder(f.owner, null, orderSchema.parse({ kind: "quote", partyId, warehouseId, orderDate: "2026-09-21", dueDate: "2026-09-22", lines: [{ productId, quantity: "1", price: "10", taxBps: 0 }] }), key());
  await expect(createAttachmentSet(db, warehouse, { ...input(), targetId: String(quotation.id), expectedTargetVersion: 1 }, key())).rejects.toMatchObject({ status: 404, code: "ATTACHMENT_TARGET_NOT_FOUND" });
  await expect(createAttachmentSet(db, warehouse, { ...input(), expectedTargetVersion: 2 }, key())).rejects.toMatchObject({ status: 409, code: "ATTACHMENT_TARGET_CHANGED" });
  expect(await db.prepare("SELECT COUNT(*) n FROM attachments WHERE company_id=?").bind(f.company).first("n")).toBe(0);
});

it("rechecks current inventory modules narrowed permissions and revoked grants before pending upload without writing source objects", async () => {
  const set = await createAttachmentSet(db, warehouse, input(), key()), slot = await reserveAttachmentFile(db, warehouse, set.id, file(), key());
  await db.prepare("UPDATE assignments SET modules_json='[]',version=version+1 WHERE id=?").bind(warehouse.assignmentId).run();
  await expect(completeAttachmentUpload(db, env.H001_FILES, warehouse, slot.uploadIntentId, bytes)).rejects.toMatchObject({ status: 403, code: "ATTACHMENT_FORBIDDEN" });
  await db.prepare("UPDATE assignments SET modules_json='[\"inventory\",\"operations\"]' WHERE id=?").bind(warehouse.assignmentId).run();
  const narrowed = { ...warehouse, iam: { ...warehouse.iam!, permissions: warehouse.iam!.permissions.filter(permission => permission !== "inventory.prepare") } };
  await expect(completeAttachmentUpload(db, env.H001_FILES, narrowed, slot.uploadIntentId, bytes)).rejects.toMatchObject({ status: 403, code: "ATTACHMENT_FORBIDDEN" });
  await db.prepare("UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?").bind(Date.now(), warehouse.assignmentId).run();
  await expect(completeAttachmentUpload(db, env.H001_FILES, warehouse, slot.uploadIntentId, bytes)).rejects.toMatchObject({ status: 404, code: "ATTACHMENT_TARGET_NOT_FOUND" });
  await expect(listAttachmentSets(db, warehouse, { targetType: "commerce_order_delivery", targetId: orderId })).rejects.toMatchObject({ status: 404, code: "ATTACHMENT_TARGET_NOT_FOUND" });
  expect(await db.prepare("SELECT status,actual_bytes FROM upload_intents WHERE id=?").bind(slot.uploadIntentId).first()).toEqual({ status: "pending", actual_bytes: null });
  const quarantine = String(await db.prepare("SELECT quarantine_key FROM upload_intents WHERE id=?").bind(slot.uploadIntentId).first("quarantine_key"));
  expect(await env.H001_FILES.get(quarantine)).toBeNull();
  expect(await db.prepare("SELECT COUNT(*) n FROM attachments WHERE company_id=?").bind(f.company).first("n")).toBe(0);
  expect((await getOrder(f.owner, orderId)).order).toMatchObject({ state: "confirmed", version: 3 });
});
