import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { mkdir, writeFile } from "node:fs/promises";
import { applyMigrations, env } from "./runtime-env";
import { seedExpenseCompany } from "../fixtures/expenses/company";
import { loadPms, pmsCommand, pmsWorkspace } from "../src/server/pms";
import { pmsAccountKeys, records, type Kind } from "../src/lib/pms-model";
import { availability } from "../src/lib/pms-inventory";
import { pmsListSelection, pmsReadSelectionSchema } from "../src/lib/pms-read-selection";
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

it("pages the guest directory without loading hotel history and preserves a selected guest outside the page", async () => {
 const guests=[];
 for(let n=0;n<55;n++)guests.push(await insert('guest',{name:`Guest ${String(n).padStart(2,'0')}`,phone:`081${n}`}));
 for(let n=0;n<25;n++)await insert('stay',{guestId:guests[54],state:'checked_out',arrival:`2026-08-${String(n+1).padStart(2,'0')}`,departure:'2026-09-01',prices:[]});
 const unrelated=await insert('entry',{folioId:'old-history',date:'2020-01-01',total:50});
 const first=await loadPms(db,fixture.owner,propertyId,false,pmsListSelection('guests',{})!);
 const second=await loadPms(db,fixture.owner,propertyId,false,pmsListSelection('guests',{page:'1'})!);
 expect(first.listMeta!.ids).toHaveLength(50);
 expect(first.listMeta!.hasNext).toBe(true);
 expect(second.listMeta!.ids).toHaveLength(5);
 expect(second.listMeta!.hasNext).toBe(false);
 expect(first.listMeta!.ids.filter(id=>second.listMeta!.ids.includes(id))).toEqual([]);
 expect(first.records.some(row=>row.id===unrelated)).toBe(false);
 const selected=await loadPms(db,fixture.owner,propertyId,false,pmsListSelection('guests',{q:'Guest 00',record:guests[54]})!);
 expect(selected.listMeta!.ids).toEqual([guests[0]]);
 expect(records(selected,'guest').map(row=>row.id)).toContain(guests[54]);
 expect(records(selected,'stay')).toHaveLength(20);
 expect(records(selected,'stay').some(row=>row.data.arrival==='2026-08-01')).toBe(false);
});

it("retains occupied rooms, open tasks and staff in bounded room/housekeeping views", async () => {
 const room=await seedRooms();
 const active=await insert('stay',{...room,state:'in_house',arrival:'2026-08-31',departure:'2026-09-02',prices:[]});
 const old=await insert('stay',{...room,state:'checked_out',arrival:'2020-01-01',departure:'2020-01-02',prices:[]});
 const pending=await insert('task',{roomId:room.roomId,state:'pending',date:'2026-08-01'});
 const done=await insert('task',{roomId:room.roomId,state:'done',date:'2026-08-01'});
 await db.prepare('UPDATE pms_records SET updated_at=0 WHERE id=?').bind(done).run();
 for(let n=0;n<55;n++)await insert('task',{roomId:room.roomId,state:'done',date:'2026-08-31'});
 for(const section of ['rooms','housekeeping']){
   const workspace=await pmsWorkspace(fixture.owner,propertyId,false,false,pmsListSelection(section,{status:'inspected'})!);
   expect(workspace.snapshot!.listMeta!.ids).toEqual([room.roomId]);
   expect(workspace.snapshot!.records.map(row=>row.id)).toEqual(expect.arrayContaining([room.roomId,active,pending]));
   expect(workspace.snapshot!.records.some(row=>row.id===old||row.id===done)).toBe(false);
   expect(workspace.staff.length).toBeGreaterThan(0);
 }
});

it("loads folio sources and original refund reservations while excluding unrelated entries", async () => {
 const guestId=await insert('guest',{name:'Selected guest'});
 const folio=await insert('folio',{name:'Selected folio',guestId,state:'open',due:0,advance:100,deposit:0});
 const entry=await insert('entry',{folioId:folio,kind:'payment',total:100});
 const refund=await insert('refund',{folioId:folio,state:'requested',requestedBy:fixture.owner.userId,amount:20});
 const old=await insert('entry',{folioId:'another-folio',kind:'payment',total:100});
 const read=await loadPms(db,fixture.owner,propertyId,false,pmsListSelection('folios',{q:'Selected',folio})!);
 expect(read.records.map(row=>row.id)).toEqual(expect.arrayContaining([folio,guestId,entry,refund]));
 expect(read.records.some(row=>row.id===old)).toBe(false);
});

it("does not hide overdue stays or unreviewed cash shifts when paging day-close history", async () => {
 const room=await seedRooms();
 for(let n=0;n<55;n++)await insert('night',{date:'2026-08-01',state:'completed'});
 const overdue=await insert('stay',{...room,state:'reserved',arrival:'2026-08-01',departure:'2026-08-02',prices:[]});
 const shift=await insert('shift',{state:'closed',date:'2020-01-01',variance:100,userId:fixture.owner.userId});
 const old=await insert('stay',{...room,state:'checked_out',arrival:'2020-01-01',departure:'2020-01-02',prices:[]});
 const read=await loadPms(db,fixture.owner,propertyId,false,pmsListSelection('night-audit',{page:'1',from:'2026-08-01',to:'2026-09-01'})!);
 expect(read.listMeta!.ids).toHaveLength(5);
 expect(read.records.map(row=>row.id)).toEqual(expect.arrayContaining([overdue,shift]));
 expect(read.records.some(row=>row.id===old)).toBe(false);
});

it("reads setup definitions and report control totals without serializing old guest or financial history", async () => {
 await seedRooms();
 const guest=await insert('guest',{name:'Old private guest'});
 await insert('stay',{guestId:guest,state:'checked_out',arrival:'2020-01-01',departure:'2020-01-02',prices:[]});
 await insert('folio',{name:'Balance',state:'open',due:3000,advance:2000,deposit:1000});
 await insert('entry',{folioId:'history',kind:'payment',total:10000});
 const setup=await pmsWorkspace(fixture.owner,propertyId,false,false,pmsListSelection('settings',{})!);
 expect(setup.snapshot!.records.map(row=>row.kind).sort()).toEqual(['rate','room','room_type']);
 expect(setup.charts.length).toBeGreaterThan(0);
 const report=await pmsWorkspace(fixture.owner,propertyId,false,false,pmsListSelection('reports',{})!);
 expect(report.snapshot!.records).toEqual([]);
 expect(report.controlBalances).toEqual({due:3000,advance:2000,deposit:1000});
 expect(report.ledger).not.toBeNull();
 expect(report.readiness).toEqual(setup.readiness);
});

it("keeps occupied folios and original movement item names available without expanding the inventory page", async () => {
 const room=await seedRooms();
 for(let n=0;n<50;n++)await insert('item',{name:`Paged item ${n}`,kind:'minibar',price:100});
 const originalItem=await insert('item',{name:'Original linen',kind:'linen',price:0});
 const movement=await insert('movement',{itemId:originalItem,date:'2026-09-01',kind:'linen.issue',quantity:1});
 const guest=await insert('guest',{name:'Current minibar guest'});
 const folio=await insert('folio',{name:'Current stay',guestId:guest,state:'open',due:0,advance:0,deposit:0});
 const stay=await insert('stay',{...room,guestId:guest,folioId:folio,state:'in_house',arrival:'2026-08-31',departure:'2026-09-02',prices:[]});
 const history=await insert('stay',{...room,guestId:guest,folioId:'historical-folio',state:'checked_out',arrival:'2020-01-01',departure:'2020-01-02',prices:[]});
 const read=await loadPms(db,fixture.owner,propertyId,false,pmsListSelection('inventory',{q:'Paged item'})!);
 expect(read.listMeta!.ids).toHaveLength(50);
 expect(read.listMeta!.ids).not.toContain(originalItem);
 expect(read.records.map(row=>row.id)).toEqual(expect.arrayContaining([originalItem,movement,guest,folio,stay]));
 expect(read.records.some(row=>row.id===history)).toBe(false);
 expect(records(read,'item').find(row=>row.id===originalItem)?.data.name).toBe('Original linen');
 expect(records(read,'folio').map(row=>row.id)).toEqual([folio]);
});

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
  const page=await pmsWorkspace(fixture.owner,propertyId,false,true,pmsListSelection('cashier',{record:hidden[0]})!);
  expect(page.snapshot!.listMeta!.ids).not.toContain(hidden[0]);
  expect(page.snapshot!.listMeta!.selectedId).toBeUndefined();
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
