import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { mkdir, writeFile } from "node:fs/promises";
import { applyMigrations, env } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { loadPms, pmsCommand, pmsWorkspace } from "../../../packages/server-pms/src/public/pms";
import { pmsAccountKeys, records, type Kind } from "@erpx/server-pms/public/rules/pms-model";
import { availability } from "@erpx/feature-pms/lib/pms-inventory";
import { pmsListSelection, pmsReadSelectionSchema } from "@erpx/feature-pms/lib/pms-read-selection";
import { readPmsInventoryCommand } from "@erpx/server-pms/public/pms/read-repository";
import { measureReads } from "@erpx/server-runtime/adapters/runtime/read-metrics";

vi.mock("@erpx/server-runtime/adapters/runtime", async () => {
  const { env } = await import("../../support/runtime-env");
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
 const unrelated=await insert('entry',{guestAccountId:'old-history',date:'2020-01-01',total:50});
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

it("loads guest account sources and original refund reservations while excluding unrelated entries", async () => {
 const guestId=await insert('guest',{name:'Selected guest'});
 const guestAccount=await insert('guest_account',{name:'Selected guest account',guestId,state:'open',due:0,advance:100,deposit:0});
 const entry=await insert('entry',{guestAccountId:guestAccount,kind:'payment',total:100});
 const refund=await insert('refund',{guestAccountId:guestAccount,state:'requested',requestedBy:fixture.owner.userId,amount:20});
 const old=await insert('entry',{guestAccountId:'another-guestAccount',kind:'payment',total:100});
 const read=await loadPms(db,fixture.owner,propertyId,false,pmsListSelection('guest-accounts',{q:'Selected',guestAccount})!);
 expect(read.records.map(row=>row.id)).toEqual(expect.arrayContaining([guestAccount,guestId,entry,refund]));
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
 await insert('guest_account',{name:'Balance',state:'open',due:3000,advance:2000,deposit:1000});
 await insert('entry',{guestAccountId:'history',kind:'payment',total:10000});
 const setup=await pmsWorkspace(fixture.owner,propertyId,false,false,pmsListSelection('settings',{})!);
 expect(setup.snapshot!.records.map(row=>row.kind).sort()).toEqual(['rate','room','room_type']);
 expect(setup.charts.length).toBeGreaterThan(0);
 const report=await pmsWorkspace(fixture.owner,propertyId,false,false,pmsListSelection('reports',{})!);
 expect(report.snapshot!.records).toEqual([]);
 expect(report.controlBalances).toEqual({due:3000,advance:2000,deposit:1000});
 expect(report.ledger).not.toBeNull();
 expect(report.readiness).toEqual(setup.readiness);
});

it("keeps occupied guest accounts and original movement item names available without expanding the inventory page", async () => {
 const room=await seedRooms();
 for(let n=0;n<50;n++)await insert('item',{name:`Paged item ${n}`,kind:'minibar',price:100});
 const originalItem=await insert('item',{name:'Original linen',kind:'linen',price:0});
 const movement=await insert('movement',{itemId:originalItem,date:'2026-09-01',kind:'linen.issue',quantity:1});
 const guest=await insert('guest',{name:'Current minibar guest'});
 const guestAccount=await insert('guest_account',{name:'Current stay',guestId:guest,state:'open',due:0,advance:0,deposit:0});
 const stay=await insert('stay',{...room,guestId:guest,guestAccountId:guestAccount,state:'in_house',arrival:'2026-08-31',departure:'2026-09-02',prices:[]});
 const history=await insert('stay',{...room,guestId:guest,guestAccountId:'historical-guestAccount',state:'checked_out',arrival:'2020-01-01',departure:'2020-01-02',prices:[]});
 const read=await loadPms(db,fixture.owner,propertyId,false,pmsListSelection('inventory',{q:'Paged item'})!);
 expect(read.listMeta!.ids).toHaveLength(50);
 expect(read.listMeta!.ids).not.toContain(originalItem);
 expect(read.records.map(row=>row.id)).toEqual(expect.arrayContaining([originalItem,movement,guest,guestAccount,stay]));
 expect(read.records.some(row=>row.id===history)).toBe(false);
 expect(records(read,'item').find(row=>row.id===originalItem)?.data.name).toBe('Original linen');
 expect(records(read,'guest_account').map(row=>row.id)).toEqual([guestAccount]);
});

it("keeps dashboard totals, readiness, guest labels and the complete legacy read while excluding history", async () => {
  const room = await seedRooms();
  const guestId = await insert("guest", { name: "Current guest", documentRef: "restricted" });
  const guestAccountId = await insert("guest_account", { guestId, state: "open", due: 100, deposit: 0 });
  await insert("stay", { ...room, guestId, guestAccountId, state: "in_house", arrival: "2026-08-31", departure: "2026-09-02", prices: [] });
  await insert("task", { roomId: room.roomId, state: "assigned" });
  await insert("task", { roomId: room.roomId, state: "done" });
  await insert("stay", { ...room, guestId, state: "checked_out", arrival: "2026-01-01", departure: "2026-01-02" });
  await insert("entry", { guestAccountId, date: "2026-01-01", total: 100 });
  const full = await pmsWorkspace(fixture.owner, propertyId, false, true);
  const small = await pmsWorkspace(fixture.owner, propertyId, false, true, { view: "dashboard" });
  expect(small.readiness).toEqual(full.readiness);
  expect(small.snapshot!.property).toEqual(full.snapshot!.property);
  expect(records(small.snapshot!, "room")).toEqual(records(full.snapshot!, "room"));
  expect(records(small.snapshot!, "guest")).toEqual(records(full.snapshot!, "guest"));
  expect(records(small.snapshot!, "stay")).toHaveLength(1);
  expect(records(small.snapshot!, "task")).toHaveLength(1);
  expect(records(small.snapshot!, "guest_account")).toHaveLength(1);
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

it("bounds detail to the selected property and keeps related guest account entries and references", async () => {
  const room = await seedRooms();
  const guestId = await insert("guest", { name: "Guest" });
  const guestAccountId = await insert("guest_account", { guestId, state: "open", due: 0 });
  const entryId = await insert("entry", { guestAccountId, total: 100 });
  const otherId = await insert("guest", { name: "Unrelated" });
  const detail = await pmsWorkspace(fixture.owner, propertyId, false, false, { view: "detail", id: guestAccountId });
  expect(detail.snapshot!.records.map(row => row.id).sort()).toEqual([guestAccountId, guestId, entryId].sort());
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
  const guestAccountId = await insert("guest_account", { state: "open", due: 0 });
  const full = await pmsWorkspace(fixture.owner, propertyId, false, true);
  expect(full.readiness).toEqual([]);
  expect(full.frontReady).toBe(true);
  const detail = await pmsWorkspace(fixture.owner, propertyId, false, true, { view: "detail", id: guestAccountId });
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
    .bind(crypto.randomUUID(), propertyId, JSON.stringify({ guestAccountId: "historical", date: "2026-01-01", total: 100, description: `Historical charge ${index}`, history: "x".repeat(400) }))));
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


it("filters room type, building and floor before pagination without losing a selected detail", async () => {
 const {typeId}=await seedRooms();
 const otherType=await insert('room_type',{name:'Other',capacity:2,amenities:''});
 const matching=[];
 for(let n=0;n<53;n++)matching.push(await insert('room',{number:`A${String(n).padStart(3,'0')}`,typeId,building:'Garden',floor:'2',active:true,cleanliness:'inspected'}));
 const wrongFloor=await insert('room',{number:'A0000',typeId,building:'Garden',floor:'1',active:true,cleanliness:'inspected'});
 await insert('room',{number:'A0001',typeId:otherType,building:'Garden',floor:'2',active:true,cleanliness:'inspected'});
 await insert('room',{number:'A0002',typeId,building:'Courtyard',floor:'2',active:true,cleanliness:'inspected'});
 await insert('stay',{roomId:matching[0],state:'in_house',arrival:'2026-08-01',departure:'2026-08-02',prices:[]});
 const filter={typeId,building:'Garden',floor:'2'};
 const first=await loadPms(db,fixture.owner,propertyId,false,pmsListSelection('rooms',filter)!);
 const second=await loadPms(db,fixture.owner,propertyId,false,pmsListSelection('housekeeping',{...filter,page:'1',record:wrongFloor})!);
 expect(first.listMeta!.ids).toEqual(matching.slice(0,50));
 expect('roomSummary' in first.listMeta! && first.listMeta.roomSummary).toMatchObject({total:53,active:53,inspected:53,occupied:1});
 expect(first.listMeta!.hasNext).toBe(true);
 expect(second.listMeta!.ids).toEqual(matching.slice(50));
 expect(second.listMeta!.hasNext).toBe(false);
 expect(second.records.some(row=>row.id===wrongFloor)).toBe(true);
 expect(second.listMeta!.building).toBe('Garden');
 expect(second.listMeta!.floor).toBe('2');
 expect(second.listMeta!.typeId).toBe(typeId);
 // Filters are a room query concern; a stale room filter must not hide guests.
 const guest=await insert('guest',{name:'Visible guest'});
 const guests=await loadPms(db,fixture.owner,propertyId,false,pmsListSelection('guests',filter)!);
 expect(guests.listMeta!.ids).toEqual([guest]);
});


it("measures scoped room reads and guest writes with three 200-room properties and two-year synthetic history",async()=>{
 const propertyIds=[propertyId];
 for(let n=1;n<3;n++){const branch=crypto.randomUUID();await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Scale branch',?,?,?)").bind(branch,branch,fixture.company,fixture.company,branch).run();propertyIds.push((await pmsCommand(db,fixture.owner,{operation:'property.create',input:{name:`Scale ${n}`,branchId:branch,businessDate:'2026-09-01'}},crypto.randomUUID())).id);}
 for(const id of propertyIds){
   const type=crypto.randomUUID();
   await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'room_type',?,1,1)").bind(type,id,JSON.stringify({name:'Scale Queen',capacity:2,amenities:''})).run();
   await db.prepare(`INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at)
     SELECT ?||':room:'||n,?,'room',jsonb_build_object('number',lpad(n::text,3,'0'),'typeId',?::text,'building','Garden','floor',((n-1)/20+1)::text,'active',true,'cleanliness','inspected')::text,1,1 FROM generate_series(1,200) n`).bind(id,id,type).run();
   // 100 completed room nights/day for 730 days, deliberately synthetic and never posted to Finance.
   for(let start=1;start<=73000;start+=5000)await db.prepare(`INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at)
     SELECT ?||':history:'||n,?,'stay',jsonb_build_object('roomId',?||':room:'||(n%200+1),'state','checked_out','arrival',('2024-08-31'::date+(n%730))::text,'departure',('2024-09-01'::date+(n%730))::text,'prices',jsonb_build_array())::text,1,1 FROM generate_series(?::integer,?::integer) n`).bind(id,id,id,start,Math.min(start+4999,73000)).run();
 }
 const readMs:number[]=[],writeMs:number[]=[];
 await loadPms(db,fixture.owner,propertyId,false,pmsListSelection('rooms',{})!);
 for(let round=0;round<3;round++)await Promise.all(Array.from({length:20},async(_,n)=>{
   const start=performance.now(),result=await loadPms(db,fixture.owner,propertyIds[n%3],false,pmsListSelection('rooms',{building:'Garden',floor:'2'})!);
   expect(result.listMeta!.ids).toHaveLength(20); expect('roomSummary' in result.listMeta!&&result.listMeta.roomSummary).toMatchObject({total:20,inspected:20});
   expect(records(result,'stay')).toHaveLength(0);readMs.push(performance.now()-start);
 }));
 await Promise.all(Array.from({length:20},async(_,n)=>{
   const id=propertyIds[n%3],start=performance.now(),idem=crypto.randomUUID();
   for(let attempt=0;attempt<20;attempt++){
     const row=await db.prepare('SELECT version FROM pms_properties WHERE id=?').bind(id).first<{version:number}>();
     try{await pmsCommand(db,fixture.owner,{operation:'guest.save',propertyId:id,expectedVersion:Number(row!.version),input:{name:`Concurrent guest ${n}`}},idem);writeMs.push(performance.now()-start);return;}
     catch(error){if(!['PMS_VERSION_CONFLICT','PMS_CONFLICT','PMS_RETRYABLE_CONFLICT'].includes((error as {code?:string}).code??''))throw error;}
   } throw new Error('CONCURRENT_GUEST_RETRY_EXHAUSTED');
 }));
 const guestCount=await db.prepare("SELECT count(*)::integer AS count FROM pms_records WHERE kind='guest' AND property_id IN(?,?,?) AND data_json::jsonb->>'name' LIKE 'Concurrent guest %'").bind(...propertyIds).first<{count:number}>();
 expect(guestCount?.count).toBe(20);
 const p95=(values:number[])=>[...values].sort((a,b)=>a-b)[Math.ceil(values.length*.95)-1];
 const result={scope:'source-level diagnostic, not release acceptance',properties:3,roomsPerProperty:200,historyDays:730,completedStays:219000,concurrency:20,readSamples:readMs.length,writeSamples:writeMs.length,readP95Ms:p95(readMs),guestWriteP95Ms:p95(writeMs),remaining:'Release HTTP candidate and other financial commands still require measurement'};
 await mkdir('.scratch/dev',{recursive:true});await writeFile('.scratch/dev/pms-scale-source.json',JSON.stringify(result,null,2)+'\n');
 expect(readMs).toHaveLength(60);expect(writeMs).toHaveLength(20);
},180000);

it('uses authoritative room occupancy and all room tasks for housekeeping commands beyond a UI page',async()=>{
 await db.prepare("UPDATE pms_properties SET state='open' WHERE id=?").bind(propertyId).run();
 const room=await insert('room',{number:'HK-101',active:true,cleanliness:'dirty',cleaningCycle:1,dnd:false});
 // Still physically in house after planned departure; a calendar-date filter would lose this invariant.
 await insert('stay',{roomId:room,state:'in_house',arrival:'2026-08-01',departure:'2026-08-02',prices:[]});
 for(let n=0;n<60;n++)await insert('task',{roomId:room,type:'stayover',date:`2026-07-${String(n%28+1).padStart(2,'0')}`,roomCycle:0,state:'done',stayId:''});
 const send=async(operation:string,input:object)=>{const row=await db.prepare('SELECT version FROM pms_properties WHERE id=?').bind(propertyId).first<{version:number}>();return pmsCommand(db,fixture.owner,{propertyId,expectedVersion:Number(row!.version),operation,input},crypto.randomUUID());};
 await send('room.observe',{id:room,occupied:true});
 expect(records(await loadPms(db,fixture.owner,propertyId),'task').filter(t=>t.data.type==='discrepancy')).toHaveLength(0);
 await send('room.observe',{id:room,occupied:false});await send('room.observe',{id:room,occupied:false});
 expect(records(await loadPms(db,fixture.owner,propertyId),'task').filter(t=>t.data.type==='discrepancy')).toHaveLength(1);
 await expect(send('room.dnd',{id:'foreign-room',dnd:true})).rejects.toMatchObject({code:'PMS_RECORD_NOT_FOUND'});
});


it("keeps complete inventory and overdue occupancy for configuration commands without reading closed hotel history", async () => {
  const room=await seedRooms();
  const rooms=[];for(let n=0;n<55;n++)rooms.push(await insert('room',{number:`2${String(n).padStart(2,'0')}`,typeId:room.typeId,active:true,cleanliness:'inspected'}));
  const overdue=await insert('stay',{typeId:room.typeId,state:'in_house',arrival:'2020-01-01',departure:'2020-01-02',adults:2,childAges:[],roomId:rooms[54],segments:[{roomId:rooms[54],from:'2020-01-01',to:'2020-01-02'}]});
  const future=await insert('stay',{typeId:room.typeId,state:'reserved',arrival:'2027-01-01',departure:'2027-01-02',adults:2,childAges:[],segments:[]});
  const group=await insert('block',{typeId:room.typeId,kind:'group',state:'released',pickedStayIds:[overdue]});
  const maintenance=await insert('block',{roomId:rooms[54],kind:'maintenance',state:'active',from:'2020-01-01'});
  const item=await insert('item',{productId:'retained-item',warehouseId:'retained-warehouse'}),credit=await insert('credit',{partyId:'retained-party'});
  const historical:string[]=[];for(let n=0;n<60;n++)historical.push(await insert('stay',{typeId:room.typeId,state:'checked_out',arrival:'2020-01-01',departure:'2020-01-02',segments:[]}));
  historical.push(await insert('entry',{guestAccountId:'closed-history',total:100}),await insert('reservation',{state:'completed'}),await insert('guest_account',{state:'closed'}));
  const otherBranch=crypto.randomUUID();await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Other inventory branch',?,?,?)").bind(otherBranch,otherBranch,fixture.company,fixture.company,otherBranch).run();
  const otherProperty=(await pmsCommand(db,fixture.owner,{operation:'property.create',input:{name:'Other inventory',branchId:otherBranch,businessDate:'2026-09-01'}},crypto.randomUUID())).id;
  const foreign=crypto.randomUUID();await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'room',?,1,1)").bind(foreign,otherProperty,JSON.stringify({number:'FOREIGN',active:true,typeId:room.typeId})).run();
  const ui=await loadPms(db,fixture.owner,propertyId,false,pmsListSelection('rooms',{})!);expect(ui.listMeta!.ids).not.toContain(rooms[54]);
  const measured=await measureReads(()=>readPmsInventoryCommand(db,propertyId)),facts=measured.result,ids=facts.map(x=>x.id);
  expect(measured.metrics.queries).toBe(1);expect(facts.filter(x=>x.kind==='room')).toHaveLength(56);
  expect(ids).toEqual(expect.arrayContaining([...rooms,overdue,future,group,maintenance,item,credit,room.typeId,room.rateId]));
  expect(facts.find(x=>x.id===overdue)?.data).toMatchObject({segments:[{roomId:rooms[54],from:'2020-01-01',to:'2020-01-02'}]});
  expect(ids.filter(id=>historical.includes(id)||id===foreign)).toEqual([]);
  expect(await db.prepare("SELECT count(*)::int n FROM pms_records WHERE property_id=? AND kind='stay' AND data_json::jsonb->>'state'='checked_out' AND version=1 AND updated_at=1").bind(propertyId).first('n')).toBe(60);
});


it("protects off-page room assignments, duplicate numbers and maintenance capacity under inventory command locks", async () => {
  const room=await seedRooms(),rooms=[];
  for(let n=0;n<55;n++)rooms.push(await insert('room',{number:`2${String(n).padStart(2,'0')}`,typeId:room.typeId,active:true,cleanliness:'inspected'}));
  const overdue=await insert('stay',{typeId:room.typeId,state:'in_house',arrival:'2020-01-01',departure:'2020-01-02',adults:2,childAges:[],roomId:rooms[54],segments:[{roomId:rooms[54],from:'2020-01-01',to:'2020-01-02'}]});
  const maintenance=await insert('block',{roomId:rooms[53],typeId:room.typeId,kind:'maintenance',state:'active',from:'2020-01-01',to:'2020-01-02',pickedStayIds:[]});
  // Already-open synthetic operations; Finance setup acceptance remains in pms.test.ts.
  await db.prepare("UPDATE pms_properties SET state='open' WHERE id=?").bind(propertyId).run();
  const original=await loadPms(db,fixture.owner,propertyId),version=original.property.version;
  const run=(operation:string,input:unknown,expectedVersion=version,idempotencyKey=crypto.randomUUID())=>pmsCommand(db,fixture.owner,{operation,propertyId,expectedVersion,input},idempotencyKey);
  await expect(run('room_type.save',{id:room.typeId,name:'Too small',capacity:1})).rejects.toMatchObject({code:'PMS_CAPACITY_IN_USE'});
  await expect(run('room.save',{id:rooms[54],number:'254',typeId:room.typeId,active:false})).rejects.toMatchObject({code:'PMS_ROOM_IN_USE'});
  await expect(run('room.save',{id:room.roomId,number:'254',typeId:room.typeId,active:true})).rejects.toMatchObject({code:'PMS_ROOM_NUMBER_EXISTS'});
  await expect(run('block.create',{name:'Over capacity',typeId:room.typeId,from:'2026-09-02',to:'2026-09-03',releaseDate:'2026-09-02',quantity:56})).rejects.toMatchObject({code:'PMS_SOLD_OUT'});
  await expect(run('block.release',{id:maintenance})).rejects.toMatchObject({code:'PMS_BLOCK_TYPE'});
  await expect(run('room_type.save',{id:room.typeId,name:'Stale',capacity:2},version+1)).rejects.toMatchObject({code:'PMS_VERSION_CONFLICT'});
  await expect(run('room.save',{id:crypto.randomUUID(),number:'MISSING',typeId:room.typeId,active:true})).rejects.toMatchObject({code:'PMS_RECORD_NOT_FOUND'});
  const after=await loadPms(db,fixture.owner,propertyId);expect(after.property.version).toBe(version);expect(after.records).toEqual(original.records);
  expect(after.records.find(x=>x.id===overdue)?.version).toBe(1);
  const creationKey=crypto.randomUUID(),creationInput={name:'Full available capacity',typeId:room.typeId,from:'2026-09-02',to:'2026-09-03',releaseDate:'2026-09-02',quantity:55};
  const created=await run('block.create',creationInput,version,creationKey);
  expect(created.version).toBe(version+1);
  const released=await run('block.release',{id:created.id},created.version);expect(released.version).toBe(created.version+1);
  expect((await loadPms(db,fixture.owner,propertyId)).records.find(x=>x.id===created.id)?.data).toMatchObject({state:'released',quantity:55});
  expect(await run('block.create',creationInput,version,creationKey)).toEqual(created);
  expect((await loadPms(db,fixture.owner,propertyId)).property.version).toBe(released.version);
});

it('keeps room and supply context while withholding pricing and upgrade reasons from housekeeping reads', async () => {
  const room = await seedRooms();
  const item = await insert('item', { name: 'Minibar water', price: 7500, kind: 'minibar', active: true });
  const stay = await insert('stay', { ...room, reservationId: 'private-booking', guestId: 'private-guest', guestAccountId: 'private-account', arrival: '2026-09-01', departure: '2026-09-03', state: 'in_house', prices: [{ date: '2026-09-01', total: 10000 }], segments: [], upgrades: [{ effectiveDate: '2026-09-01', reason: 'Private guest negotiation', actorId: 'private-actor' }] });
  const cleaner = await fixture.member('pricing-private-cleaner', 'pms_housekeeping');
  for (const selection of [undefined, { view: 'setup' } as const, { view: 'detail', id: room.roomId } as const, pmsListSelection('housekeeping', {})!]) {
    const workspace = await pmsWorkspace(cleaner, propertyId, false, false, selection);
    expect(records(workspace.snapshot!, 'rate')).toEqual([]);
    expect(records(workspace.snapshot!, 'item').every(row => row.data.price === 0)).toBe(true);
    expect(records(workspace.snapshot!, 'stay').every(row => row.data.prices.length === 0 && !row.data.upgrades)).toBe(true);
    expect(JSON.stringify(workspace.snapshot)).not.toContain('Private guest negotiation');
  }
  const owner = await pmsWorkspace(fixture.owner, propertyId);
  expect(records(owner.snapshot!, 'rate')[0].data.price).toBe(10000);
  expect(records(owner.snapshot!, 'item').find(row => row.id === item)?.data.price).toBe(7500);
  expect(records(owner.snapshot!, 'stay').find(row => row.id === stay)?.data.upgrades?.[0].reason).toBe('Private guest negotiation');
});

it('retains legacy PMS rows and rejects malformed optional upgrade and pending-quota containers', async () => {
  const original = { state: 'checked_out', arrival: '2026-08-01', departure: '2026-08-02', prices: [] };
  const legacy = await insert('stay', original);
  expect(await db.prepare('SELECT data_json FROM pms_records WHERE id=?').bind(legacy).first('data_json')).toBe(JSON.stringify(original));
  await expect(insert('stay', { ...original, roomTypePeriods: {} })).rejects.toMatchObject({ code: '23514' });
  await expect(insert('stay', { ...original, upgrades: 'invalid' })).rejects.toMatchObject({ code: '23514' });
  await expect(insert('block', { kind: 'group', state: 'active', quotaOnly: true })).rejects.toMatchObject({ code: '23514' });
  const marker = await insert('block', { kind: 'group', state: 'active', quotaOnly: true, groupPickupClaims: [], expiresAt: 1000, bookingChangeId: 'change' });
  expect(await db.prepare('SELECT id FROM pms_records WHERE id=?').bind(marker).first('id')).toBe(marker);
});

it('offers bounded searchable guest and group choices without history or foreign-property data', async () => {
 const guests=[];for(let n=0;n<65;n++)guests.push(await insert('guest',{name:`Choice guest ${String(n).padStart(2,'0')}`,email:`choice${n}@example.test`,documentRef:'private document'}));
 const group=await insert('block',{kind:'group',name:'Choice active allocation',state:'active',from:'2026-09-10',to:'2026-09-12',releaseDate:'2026-09-09',quantity:2});
 const released=await insert('block',{kind:'group',name:'Released allocation',state:'released'});
 const internal=await insert('block',{kind:'group',name:'Internal claim',state:'active',bookingChangeId:'internal-review'});
 const history=await insert('stay',{guestId:guests[64],state:'checked_out',arrival:'2020-01-01',departure:'2020-01-02',prices:[{total:999}]});
 const otherBranch=crypto.randomUUID();await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Other choice branch',?,?,?)").bind(otherBranch,otherBranch,fixture.company,fixture.company,otherBranch).run();
 const otherProperty=(await pmsCommand(db,fixture.owner,{operation:'property.create',input:{name:'Other choice hotel',branchId:otherBranch,businessDate:'2026-09-01'}},crypto.randomUUID())).id;
 const foreignGuest=crypto.randomUUID(),foreignGroup=crypto.randomUUID();for(const [id,kind,data]of [[foreignGuest,'guest',{name:'Choice guest 64',email:'foreign@example.test',documentRef:'foreign document'}],[foreignGroup,'block',{kind:'group',name:'Choice active allocation',state:'active'}]]as const)await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,?,?,1,1)").bind(id,otherProperty,kind,JSON.stringify(data)).run();
 const select=(kind:'guest'|'block',q='',selectedId?:string)=>pmsReadSelectionSchema.parse({view:'choices',kind,q,selectedId});
 const first=await loadPms(db,fixture.owner,propertyId,false,select('guest'));expect(records(first,'guest')).toHaveLength(50);expect(first.records.some(r=>r.id===history)).toBe(false);expect(records(first,'guest').every(r=>!('documentRef' in r.data))).toBe(true);
 const found=await loadPms(db,fixture.owner,propertyId,false,select('guest','Choice guest 64'));expect(found.records.map(r=>r.id)).toEqual([guests[64]]);
 const selected=await loadPms(db,fixture.owner,propertyId,false,select('guest','does not match',guests[64]));expect(selected.records.map(r=>r.id)).toEqual([guests[64]]);
 const groups=await loadPms(db,fixture.owner,propertyId,false,select('block'));expect(groups.records.map(r=>r.id)).toEqual([group]);expect(groups.records.some(r=>r.id===released||r.id===internal)).toBe(false);
 expect(records(found,'guest')[0].data).toEqual({name:'Choice guest 64',email:'choice64@example.test',phone:''});expect(records(groups,'block')[0].data).toEqual({name:'Choice active allocation',kind:'group',state:'active'});
 for(const [kind,id,q]of [['guest',foreignGuest,'Choice guest 64'],['block',foreignGroup,'Choice active allocation']]as const){const scoped=await loadPms(db,fixture.owner,propertyId,false,select(kind,q,id));expect(scoped.records.some(row=>row.id===id)).toBe(false);expect(scoped.records.map(row=>row.id)).toEqual([kind==='guest'?guests[64]:group]);}
 const cleaner=await fixture.member('choices-cleaner','pms_housekeeping');await expect(loadPms(db,cleaner,propertyId,false,select('guest'))).rejects.toMatchObject({status:403});await expect(loadPms(db,cleaner,propertyId,false,select('block'))).rejects.toMatchObject({status:403});
 await expect(loadPms(db,{...fixture.owner,companyId:'foreign'},propertyId,false,select('guest'))).rejects.toMatchObject({status:404});await expect(loadPms(db,{...fixture.owner,branchId:'foreign'},propertyId,false,select('block'))).rejects.toMatchObject({status:404});
 await expect(loadPms(db,fixture.owner,propertyId,true,select('guest'))).rejects.toThrow('PMS_COMMAND_REQUIRES_COMPLETE_STATE');
});
