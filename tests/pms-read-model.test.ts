import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { mkdir, writeFile } from "node:fs/promises";
import { applyMigrations, env } from "./runtime-env";
import { seedExpenseCompany } from "../fixtures/expenses/company";
import { loadPms, pmsCommand, pmsWorkspace } from "../src/server/pms";
import { pmsAccountKeys, records, type Kind } from "../src/lib/pms-model";
import { availability } from "../src/lib/pms-inventory";
import { pmsReadSelectionSchema } from "../src/lib/pms-read-selection";
import { measureReads } from "../src/server/runtime/read-metrics";

vi.mock("../src/server/runtime", async () => {
  const { env } = await import("./runtime-env");
  return { holdingResources: async () => ({ write: env.H001_DB }) };
});
const db = env.H001_DB;
let fixture: Awaited<ReturnType<typeof seedExpenseCompany>>, propertyId: string;
beforeAll(() => applyMigrations(db, env.STAFF_TEST_MIGRATIONS));
beforeEach(async () => {
  fixture = await seedExpenseCompany(db, env.H001_FILES);
  propertyId = (await pmsCommand(db, fixture.owner, { operation: "property.create", input: {
    name: "Read fixture", branchId: fixture.branch, businessDate: "2026-09-01" } }, crypto.randomUUID())).id;
});
async function insert(kind: Kind, data: object, id = crypto.randomUUID()) {
  await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,?,?,1,1)")
    .bind(id, propertyId, kind, JSON.stringify(data)).run();
  return id;
}
async function seedRooms() {
  const typeId = await insert("room_type", { name: "Deluxe", capacity: 2, amenities: "" });
  const roomId = await insert("room", { number: "101", typeId, active: true, cleanliness: "inspected" });
  const rateId = await insert("rate", { typeId, active: true, from: "2026-01-01", to: "2026-12-31", price: 10000 });
  return { typeId, roomId, rateId };
}

it("keeps dashboard totals, readiness, guest labels and the complete legacy read while excluding history", async () => {
  const room = await seedRooms();
  const guestId = await insert("guest", { name: "Current guest", documentRef: "restricted" });
  const folioId = await insert("folio", { guestId, state: "open", due: 100, deposit: 0 });
  await insert("stay", { ...room, guestId, folioId, state: "in_house", arrival: "2026-08-31", departure: "2026-09-02", prices: [] });
  await insert("task", { roomId: room.roomId, state: "assigned" });
  await insert("task", { roomId: room.roomId, state: "done" });
  await insert("stay", { ...room, guestId, state: "checked_out", arrival: "2026-01-01", departure: "2026-01-02" });
  await insert("entry", { folioId, date: "2026-01-01", total: 100 });
  const full = await pmsWorkspace(fixture.owner, propertyId, false, true);
  const small = await pmsWorkspace(fixture.owner, propertyId, false, true, { view: "dashboard" });
  expect(small.readiness).toEqual(full.readiness);
  expect(small.snapshot!.property).toEqual(full.snapshot!.property);
  expect(records(small.snapshot!, "room")).toEqual(records(full.snapshot!, "room"));
  expect(records(small.snapshot!, "guest")).toEqual(records(full.snapshot!, "guest"));
  expect(records(small.snapshot!, "stay")).toHaveLength(1);
  expect(records(small.snapshot!, "task")).toHaveLength(1);
  expect(records(small.snapshot!, "folio")).toHaveLength(1);
  expect(records(small.snapshot!, "entry")).toHaveLength(0);
  expect(small.snapshot!.dashboardHasStays).toBe(true);
  expect(full.snapshot!.dashboardHasStays).toBeUndefined();
  expect((await loadPms(db, fixture.owner, propertyId)).records).toHaveLength(10);
  await expect(loadPms(db, fixture.owner, propertyId, true, { view: "dashboard" })).rejects.toThrow("PMS_COMMAND_REQUIRES_COMPLETE_STATE");
});

it("uses exclusive calendar overlap and retains maintenance and group availability semantics", async () => {
  const room = await seedRooms();
  await insert("stay", { ...room, state: "reserved", arrival: "2026-08-31", departure: "2026-09-02" });
  await insert("stay", { ...room, state: "reserved", arrival: "2026-09-03", departure: "2026-09-04" });
  await insert("stay", { ...room, state: "reserved", arrival: "2026-08-30", departure: "2026-09-01" });
  await insert("block", { roomId: room.roomId, kind: "maintenance", state: "active", from: "2026-08-01", to: "2026-08-02" });
  const full = await loadPms(db, fixture.owner, propertyId);
  const narrow = await loadPms(db, fixture.owner, propertyId, false, { view: "calendar", from: "2026-09-01", to: "2026-09-03" });
  expect(records(narrow, "stay")).toHaveLength(1);
  for (const date of ["2026-09-01", "2026-09-02"]) expect(availability(narrow, room.typeId, date)).toEqual(availability(full, room.typeId, date));
  expect(pmsReadSelectionSchema.safeParse({ view: "calendar", from: "2026-09-03", to: "2026-09-01" }).success).toBe(false);
  expect(pmsReadSelectionSchema.safeParse({ view: "calendar", from: "2026-01-01", to: "2026-09-01" }).success).toBe(false);
});

it("bounds detail to the selected property and keeps related folio entries and references", async () => {
  const room = await seedRooms();
  const guestId = await insert("guest", { name: "Guest" });
  const folioId = await insert("folio", { guestId, state: "open", due: 0 });
  const entryId = await insert("entry", { folioId, total: 100 });
  const otherId = await insert("guest", { name: "Unrelated" });
  const detail = await pmsWorkspace(fixture.owner, propertyId, false, false, { view: "detail", id: folioId });
  expect(detail.snapshot!.records.map(row => row.id).sort()).toEqual([folioId, guestId, entryId].sort());
  expect(detail.snapshot!.records.some(row => row.id === otherId || row.id === room.roomId)).toBe(false);
  await expect(pmsWorkspace(fixture.owner, propertyId, false, false, { view: "detail", id: "missing" })).rejects.toMatchObject({ status: 404 });
});

it("keeps property readiness for detail and calendars with no rates in their window", async () => {
  await seedRooms();
  const row = await db.prepare("SELECT config_json FROM pms_properties WHERE id=?").bind(propertyId).first<{ config_json: string }>();
  const config = JSON.parse(row!.config_json);
  config.approvedBy = fixture.owner.userId;
  config.chartId = "readiness-fixture-chart";
  config.accounts = Object.fromEntries(pmsAccountKeys.map(key => [key, `readiness-fixture-${key}`]));
  await db.prepare("UPDATE pms_properties SET state='open',config_json=? WHERE id=?").bind(JSON.stringify(config), propertyId).run();
  const folioId = await insert("folio", { state: "open", due: 0 });
  const full = await pmsWorkspace(fixture.owner, propertyId, false, true);
  expect(full.readiness).toEqual([]);
  expect(full.frontReady).toBe(true);
  const detail = await pmsWorkspace(fixture.owner, propertyId, false, true, { view: "detail", id: folioId });
  const calendar = await pmsWorkspace(fixture.owner, propertyId, false, true,
    { view: "calendar", from: "2027-01-01", to: "2027-01-03" });
  expect(records(detail.snapshot!, "room")).toHaveLength(0);
  expect(records(calendar.snapshot!, "rate")).toHaveLength(0);
  for (const view of [detail, calendar]) {
    expect(view.readiness).toEqual(full.readiness);
    expect(view.frontReady).toBe(full.frontReady);
    expect(view.snapshot).not.toHaveProperty("setupFacts");
  }
  expect(full.snapshot).not.toHaveProperty("setupFacts");
});

it("retains fresh permission, company and branch privacy for every query model", async () => {
  await seedRooms();
  const guestId = await insert("guest", { name: "Private guest", documentRef: "restricted" });
  const cleaner = await fixture.member("read-model-cleaner", "pms_housekeeping");
  await expect(pmsWorkspace(cleaner, propertyId, false, false, { view: "detail", id: guestId })).rejects.toMatchObject({ status: 404 });
  await expect(pmsWorkspace({ ...fixture.owner, companyId: "foreign" }, propertyId, false, false, { view: "dashboard" })).rejects.toMatchObject({ status: 404 });
  await expect(pmsWorkspace({ ...fixture.owner, branchId: "foreign" }, propertyId, false, false, { view: "dashboard" })).rejects.toMatchObject({ status: 404 });
  await expect(pmsWorkspace({ ...fixture.owner, role: "employee", modules: [] }, propertyId, false, false, { view: "dashboard" })).rejects.toMatchObject({ status: 403 });
});

it("rejects detail targets removed by Front ownership redaction", async () => {
  const hidden = [
    await insert("shift", { userId: "another-cashier", state: "open" }),
    await insert("refund", { requestedBy: "another-cashier", state: "requested" }),
    await insert("credit", { state: "open" }),
  ];
  for (const id of hidden) {
    const consoleDetail = await pmsWorkspace(fixture.owner, propertyId, false, false, { view: "detail", id });
    expect(consoleDetail.snapshot!.records.some(row => row.id === id)).toBe(true);
    await expect(pmsWorkspace(fixture.owner, propertyId, false, true, { view: "detail", id }))
      .rejects.toMatchObject({ status: 404, code: "PMS_RECORD_NOT_FOUND" });
  }
  for (const id of [
    await insert("shift", { userId: fixture.owner.userId, state: "open" }),
    await insert("refund", { requestedBy: fixture.owner.userId, state: "requested" }),
  ]) {
    const frontDetail = await pmsWorkspace(fixture.owner, propertyId, false, true, { view: "detail", id });
    expect(frontDetail.snapshot!.records.some(row => row.id === id)).toBe(true);
  }
});

it("retains all 20 measurements and reduces dashboard payload by at least half on identical data", async () => {
  await seedRooms();
  await db.batch(Array.from({ length: 200 }, (_, index) => db.prepare(
    "INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'entry',?,1,1)")
    .bind(crypto.randomUUID(), propertyId, JSON.stringify({ folioId: "historical", date: "2026-01-01", total: 100, description: `Historical charge ${index}`, history: "x".repeat(400) }))));
  const samples = [];
  for (let round = 0; round < 20; round++) {
    const full = await measureReads(() => pmsWorkspace(fixture.owner, propertyId, false, true));
    const small = await measureReads(() => pmsWorkspace(fixture.owner, propertyId, false, true, { view: "dashboard" }));
    const beforeBytes = Buffer.byteLength(JSON.stringify(full.result));
    const afterBytes = Buffer.byteLength(JSON.stringify(small.result));
    expect(afterBytes).toBeLessThan(beforeBytes / 2);
    expect(small.result.readiness).toEqual(full.result.readiness);
    samples.push({ round, beforeBytes, afterBytes, before: full.metrics, after: small.metrics });
  }
  await mkdir(".scratch/dev", { recursive: true });
  await writeFile(".scratch/dev/pms-dashboard-read-comparison.json", JSON.stringify({
    methodology: "Alternating legacy/front dashboard query model on the same isolated 200-history-record fixture. Service SQL roundtrips; excludes compiler, HTTP authentication/network/browser.", samples }, null, 2));
});
