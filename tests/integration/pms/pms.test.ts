import { PostgresDatabase } from "../../../src/server/modules/runtime/adapters/runtime/postgres";
import { prepareRoomCharge } from "../../../src/lib/pms/pms-room-charge";
import { frontOperationAllowed } from "../../../src/server/modules/work_projection/public/front-app";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { pmsCommand, loadPms, drainPms, pmsWorkspace, pmsWork, applyPosRoomCharge, reversePosRoomCharge, persistPmsRecords } from "../../../src/server/modules/pms/public/pms";
import {
  records,
  record,
  pmsAccountKeys,
  defaultConfig,
  amounts,
  type Config,
} from "../../../src/server/modules/pms/public/rules/pms";
import { availability, dailyRoomReport } from "../../../src/lib/pms/pms-engine";
import { pmsHome,pmsTaskPage,pmsHotelSources } from "../../../src/server/modules/pms/public/pms/home";
import { pmsFinanceOverview } from "../../../src/server/modules/pms/public/pms/finance";
import { readablePmsSource } from "../../../src/server/modules/pms/public/pms/source";
import { previewPmsImport,approvePmsImport,applyPmsImport } from "../../../src/server/modules/pms/public/pms/import";
import { publishHandover,acknowledgeHandover,handoverView } from "../../../src/server/modules/operations/public/handover";
import { homePosts,postView } from "../../../src/server/modules/collaboration/public/social-home";
import { projectPmsSocialEvent } from "../../../src/server/modules/pms/public/pms/social-events";
import type { EventMessage } from "../../../src/server/modules/workflow/public/job-projector";
import { createJournal,postJournal,previewOpeningTrialBalance,importOpeningTrialBalance,openingTrialBalanceSchema } from "../../../src/server/modules/finance/public/finance";
import { assignFlowPrimary,resolveFlow } from "../../../src/server/modules/workflow/public/flow-routing";
import { confirmManualRate } from "../../../src/server/modules/finance/public/company-money";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import {
  attachmentBindingState,
  enableAttachmentV2ForTest,
  seedAttachmentSet,
} from "../../support/helpers/attachment-v2";

it("allows staff payout of approved PMS refunds while keeping independent decisions in ERPX", () => {
  expect(frontOperationAllowed("pms", "refund.pay")).toBe(true);
  for (const operation of ["refund.approve", "refund.reject", "payment.verify_transfer", "shift.review"])
    expect(frontOperationAllowed("pms", operation)).toBe(false);
  expect(frontOperationAllowed("pos", "refund.pay")).toBe(false);
});

vi.mock("../../../src/server/modules/runtime/adapters/runtime", async () => {
  const { env } = await import("../../support/runtime-env");
  return { holdingResources: async () => ({ write: env.H001_DB }) };
});
const db = env.H001_DB,
  key = () => crypto.randomUUID();
let f: Awaited<ReturnType<typeof seedExpenseCompany>>,
  owner: WorkContext,
  reviewer: WorkContext,
  operationalReviewer: WorkContext,
  propertyId: string,
  typeId: string,
  rateId: string,
  roomId: string,
  guestId: string,
  accounts: Config["accounts"];
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  enableAttachmentV2ForTest();
});
afterEach(() => vi.unstubAllEnvs());
async function snapshot() {
  return loadPms(db, owner, propertyId);
}
async function cmd(operation: string, input: unknown = {}, actor = owner) {
  const s = await snapshot();
  if (actor === reviewer && ["refund.approve","refund.reject","credit.approve","charge.correction.review"].includes(operation)) actor = operationalReviewer;
  return pmsCommand(
    db,
    actor,
    { propertyId, expectedVersion: s.property.version, operation, input },
    key(),
  );
}
async function projectCommittedEvent(eventType:string) {
  const event=await db.prepare("SELECT * FROM outbox_events WHERE event_type=? AND payload_json::jsonb->>'companyId'=? ORDER BY created_at DESC,id LIMIT 1")
    .bind(eventType,f.company).first<{id:string;event_type:string;aggregate_id:string;aggregate_version:number;payload_json:string}>();
  if(!event)throw new Error("Expected committed PMS event");
  const message={id:event.id,holdingId:"h001",eventType:event.event_type,aggregateId:event.aggregate_id,aggregateVersion:event.aggregate_version,payload:JSON.parse(event.payload_json)} satisfies EventMessage;
  await projectPmsSocialEvent(db,message);
  return message;
}
beforeEach(async context => {
  f = await seedExpenseCompany(db, env.H001_FILES, !context.task.name.startsWith("blocks a new booking"));
  owner = f.owner;
  reviewer = await f.member("hotel-reviewer", "finance_approver");
  operationalReviewer = await f.member("hotel-manager", "pms_manager");
  propertyId = (
    await pmsCommand(
      db,
      owner,
      {
        operation: "property.create",
        input: {
          name: "Test Hotel",
          branchId: f.branch,
          businessDate: "2026-09-01",
        },
      },
      key(),
    )
  ).id;
  accounts = {} as Config["accounts"];
  for (const [i, k] of pmsAccountKeys.entries()) {
    const id = key();
    accounts[k] = id;
    const category = ["revenue", "service"].includes(k)
      ? "revenue"
      : ["advance", "deposit", "tax"].includes(k)
        ? "liability"
        : ["consumption", "commission", "cashDifference"].includes(k)
          ? "expense"
          : "asset";
    await db
      .prepare(
        "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',1,1)",
      )
      .bind(id, f.chart, f.company, `PMS-${i}`, k, category)
      .run();
  }
  await cmd("accounting.save", {
    chartId: f.chart,
    accounts,
    taxBps: 700,
    serviceBps: 0,
    taxInclusive: false,
    advanceTax: true,
  });
  await cmd("accounting.approve", {}, reviewer);
  typeId = (
    await cmd("room_type.save", {
      name: "Deluxe",
      capacity: 2,
      amenities: "Shower",
    })
  ).id;
  roomId = (await cmd("room.save", { number: "101", typeId, active: true })).id;
  if (context.task.name.startsWith("blocks a new booking")) {
    // An explicitly declared pre-ADR price fixture lets the new booking guard
    // prove that legacy prices cannot invent an absent historical FX quote.
    rateId = `legacy-pms-rate-${key()}`;
    await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'rate',?,1,1)")
      .bind(rateId, propertyId, JSON.stringify({name:"Legacy Flexible",typeId,from:"2026-09-01",to:"2026-12-31",price:10000,extraAdult:0,includedGuests:2,minNights:1,maxNights:365,stopSell:false,closedArrival:false,closedDeparture:false,market:"public",partyId:"",package:"",active:true})).run();
  } else rateId = (
    await cmd("rate.save", {
      name: "Flexible",
      typeId,
      from: "2026-09-01",
      to: "2026-12-31",
      price: 10000,
    })
  ).id;
  await cmd("property.enable");
  guestId = (await cmd("guest.save", { name: "Test Guest" })).id;
});
async function book(
  arrival = "2026-09-01",
  departure = "2026-09-02",
  extra: Record<string, unknown> = {},
) {
  const r = await cmd("reservation.create", {
    guestId,
    source: "direct",
    rooms: [{ typeId, rateId, arrival, departure, adults: 2, quantity: 1 }],
    ...extra,
  });
  const s = await snapshot();
  const stay = records(s, "stay").find((x) => x.data.reservationId === r.id)!;
  return { reservationId: r.id, stayId: stay.id, guestAccountId: stay.data.guestAccountId };
}
it("batches changed PMS records in input order while retaining row counters, multiple flushes and rollback", async () => {
  const booking = await book(), s = await snapshot();
  const original = new Map(s.records.map(row => [row.id, JSON.stringify(row)]));
  const stay = record(s, "stay", booking.stayId), reservation = record(s, "reservation", booking.reservationId);
  const guest = structuredClone(record(s, "guest", guestId)); guest.id=key();guest.version=1;
  stay.data.state="in_house";stay.version++;stay.updatedAt++;
  reservation.data.source="batch-proof";reservation.version++;reservation.updatedAt++;
  s.records=[stay,reservation,guest];
  const revision = async () => Number(await db.prepare("SELECT revision FROM work_cache_revisions WHERE table_name='pms_records'").first("revision"));
  const observations = async () => (await db.prepare("SELECT record_id FROM pms_booking_observations WHERE record_id IN (?,?) ORDER BY observation_id")
    .bind(stay.id,reservation.id).all<{record_id:string}>()).results.map(row=>row.record_id);
  const before=await revision(), observedBefore=(await observations()).length;
  const {measureReads}=await import("../../../src/server/modules/runtime/adapters/runtime/read-metrics");
  await db.transaction(async tx => {
    await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
    await tx.prepare("SELECT id FROM pms_properties WHERE id=? FOR UPDATE").bind(propertyId).first();
    const measured=await measureReads(()=>persistPmsRecords(tx,s,original));
    expect(measured.metrics.queries).toBe(1);
    for(const row of s.records)expect(original.get(row.id)).toBe(JSON.stringify(row));
    guest.data.name="Second flush guest";guest.version++;guest.updatedAt++;
    const second=await measureReads(()=>persistPmsRecords(tx,s,original));
    expect(second.metrics.queries).toBe(1);
    const replay=await measureReads(()=>persistPmsRecords(tx,s,original));
    expect(replay.metrics.queries).toBe(0);
  });
  expect(await revision()).toBe(before+4);
  expect((await observations()).slice(observedBefore)).toEqual([stay.id,reservation.id]);
  const saved = async () => (await db.prepare("SELECT id,property_id,kind,data_json,version,updated_at FROM pms_records WHERE id IN (?,?,?) ORDER BY id")
    .bind(stay.id,reservation.id,guest.id).all()).results;
  const committed=await saved(), committedRevision=await revision(), committedObservations=await observations();
  expect(committed).toEqual(expect.arrayContaining(s.records.map(row=>({id:row.id,property_id:propertyId,kind:row.kind,data_json:JSON.stringify(row.data),version:row.version,updated_at:row.updatedAt}))));
  await expect(db.transaction(async tx=>{
    await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
    await tx.prepare("SELECT id FROM pms_properties WHERE id=? FOR UPDATE").bind(propertyId).first();
    guest.data.name="Rolled back guest";guest.version++;
    reservation.data.source="rolled-back-source";reservation.version++;
    await persistPmsRecords(tx,s,original);
    throw Error("SIMULATED_LATE_OWNER_FAILURE");
  })).rejects.toThrow("SIMULATED_LATE_OWNER_FAILURE");
  expect(await saved()).toEqual(committed);expect(await revision()).toBe(committedRevision);
  expect(await observations()).toEqual(committedObservations);
});
it("keeps multi-chunk PMS writes atomic under a real later SQL failure and supports replay", async () => {
  const s=await snapshot(), template=record(s,"guest",guestId);
  const guests=Array.from({length:501},(_,index)=>({...structuredClone(template),id:key(),version:1,data:{...structuredClone(template.data),name:`Mock batch guest ${index}`}}));
  s.records=guests;
  const revision=async()=>Number(await db.prepare("SELECT revision FROM work_cache_revisions WHERE table_name='pms_records'").first("revision"));
  const count=async()=>Number(await db.prepare("SELECT COUNT(*)::int n FROM pms_records WHERE id IN (SELECT jsonb_array_elements_text(?::jsonb))").bind(JSON.stringify(guests.map(row=>row.id))).first("n"));
  const {measureReads}=await import("../../../src/server/modules/runtime/adapters/runtime/read-metrics");
  const before=await revision();
  await expect(db.transaction(async tx=>{
    await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
    await tx.prepare("SELECT id FROM pms_properties WHERE id=? FOR UPDATE").bind(propertyId).first();
    const measured=await measureReads(()=>persistPmsRecords(tx,s,new Map()));expect(measured.metrics.queries).toBe(2);
    await tx.prepare("SELECT 1/0").first();
  })).rejects.toMatchObject({code:"22012"});
  expect(await count()).toBe(0);expect(await revision()).toBe(before);
  await db.transaction(async tx=>{
    await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
    await tx.prepare("SELECT id FROM pms_properties WHERE id=? FOR UPDATE").bind(propertyId).first();
    const original=new Map<string,string>();
    const measured=await measureReads(()=>persistPmsRecords(tx,s,original));expect(measured.metrics.queries).toBe(2);
    const replay=await measureReads(()=>persistPmsRecords(tx,s,original));expect(replay.metrics.queries).toBe(0);
  });
  expect(await count()).toBe(501);expect(await revision()).toBe(before+501);
});
it("orders PMS and booking-history cache locks across concurrent hotel writes", async () => {
  const booking = await book();
  const otherBranch = key();
  await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Concurrent mock branch',?,?,?)")
    .bind(otherBranch,otherBranch,f.company,f.company,otherBranch).run();
  const otherProperty = (await pmsCommand(db, owner, { operation: "property.create", input: {
    name: "Concurrent mock hotel", branchId: otherBranch, businessDate: "2026-09-01",
  } }, key())).id;
  const otherStay = key(), otherReservation = key();
  // A second hotel's record is a disposable dependency fixture, not an opening
  // balance or historical GL. Both writes exercise the real database triggers.
  await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) SELECT ?,?,'reservation',data_json,version,updated_at FROM pms_records WHERE id=?")
    .bind(otherReservation, otherProperty, booking.reservationId).run();
  await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) SELECT ?,?,'stay',jsonb_set(data_json::jsonb,'{reservationId}',to_jsonb(?::text))::text,version,updated_at FROM pms_records WHERE id=?")
    .bind(otherStay, otherProperty, otherReservation, booking.stayId).run();
  const revisions = async () => (await db.prepare("SELECT table_name,revision FROM work_cache_revisions WHERE table_name IN ('pms_records','pms_booking_observations') ORDER BY table_name")
    .all<{table_name:string;revision:number}>()).results;
  const before = await revisions();
  const observationCount = async () => Number(await db.prepare("SELECT COUNT(*)::int n FROM pms_booking_observations WHERE record_id IN (?,?)")
    .bind(booking.stayId, otherStay).first("n"));
  const beforeObservations = await observationCount();
  const a = await db.pool.connect(), b = await db.pool.connect();
  try {
    // READ COMMITTED isolates lock ordering from the separately tested
    // serializable snapshot retry policy. A real queued lock is the barrier.
    await a.query("BEGIN"); await b.query("BEGIN");
    await a.query("SET LOCAL lock_timeout='5s'"); await b.query("SET LOCAL lock_timeout='5s'");
    const pid = Number((await b.query("SELECT pg_backend_pid() pid")).rows[0].pid);
    await a.query("UPDATE pms_records SET data_json=jsonb_set(data_json::jsonb,'{history}','[]'::jsonb)::text,version=version+1 WHERE id=$1", [booking.reservationId]);
    const queued = b.query("UPDATE pms_records SET data_json=jsonb_set(data_json::jsonb,'{state}','\"in_house\"'::jsonb)::text,version=version+1 WHERE id=$1", [otherStay])
      .then(() => undefined, error => error as Error);
    let waiting = false;
    for (let i=0;i<100&&!waiting;i++) {
      waiting = (await db.pool.query("SELECT cardinality(pg_blocking_pids($1))>0 waiting", [pid])).rows[0].waiting === true;
      if (!waiting) await new Promise(resolve => setTimeout(resolve, 5));
    }
    expect(waiting).toBe(true);
    let firstError: unknown;
    try {
      await a.query("UPDATE pms_records SET data_json=jsonb_set(data_json::jsonb,'{state}','\"in_house\"'::jsonb)::text,version=version+1 WHERE id=$1", [booking.stayId]);
      await a.query("COMMIT");
    } catch (error) { firstError=error; await a.query("ROLLBACK"); }
    const secondError = await queued;
    if (secondError) await b.query("ROLLBACK"); else await b.query("COMMIT");
    expect(firstError).toBeUndefined(); expect(secondError).toBeUndefined();
  } finally {
    await a.query("ROLLBACK").catch(() => {}); await b.query("ROLLBACK").catch(() => {});
    a.release(); b.release();
  }
  expect(await observationCount()).toBe(beforeObservations+2);
  const after = await revisions();
  expect(after.find(row => row.table_name==='pms_records')!.revision).toBe(before.find(row => row.table_name==='pms_records')!.revision+3);
  expect(after.find(row => row.table_name==='pms_booking_observations')!.revision).toBe(before.find(row => row.table_name==='pms_booking_observations')!.revision+2);
});
it("blocks a new booking when the selected currency lacks the hotel's account setup or manual rate", async () => {
  await expect(cmd("rate.save", { name: "Unquoted price", typeId, from: "2026-09-01", to: "2026-12-31", price: 10000 })).rejects.toMatchObject({ code: "MANUAL_RATE_REQUIRED" });
  expect(await db.prepare("SELECT COUNT(*)::int n FROM pms_records WHERE property_id=? AND kind='rate'").bind(propertyId).first("n")).toBe(1);
  await expect(book("2026-09-01", "2026-09-02", { currency: "USD" })).rejects.toMatchObject({ code: "PMS_CURRENCY_SETUP_REQUIRED" });
  await expect(book("2026-09-01", "2026-09-02", { currency: "THB" })).rejects.toMatchObject({ code: "MANUAL_RATE_REQUIRED" });
  const rateId=`pms-rate-${key()}`;
  await db.prepare("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES(?,?,?,?,?,?,?,?)")
    .bind(rateId, f.company, "THB", "2026-09-01", "36", reviewer.userId, Date.now(), "manual test quote").run();
  const booked=await book("2026-09-01", "2026-09-02", { currency: "THB" });
  expect(await db.prepare("SELECT currency,amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE company_id=? AND source_type='pms_reservation' AND source_id=?")
    .bind(f.company,booked.reservationId).first()).toMatchObject({currency:"THB",amount_units:"10700",usd_units:"297",fx_rate_id:rateId});
});
it("preserves reviewed night prices and original deposit FX through concurrent partial refunds and reload", async () => {
  const booked = await book();
  await checkIn(booked.stayId);
  const shift = (await cmd("shift.open", { name: "Original-rate cash desk", opening: 0 })).id;
  const receipt = await cmd("payment.deposit", { guestAccountId: booked.guestAccountId, amount: 32100, method: "cash", shiftId: shift });
  const original = await db.prepare("SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE company_id=? AND source_type='pms_entry' AND source_id=? AND amount_kind='total'").bind(f.company, receipt.id).first<{ amount_units:string;usd_units:string;fx_rate_id:string }>();
  expect(original).toMatchObject({ amount_units: "32100", usd_units: "892" });
  await confirmManualRate(reviewer, { currency: "THB", effectiveDate: "2026-09-01", currencyPerUsd: "40", note: "Later rate must not replace settled deposit or booked room evidence" }, key());
  for (let i = 0; i < 3; i++) {
    const refund = await cmd("refund.request", { guestAccountId: booked.guestAccountId, pocket: "deposit", amount: 10700, reason: `Reviewed original payment return ${i + 1}` });
    await expect(cmd("refund.approve", { id: refund.id })).rejects.toMatchObject({ code: "PMS_INDEPENDENT_APPROVER" });
    await cmd("refund.approve", { id: refund.id }, reviewer);
    const current = await snapshot(), input = { propertyId, expectedVersion: current.property.version, operation: "refund.pay", input: { id: refund.id, method: "cash", shiftId: shift } };
    const commandIds = [key(), key()];
    const attempts = await Promise.allSettled(commandIds.map(commandId => pmsCommand(db, owner, input, commandId)));
    const committed = attempts.findIndex(result => result.status === "fulfilled");
    expect(attempts.filter(result => result.status === "fulfilled")).toHaveLength(1);
    expect(attempts.filter(result => result.status === "rejected")).toHaveLength(1);
    expect(await pmsCommand(db, owner, input, commandIds[committed])).toEqual((attempts[committed] as PromiseFulfilledResult<unknown>).value);
  }
  const reversals = (await db.prepare("SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE company_id=? AND original_source_type='pms_entry' AND original_source_id=? ORDER BY captured_at,source_id").bind(f.company, receipt.id).all<{amount_units:string;usd_units:string;fx_rate_id:string}>()).results;
  expect(reversals.map(value => value.amount_units)).toEqual(["-10700", "-10700", "-10700"]);
  expect(reversals.map(value => value.usd_units)).toEqual(["-297", "-298", "-297"]);
  expect(reversals.every(value => value.fx_rate_id === original!.fx_rate_id)).toBe(true);
  expect(record(await snapshot(), "guest_account", booked.guestAccountId).data.deposit).toBe(0);
  expect(record(await snapshot(), "shift", shift).data.cash).toBe(0);
  expect(await db.prepare("SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE company_id=? AND source_type='pms_entry' AND source_id=? AND amount_kind='total'").bind(f.company, receipt.id).first()).toEqual(original);
  const paid = await cmd("payment.receive", { guestAccountId: booked.guestAccountId, amount: 10700, method: "cash", shiftId: shift });
  expect(await db.prepare("SELECT usd_units::text FROM money_transaction_snapshots WHERE company_id=? AND source_type='pms_entry' AND source_id=? AND amount_kind='total'").bind(f.company, paid.id).first("usd_units")).toBe("268");
  await cmd("stay.check_out", { id: booked.stayId });
  const charged = records(await snapshot(), "entry").find(row => row.data.sourceKey === `night:${booked.stayId}:2026-09-01`)!;
  expect(await db.prepare("SELECT amount_units::text,usd_units::text,fx_rate_id,carried_source_type FROM money_transaction_snapshots WHERE company_id=? AND source_type='pms_entry' AND source_id=? AND amount_kind='total'").bind(f.company, charged.id).first())
    .toEqual({amount_units:"10700",usd_units:"297",fx_rate_id:original!.fx_rate_id,carried_source_type:"pms_stay_price"});
  expect(record(await snapshot(), "stay", booked.stayId).data.state).toBe("checked_out");
  expect(record(await snapshot(), "guest_account", booked.guestAccountId).data).toMatchObject({due:0,advance:0,deposit:0});
  expect(await db.prepare("SELECT COALESCE(SUM(l.debit_minor-l.credit_minor),0)::text difference FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.status='posted'").bind(f.company).first("difference")).toBe("0");
});
async function clean() {
  const t = (
    await cmd("task.create", {
      roomId,
      type: "arrival",
      date: (await snapshot()).property.businessDate,
    })
  ).id;
  const task = record(await snapshot(), "task", t);
  await cmd("task.complete", { id: t, checked: task.data.checklist });
  const inspector = await f.member(`inspector-${key()}`, "pms_supervisor");
  await cmd("task.inspect", { id: t }, inspector);
}
async function checkIn(stayId: string) {
  await clean();
  await cmd("stay.assign", { id: stayId, roomId });
  await cmd("stay.check_in", { id: stayId });
}
async function balance(account: string) {
  const r = await db
    .prepare(
      "SELECT COALESCE(SUM(l.debit_minor-l.credit_minor),0) n FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND l.account_id=?",
    )
    .bind(f.company, account)
    .first<{ n: number }>();
  return Number(r!.n);
}

it("binds room-task photos in the PMS action transaction and leaves stale evidence draft", async () => {
  const taskId = (
    await cmd("task.create", {
      roomId,
      type: "arrival",
      date: (await snapshot()).property.businessDate,
    })
  ).id;
  const task = record(await snapshot(), "task", taskId);
  const evidence = await seedAttachmentSet(db, owner, {
    targetType: "pms_task",
    targetId: taskId,
    componentKind: "action",
    componentRef: "task.complete",
    purposeKey: "room_photo",
    expectedTargetVersion: task.version,
  });
  const current = await snapshot();
  await pmsCommand(
    db,
    owner,
    {
      propertyId,
      expectedVersion: current.property.version,
      operation: "task.complete",
      input: { id: taskId, checked: task.data.checklist },
      attachmentSetIds: [evidence.setId],
    },
    key(),
  );
  expect(await attachmentBindingState(db, evidence.setId)).toMatchObject({
    set_state: "bound",
    link_state: "bound",
    binding_type: "pms_task_completion",
  });

  const staleTaskId = (
    await cmd("task.create", {
      roomId,
      type: "stayover",
      date: (await snapshot()).property.businessDate,
    })
  ).id;
  const staleTask = record(await snapshot(), "task", staleTaskId);
  const stale = await seedAttachmentSet(db, owner, {
    targetType: "pms_task",
    targetId: staleTaskId,
    componentKind: "action",
    componentRef: "task.complete",
    purposeKey: "damage_photo",
    expectedTargetVersion: staleTask.version,
  });
  await cmd("task.start", { id: staleTaskId });
  const afterStart = await snapshot();
  await expect(
    pmsCommand(
      db,
      owner,
      {
        propertyId,
        expectedVersion: afterStart.property.version,
        operation: "task.complete",
        input: { id: staleTaskId, checked: staleTask.data.checklist },
        attachmentSetIds: [stale.setId],
      },
      key(),
    ),
  ).rejects.toMatchObject({ status: 409, code: "ATTACHMENT_SET_STALE" });
  expect(await attachmentBindingState(db, stale.setId)).toMatchObject({
    set_state: "draft",
    link_state: "draft",
  });
  expect(record(await snapshot(), "task", staleTaskId).data.state).toBe("working");
});

it("binds lost-property handover evidence in the release transaction", async () => {
  const lostId = (
    await cmd("lost.save", {
      description: "Guest umbrella",
      roomId,
      guestId,
      location: "Front desk locker",
      foundDate: (await snapshot()).property.businessDate,
      notes: "Tagged by housekeeping",
    })
  ).id;
  const lost = record(await snapshot(), "lost", lostId);
  const handover = await seedAttachmentSet(db, owner, {
    targetType: "pms_lost_property",
    targetId: lostId,
    componentKind: "action",
    componentRef: "lost.release",
    purposeKey: "handover_evidence",
    expectedTargetVersion: lost.version,
  });
  const current = await snapshot();
  await pmsCommand(
    db,
    owner,
    {
      propertyId,
      expectedVersion: current.property.version,
      operation: "lost.release",
      input: { id: lostId, state: "returned", notes: "Released to the named guest" },
      attachmentSetIds: [handover.setId],
    },
    key(),
  );
  expect(await attachmentBindingState(db, handover.setId)).toMatchObject({
    set_state: "bound",
    link_state: "bound",
    binding_type: "pms_lost_property_release",
  });
  expect(record(await snapshot(), "lost", lostId).data.state).toBe("returned");
});

it("does not settle a production hotel transfer from a typed reference alone", async () => {
  const booking = await book();
  await checkIn(booking.stayId);
  const shift = (await cmd("shift.open", { name: "AM", opening: 0 })).id;
  const before = await snapshot();
  const accountBalance = await balance(accounts.bank);
  const { changeCompanyFeatureRollout } = await import("../../../src/server/modules/pms/public/hospitality-rollout");
  await changeCompanyFeatureRollout(owner, { moduleId: "pms", state: "pilot", expectedVersion: 0, reason: "Hotel pilot acceptance rehearsal" }, key());
  vi.stubEnv("NODE_ENV", "production");
  expect((await pmsWorkspace(owner, propertyId)).availablePaymentMethods).toEqual(["cash"]);
  const request = { propertyId, expectedVersion: before.property.version, operation: "payment.receive", input: { guestAccountId: booking.guestAccountId, amount: 10700, method: "transfer", reference: "TYPED-ONLY", shiftId: shift } };
  await expect(pmsCommand(db, owner, request, key())).rejects.toMatchObject({ code: "PMS_BANK_VERIFICATION_REQUIRED" });
  await expect(pmsCommand(db, owner, { ...request, operation: "payment.deposit" }, key())).rejects.toMatchObject({ code: "PMS_BANK_VERIFICATION_REQUIRED" });
  await expect(pmsCommand(db, owner, { ...request, input: { ...request.input, method: "edc" } }, key())).rejects.toMatchObject({ code: "PMS_BANK_VERIFICATION_REQUIRED" });
  await expect(pmsCommand(db, owner, { ...request, operation: "refund.pay", input: { id: "not-a-refund", method: "transfer", reference: "TYPED-ONLY", shiftId: shift } }, key())).rejects.toMatchObject({ code: "PMS_BANK_VERIFICATION_REQUIRED" });
  const after = await snapshot();
  expect(after.property.version).toBe(before.property.version);
  expect(records(after, "entry").length).toBe(records(before, "entry").length);
  expect(await balance(accounts.bank)).toBe(accountBalance);
  await expect(pmsCommand(db, owner, { ...request, input: { guestAccountId: booking.guestAccountId, amount: 10700, method: "cash", shiftId: shift } }, key())).resolves.toMatchObject({ propertyId });
});

it("settles a pending hotel transfer only after independent exact bank evidence", async () => {
  const booking=await book();
  await checkIn(booking.stayId);
  const shift=(await cmd("shift.open",{name:"AM",opening:0})).id;
  const {changeCompanyFeatureRollout}=await import("../../../src/server/modules/pms/public/hospitality-rollout");
  await changeCompanyFeatureRollout(owner,{moduleId:"pms",state:"pilot",expectedVersion:0,reason:"Hotel transfer pilot rehearsal"},key());
  vi.stubEnv("NODE_ENV","production");
  const initial=await snapshot();
  const request={propertyId,expectedVersion:initial.property.version,operation:"payment.request_transfer",input:{guestAccountId:booking.guestAccountId,shiftId:shift,amount:10700,reference:"PMS-BANK-001"}};
  const requestKey=key();
  const pending=await pmsCommand(db,owner,request,requestKey);
  expect(pending).toMatchObject({state:"pending",transferVersion:1});
  expect(await pmsCommand(db,owner,request,requestKey)).toEqual(pending);
  expect((await snapshot()).property.version).toBe(initial.property.version);
  expect(await balance(accounts.bank)).toBe(0);
  const {hospitalityDayControls}=await import("../../../src/server/modules/pms/public/hospitality-day-controls");
  const controls=await hospitalityDayControls(db,f.company,f.branch,propertyId,initial.property.businessDate);
  expect(controls.pendingHotelTransfers).toBe(1);
  expect(controls.blockers).toContain("1 hotel bank transfer(s) awaiting Finance verification");
  expect((await pmsWork(reviewer)).find((item) => item.id === pending.id)).toMatchObject({
    queue: "todo",
    responsibleQueue: "Finance approver",
    nextAction: "Match imported bank evidence to this hotel transfer",
  });
  expect((await pmsWork(owner)).find((item) => item.id === pending.id)).toMatchObject({queue:"waiting",canAct:false});
  await projectCommittedEvent("pms.payment.request_transfer");
  expect(await db.prepare("SELECT count(*) FROM staff_notifications WHERE company_id=? AND user_id=? AND record_type='post'")
    .bind(f.company,reviewer.userId).first<number>("count")).toBe(1);
  expect(await db.prepare("SELECT count(*) FROM staff_notifications WHERE company_id=? AND user_id=? AND record_type='post'")
    .bind(f.company,owner.userId).first<number>("count")).toBe(0);
  const statement=key(),bankId=key();
  await db.prepare("INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,source_hash,imported_by,created_at) VALUES(?,?,?,?,?,0,10700,?,?,?)").bind(statement,f.company,accounts.bank,"PMS-STATEMENT-1",initial.property.businessDate,key(),reviewer.userId,Date.now()).run();
  await db.prepare("INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,amount_minor,currency) VALUES(?,?,?,?,?,10600,'THB')").bind(bankId,statement,f.company,initial.property.businessDate,"PMS-BANK-001").run();
  const verify={propertyId,expectedVersion:initial.property.version,operation:"payment.verify_transfer",input:{id:pending.id,expectedTransferVersion:1,bankTransactionId:bankId,bankVersion:1}};
  await expect(pmsCommand(db,owner,verify,key())).rejects.toMatchObject({code:"PMS_INDEPENDENT_VERIFICATION"});
  await expect(pmsCommand(db,reviewer,verify,key())).rejects.toMatchObject({code:"PMS_BANK_EVIDENCE_MISMATCH"});
  expect(await balance(accounts.bank)).toBe(0);
  await db.prepare("UPDATE finance_bank_transactions SET amount_minor=10700 WHERE id=?").bind(bankId).run();
  for(const [sql,value,restore] of [
    ["UPDATE finance_bank_transactions SET external_id=? WHERE id=?","WRONG-REFERENCE","PMS-BANK-001"],
    ["UPDATE finance_bank_transactions SET currency=? WHERE id=?","USD","THB"],
    ["UPDATE finance_bank_transactions SET transaction_date=? WHERE id=?","2026-09-02",initial.property.businessDate],
  ] as const){
    await db.prepare(sql).bind(value,bankId).run();
    await expect(pmsCommand(db,reviewer,verify,key())).rejects.toMatchObject({code:"PMS_BANK_EVIDENCE_MISMATCH"});
    await db.prepare(sql).bind(restore,bankId).run();
  }
  await db.prepare("UPDATE finance_bank_statements SET bank_account=? WHERE id=?").bind(accounts.cash,statement).run();
  await expect(pmsCommand(db,reviewer,verify,key())).rejects.toMatchObject({code:"PMS_BANK_EVIDENCE_MISMATCH"});
  await db.prepare("UPDATE finance_bank_statements SET bank_account=? WHERE id=?").bind(accounts.bank,statement).run();
  const verifyKey=key();
  const result=await pmsCommand(db,reviewer,verify,verifyKey);
  expect(result).toMatchObject({id:pending.id,state:"verified",bankTransactionId:bankId,transferVersion:2});
  expect(await pmsCommand(db,reviewer,verify,verifyKey)).toEqual(result);
  if(!("entryId" in result)||!result.entryId)throw new Error("Verified transfer must retain its entry receipt");
  const transfer=await db.prepare("SELECT state,entry_id,finance_payment_id,bank_transaction_id FROM pms_transfer_requests WHERE id=?").bind(pending.id).first<{state:string;entry_id:string;finance_payment_id:string;bank_transaction_id:string}>();
  expect(transfer).toMatchObject({state:"verified",entry_id:result.entryId,bank_transaction_id:bankId});
  const bank=await db.prepare("SELECT match_status,matched_type,matched_id FROM finance_bank_transactions WHERE id=?").bind(bankId).first();
  expect(bank).toMatchObject({match_status:"matched",matched_type:"payment",matched_id:transfer!.finance_payment_id});
  const payment=await db.prepare("SELECT bank_account,amount_minor,status FROM finance_payments WHERE id=?").bind(transfer!.finance_payment_id).first();
  expect(payment).toMatchObject({bank_account:accounts.bank,status:"settled"});
  expect(Number(payment!.amount_minor)).toBe(10700);
  expect(await balance(accounts.bank)).toBe(10700);
  expect(records(await snapshot(),"entry").filter(row=>row.id===result.entryId)).toHaveLength(1);
  const verificationEvent=await projectCommittedEvent("pms.payment.verify_transfer");
  await projectPmsSocialEvent(db,verificationEvent);
  const notification=await db.prepare("SELECT record_id,message FROM staff_notifications WHERE company_id=? AND user_id=? AND record_type='post'")
    .bind(f.company,owner.userId).first<{record_id:string;message:string}>();
  expect(notification?.message).toContain("Bank transfer verified");
  expect((await postView(db,owner,notification!.record_id)).linkedSources[0]).toMatchObject({id:pending.id,available:true});
  expect(await db.prepare("SELECT count(*) FROM staff_notifications WHERE company_id=? AND user_id=? AND record_type='post'")
    .bind(f.company,owner.userId).first<number>("count")).toBe(1);
});

it("cancels an unverified hotel transfer without changing guest account or ledger", async () => {
  const booking=await book();
  await checkIn(booking.stayId);
  const shift=(await cmd("shift.open",{name:"AM",opening:0})).id;
  const before=await snapshot();
  const request={propertyId,expectedVersion:before.property.version,operation:"payment.request_transfer",input:{guestAccountId:booking.guestAccountId,shiftId:shift,amount:10700,reference:"PMS-CANCEL-001"}};
  const pending=await pmsCommand(db,owner,request,key());
  const cancel={propertyId,expectedVersion:before.property.version,operation:"payment.cancel_transfer",input:{id:pending.id,expectedTransferVersion:1}};
  const cancelKey=key(),result=await pmsCommand(db,owner,cancel,cancelKey);
  expect(result).toMatchObject({state:"cancelled",transferVersion:2});
  expect(await pmsCommand(db,owner,cancel,cancelKey)).toEqual(result);
  expect((await db.prepare("SELECT state FROM pms_transfer_requests WHERE id=?").bind(pending.id).first<{state:string}>())?.state).toBe("cancelled");
  expect((await snapshot()).property.version).toBe(before.property.version);
  expect(await balance(accounts.bank)).toBe(0);
  expect(records(await snapshot(),"entry")).toHaveLength(records(before,"entry").length);
  const retry=await pmsCommand(db,owner,request,key());
  expect(retry).toMatchObject({state:"pending"});
  expect(retry.id).not.toBe(pending.id);
});

it("completes deposit, stay, durable night audit, checkout and invoice without duplicate revenue", async () => {
  const b = await book();
  await checkIn(b.stayId);
  const shift = (await cmd("shift.open", { name: "AM", opening: 0 })).id;
  const s = await snapshot(),
    request = {
      operation: "payment.receive",
      propertyId,
      expectedVersion: s.property.version,
      input: {
        guestAccountId: b.guestAccountId,
        amount: 10700,
        method: "cash",
        shiftId: shift,
      },
    };
  const id = key();
  const first = await pmsCommand(db, owner, request, id);
  expect(await pmsCommand(db, owner, request, id)).toEqual(first);
  expect(await balance(accounts.advance)).toBe(-10700);
  expect(await balance(accounts.revenue)).toBe(0);
  await cmd("shift.close", { id: shift, counted: 10700, handover: "Verified" });
  await cmd("night.start");
  const seedNight=records(await snapshot(),'night')[0].id;
  await drainPms(db,propertyId,'some-other-job');
  expect((await snapshot()).property.businessDate).toBe('2026-09-01');
  await drainPms(db,propertyId,seedNight);
  await drainPms(db);
  expect((await snapshot()).property.businessDate).toBe("2026-09-02");
  expect(await balance(accounts.revenue)).toBe(-10000);
  expect(await balance(accounts.tax)).toBe(-700);
  expect(await balance(accounts.prepaidTax)).toBe(0);
  expect(await balance(accounts.advance)).toBe(0);
  expect(await balance(accounts.receivable)).toBe(0);
  await cmd("stay.check_out", { id: b.stayId });
  const doc = await cmd("guest_account.close", { id: b.guestAccountId });
  expect(await balance(accounts.revenue)).toBe(-10000);
  const invoice = await db
    .prepare(
      "SELECT total_minor,outstanding_minor,status FROM finance_invoices WHERE id=?",
    )
    .bind(doc.id)
    .first();
  expect(invoice).toMatchObject({
    total_minor: 10700,
    outstanding_minor: 0,
    status: "paid",
  });
  expect(record(await snapshot(), "room", roomId).data.cleanliness).toBe(
    "dirty",
  );
  expect(
    records(await snapshot(), "task").some((t) => t.data.type === "departure"),
  ).toBe(true);
  const departureTask=records(await snapshot(),'task').find(t=>t.data.type==='departure')!;
  await cmd('task.complete',{id:departureTask.id,checked:departureTask.data.checklist});
  await cmd('task.inspect',{id:departureTask.id},await f.member('departure-inspector','pms_supervisor'));
  expect(record(await snapshot(),'room',roomId).data.cleanliness).toBe('inspected');
  const totals = await db
    .prepare(
      "SELECT SUM(l.debit_minor) debit,SUM(l.credit_minor) credit FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=?",
    )
    .bind(f.company)
    .first<{ debit: number; credit: number }>();
  expect(totals!.debit).toBe(totals!.credit);
});
it("holds Night Audit for restaurant cash, variance and unverified transfer controls",async()=>{
  const warehouse=key(),register=key(),shift=key(),sale=key(),payment=key();
  const date=(await snapshot()).property.businessDate;
  await db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES(?,?,?,?,?)")
    .bind(warehouse,f.company,f.branch,`HOTEL-${warehouse.slice(0,6)}`,'Hotel restaurant').run();
  await db.prepare("INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode) VALUES(?,?,?,?,?,'restaurant')")
    .bind(register,f.company,f.branch,warehouse,'Night Audit restaurant').run();
  await db.prepare("UPDATE pos_registers SET property_id=? WHERE id=?").bind(propertyId,register).run();
  await db.prepare("INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor) VALUES(?,?,?,?,?,0,0)")
    .bind(shift,register,f.company,f.branch,owner.userId).run();
  await expect(cmd('night.start')).rejects.toMatchObject({code:'PMS_DAY_RECONCILIATION_BLOCKED'});
  expect((await pmsWorkspace(owner,propertyId,true)).dayControls?.openShifts).toBe(1);
  await db.prepare("UPDATE pos_shifts SET state='closed',counted_minor=100,expected_minor=0,variance_minor=100,closed_at=1 WHERE id=?").bind(shift).run();
  await expect(cmd('night.start')).rejects.toMatchObject({code:'PMS_DAY_RECONCILIATION_BLOCKED'});
  await db.prepare("UPDATE pos_shifts SET state='reviewed',reviewed_by='independent-manager',review_reason='Counted and accepted' WHERE id=?").bind(shift).run();
  await db.prepare(`INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,state,created_by,created_at,updated_at,accounts_json)
    VALUES(?,?,?,?,?,'PILOT-PENDING','THB',?,'payment_pending',?,1,1,'{}')`).bind(sale,f.company,f.branch,register,shift,date,owner.userId).run();
  await db.prepare("INSERT INTO pos_payments(id,sale_id,company_id,method,amount_minor,tendered_minor,reference,state,created_by,created_at,updated_at) VALUES(?,?,?,'transfer',100,100,'PENDING-BANK','pending',?,1,1)")
    .bind(payment,sale,f.company,owner.userId).run();
  await expect(cmd('night.start')).rejects.toMatchObject({code:'PMS_DAY_RECONCILIATION_BLOCKED'});
  expect((await pmsWorkspace(owner,propertyId,true)).dayControls).toMatchObject({openBills:1,pendingPayments:1});
  await db.prepare("UPDATE pos_payments SET state='cancelled' WHERE id=?").bind(payment).run();
  await db.prepare("UPDATE pos_sales SET state='cancelled' WHERE id=?").bind(sale).run();
  expect((await pmsWorkspace(owner,propertyId,true)).dayControls?.blockers).toEqual([]);
  await expect(cmd('night.start')).resolves.toMatchObject({propertyId});
});
it("serializes competing bookings for the last room and rolls back the losing command", async () => {
  const s = await snapshot(),
    input = {
      guestId,
      source: "direct",
      rooms: [
        {
          typeId,
          rateId,
          arrival: "2026-09-01",
          departure: "2026-09-03",
          adults: 2,
          quantity: 1,
        },
      ],
    };
  const request = {
    operation: "reservation.create",
    propertyId,
    expectedVersion: s.property.version,
    input,
  };
  const outcomes = await Promise.allSettled([
    pmsCommand(db, owner, request, key()),
    pmsCommand(db, owner, request, key()),
  ]);
  expect(outcomes.filter((r) => r.status === "fulfilled")).toHaveLength(1);
  expect(records(await snapshot(), "stay")).toHaveLength(1);
});
it("rejects double booking, duplicate OTA references and stale versions", async () => {
  await book("2026-09-01", "2026-09-02", {
    source: "Agoda",
    externalRef: "OTA-123",
  });
  await expect(book()).rejects.toMatchObject({ code: "PMS_SOLD_OUT" });
  await expect(
    book("2026-09-02", "2026-09-03", {
      source: "Agoda",
      externalRef: "OTA-123",
    }),
  ).rejects.toMatchObject({ code: "PMS_EXTERNAL_REF_EXISTS" });
  await expect(
    pmsCommand(
      db,
      owner,
      {
        operation: "guest.save",
        propertyId,
        expectedVersion: 1,
        input: { name: "stale" },
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "PMS_VERSION_CONFLICT" });
});
it("creates reservations without archived capacity while retaining cancelled channel references and original history", async () => {
  const original=await book('2026-09-01','2026-09-02',{source:'Agoda',externalRef:'ARCHIVED-OTA-123'});
  await cmd('reservation.cancel',{id:original.reservationId,reason:'Fictional guest cancelled before arrival'});
  const cancelled=record(await snapshot(),'stay',original.stayId);
  const historicalIds=Array.from({length:60},(_,n)=>`archived-stay-${propertyId}-${n}`);
  await db.batch(historicalIds.map(id=>db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'stay',?,1,1)")
    .bind(id,propertyId,JSON.stringify({...cancelled.data,state:'checked_out',arrival:'2024-01-01',departure:'2024-01-02'}))));
  await expect(book('2026-09-02','2026-09-03',{source:' Agoda ',externalRef:' ARCHIVED-OTA-123 '})).rejects.toMatchObject({code:'PMS_EXTERNAL_REF_EXISTS'});
  const booked=await book('2026-09-02','2026-09-03',{source:'direct',externalRef:'NEW-123'});
  expect(record(await snapshot(),'stay',booked.stayId).data.state).toBe('reserved');
  expect(await db.prepare("SELECT count(*)::int n FROM pms_records WHERE property_id=? AND id LIKE 'archived-stay-%' AND version=1 AND updated_at=1").bind(propertyId).first('n')).toBe(60);
  expect(record(await snapshot(),'reservation',original.reservationId).data.state).toBe('cancelled');
});
it("requires inspected rooms and independent inspection; maintenance cannot displace guests", async () => {
  const b = await book();
  await cmd("stay.assign", { id: b.stayId, roomId });
  await expect(cmd("stay.check_in", { id: b.stayId })).rejects.toMatchObject({
    code: "PMS_ROOM_NOT_READY",
  });
  const t = (
    await cmd("task.create", { roomId, type: "arrival", date: "2026-09-01" })
  ).id;
  await cmd("task.complete", {
    id: t,
    checked: record(await snapshot(), "task", t).data.checklist,
  });
  await expect(cmd("task.inspect", { id: t })).rejects.toMatchObject({
    code: "PMS_INDEPENDENT_INSPECTOR",
  });
  await expect(
    cmd("maintenance.open", {
      roomId,
      from: "2026-09-01",
      to: "2026-09-02",
      reason: "AC",
    }),
  ).rejects.toMatchObject({ code: "PMS_ROOM_IN_USE" });
});
it("keeps group pickup from consuming inventory twice and allows back-to-back stays", async () => {
  const block = (
    await cmd("block.create", {
      name: "Conference",
      typeId,
      from: "2026-09-03",
      to: "2026-09-05",
      releaseDate: "2026-09-02",
      quantity: 1,
    })
  ).id;
  expect(availability(await snapshot(), typeId, "2026-09-03").available).toBe(
    0,
  );
  await book("2026-09-03", "2026-09-05", { blockId: block });
  expect(availability(await snapshot(), typeId, "2026-09-03").available).toBe(
    0,
  );
  await book("2026-09-05", "2026-09-06");
  expect(records(await snapshot(), "stay")).toHaveLength(2);
});
it("separates security deposits, enforces refund independence and reverses advance VAT once", async () => {
  const b = await book(),
    sh = (await cmd("shift.open", { name: "Desk", opening: 0 })).id;
  await cmd("payment.receive", {
    guestAccountId: b.guestAccountId,
    amount: 10700,
    method: "cash",
    shiftId: sh,
  });
  await cmd("payment.deposit", {
    guestAccountId: b.guestAccountId,
    amount: 5000,
    method: "cash",
    shiftId: sh,
  });
  expect(await balance(accounts.tax)).toBe(-700);
  const refund = (
    await cmd("refund.request", {
      guestAccountId: b.guestAccountId,
      amount: 10700,
      pocket: "advance",
      reason: "Cancelled",
    })
  ).id;
  await expect(cmd("refund.approve", { id: refund })).rejects.toMatchObject({
    code: "PMS_INDEPENDENT_APPROVER",
  });
  await cmd("refund.approve", { id: refund }, reviewer);
  expect((await pmsWork(owner)).find(item=>item.id===refund)).toMatchObject({queue:"todo",canAct:true,nextAction:"Pay approved refund",responsibleQueue:"Cashier"});
  await projectCommittedEvent("pms.refund.approve");
  expect(await db.prepare("SELECT message FROM staff_notifications WHERE company_id=? AND user_id=? AND record_type='post'").bind(f.company,owner.userId).first("message")).toContain("Refund approved");
  await cmd("refund.pay", { id: refund, method: "cash", shiftId: sh });
  expect((await pmsWork(owner)).some(item=>item.id===refund)).toBe(false);
  const payoutEvent=await projectCommittedEvent("pms.refund.pay");
  await projectPmsSocialEvent(db,payoutEvent);
  expect((await homePosts(db,owner)).find(post=>post.linkedSources.some(source=>source.id===refund))?.body).toContain("Refund paid");
  expect(await balance(accounts.advance)).toBe(0);
  expect(await balance(accounts.tax)).toBe(0);
  expect(await balance(accounts.deposit)).toBe(-5000);
  await expect(
    cmd("refund.pay", { id: refund, method: "cash", shiftId: sh }),
  ).rejects.toMatchObject({ code: "PMS_REFUND_NOT_APPROVED" });
});
it("blocks unauthorized, cross-company and revoked actors without business writes", async () => {
  const staff = await f.member("cleaner", "pms_housekeeping");
  await expect(cmd("reservation.create", {}, staff)).rejects.toMatchObject({
    code: "PERMISSION_REQUIRED",
  });
  await expect(
    loadPms(db, { ...owner, companyId: "foreign" }, propertyId),
  ).rejects.toMatchObject({ code: "PMS_NOT_FOUND" });
  await db
    .prepare("UPDATE assignments SET revoked_at=? WHERE id=?")
    .bind(Date.now(), owner.assignmentId)
    .run();
  await expect(cmd("guest.save", { name: "No access" })).rejects.toBeTruthy();
  expect(records(await snapshot(), "guest")).toHaveLength(1);
});
it("blocks closed accounting periods and keeps the payment and guest account atomic", async () => {
  const b = await book(),
    sh = (await cmd("shift.open", { name: "Desk", opening: 0 })).id;
  await db
    .prepare("UPDATE finance_periods SET state='closed' WHERE id=?")
    .bind(f.period)
    .run();
  await expect(
    cmd("payment.receive", {
      guestAccountId: b.guestAccountId,
      amount: 10700,
      method: "cash",
      shiftId: sh,
    }),
  ).rejects.toMatchObject({ code: "PMS_PERIOD_CLOSED" });
  expect(record(await snapshot(), "guest_account", b.guestAccountId).data.advance).toBe(0);
  expect(records(await snapshot(), "entry")).toHaveLength(0);
  await db.prepare("UPDATE finance_periods SET state='open' WHERE id=?").bind(f.period).run();
  const initial=await snapshot(),commandId=key(),request={propertyId,expectedVersion:initial.property.version,operation:"payment.receive",input:{guestAccountId:b.guestAccountId,amount:10700,method:"cash",shiftId:sh}};
  const retained=async()=>({
    property:await db.prepare("SELECT version,config_json FROM pms_properties WHERE id=?").bind(propertyId).first(),
    records:(await db.prepare("SELECT id,kind,data_json,version FROM pms_records WHERE property_id=? ORDER BY id").bind(propertyId).all()).results,
    journals:(await db.prepare("SELECT id,status FROM finance_journals WHERE company_id=? ORDER BY id").bind(f.company).all()).results,
    payments:(await db.prepare("SELECT id,status,amount_minor FROM finance_payments WHERE company_id=? ORDER BY id").bind(f.company).all()).results,
    documents:(await db.prepare("SELECT id,finance_id,snapshot_json FROM pms_documents WHERE property_id=? ORDER BY id").bind(propertyId).all()).results,
    money:(await db.prepare("SELECT source_type,source_id,amount_kind,amount_units::text,usd_units::text FROM money_transaction_snapshots WHERE company_id=? ORDER BY source_type,source_id,amount_kind").bind(f.company).all()).results,
  });
  const before=await retained(),fault=`pms_payment_late_${key().replaceAll("-","")}`;
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'PMS_PAYMENT_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.command_id='${commandId}') EXECUTE FUNCTION ${fault}();`);
  try {
    await expect(pmsCommand(db,owner,request,commandId)).rejects.toMatchObject({code:"PMS_CONFLICT"});
    expect(await retained()).toEqual(before);
    for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT command_id FROM ${table} WHERE command_id=?`).bind(commandId).first()).toBeNull();
  } finally {await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
  const result=await pmsCommand(db,owner,request,commandId),settled=await retained();
  expect(await pmsCommand(db,owner,request,commandId)).toEqual(result);
  expect(await retained()).toEqual(settled);
  expect(records(await snapshot(),"entry")).toHaveLength(1);
  for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT COUNT(*)::int n FROM ${table} WHERE command_id=?`).bind(commandId).first("n")).toBe(1);
});
it("durably blocks failed night audit and resumes under a valid auditor", async () => {
  const b = await book();
  await checkIn(b.stayId);
  await cmd("night.start");
  await db
    .prepare("UPDATE finance_periods SET state='closed' WHERE id=?")
    .bind(f.period)
    .run();
  await drainPms(db);
  let s = await snapshot();
  expect(s.property.state).toBe("closing");
  expect(records(s, "night")[0].data.state).toBe("blocked");
  expect(s.property.businessDate).toBe("2026-09-01");
  await db
    .prepare("UPDATE finance_periods SET state='open' WHERE id=?")
    .bind(f.period)
    .run();
  await cmd("night.resume", { id: records(s, "night")[0].id });
  await drainPms(db);
  s = await snapshot();
  expect(s.property.state).toBe("open");
  expect(await balance(accounts.revenue)).toBe(-10000);
});
it("protects PMS invoices and payments from generic finance mutations", async () => {
  const b = await book(),
    sh = (await cmd("shift.open", { name: "Desk", opening: 0 })).id;
  await cmd("payment.receive", {
    guestAccountId: b.guestAccountId,
    amount: 10700,
    method: "cash",
    shiftId: sh,
  });
  const payment = await db
    .prepare(
      "SELECT finance_id FROM pms_documents WHERE property_id=? AND kind='receipt'",
    )
    .bind(propertyId)
    .first<{ finance_id: string }>();
  await expect(
    db
      .prepare("UPDATE finance_payments SET amount_minor=1 WHERE id=?")
      .bind(payment!.finance_id)
      .run(),
  ).rejects.toThrow("PMS_OWNER_WORKFLOW_REQUIRED");
  const journal = await db
    .prepare("SELECT journal_id FROM pms_postings WHERE property_id=? LIMIT 1")
    .bind(propertyId)
    .first<{ journal_id: string }>();
  await expect(
    db
      .prepare("UPDATE finance_journals SET status='reversed' WHERE id=?")
      .bind(journal!.journal_id)
      .run(),
  ).rejects.toThrow(/PMS_OWNER_WORKFLOW_REQUIRED|POSTED_JOURNAL_LINES_IMMUTABLE/);
});

it("aggregates accessible hotels without choosing one and keeps guest amounts private from housekeeping",async()=>{
  const booking=await book();
  await cmd("charge.add",{guestAccountId:booking.guestAccountId,description:"Private guest service",price:10000});
  const home=await pmsHome(owner);
  expect(home.hotels.map(hotel=>hotel.id)).toContain(propertyId);
  expect(home.tasks.items.some(item=>item.id===booking.stayId)).toBe(true);
  expect(home.hotels[0].receivable).toBe(10700);
  const sources=await pmsHotelSources(owner,propertyId,'occupancy',home.hotels[0].businessDate);
  expect(sources.rows.some(row=>row.id===roomId&&row.kind==='room')).toBe(true);
  expect(JSON.stringify(sources.rows)).not.toContain('Private guest service');
  await expect(pmsHotelSources({...owner,companyId:'foreign'},propertyId,'occupancy',home.hotels[0].businessDate)).rejects.toMatchObject({status:404});
  await expect(pmsHotelSources(owner,propertyId,'occupancy','2020-01-01')).rejects.toMatchObject({code:'PMS_BUSINESS_DATE_CHANGED'});

  const housekeeper=await f.member("housekeeper","pms_housekeeping");
  housekeeper.branchId=f.branch;housekeeper.assignmentScope="branch";
  await db.prepare("UPDATE assignments SET scope_kind='branch',scope_node_id=? WHERE id=?").bind(f.branch,housekeeper.assignmentId).run();
  const privateHome=await pmsHome(housekeeper);
  expect(privateHome.hotels[0].receivable).toBeNull();
  await expect(pmsHotelSources(housekeeper,propertyId,'room_revenue',home.hotels[0].businessDate)).rejects.toMatchObject({status:403});
  expect(privateHome.tasks.items.every(item=>!["arrivals","departures"].includes(item.label))).toBe(true);
  expect(JSON.stringify(privateHome)).not.toContain("Private guest service");
  await expect(readablePmsSource(db,housekeeper,booking.stayId)).rejects.toMatchObject({status:404});
  const wrongBranch=key();
  await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Other branch',?,?,?)").bind(wrongBranch,wrongBranch,f.company,f.company,wrongBranch).run();
  const otherHotel=await pmsCommand(db,owner,{operation:"property.create",input:{name:"Other hotel",branchId:wrongBranch,businessDate:"2026-09-02"}},key());
  expect((await pmsHome(owner)).hotels).toHaveLength(2);
  expect((await pmsHome(housekeeper)).hotels.map(hotel=>hotel.id)).not.toContain(otherHotel.id);
});

it("links one PMS handover in Social and Work and withdraws its body and acknowledgement with source access",async()=>{
  const taskId=(await cmd("task.create",{roomId,type:"arrival",date:"2026-09-01"})).id;
  const housekeeper=await f.member("shift receiver","pms_housekeeping"),outsider=await f.member("cashier only","pms_cashier");
  const source=await readablePmsSource(db,owner,taskId);
  const snapshot={items:await pmsWork(owner),unavailableSources:[]};
  const view=await handoverView(db,owner,snapshot);
  expect(view.items.find(item=>item.id===taskId)?.sourceKind).toBe("pms_record");
  expect((await handoverView(db,owner,{...snapshot,unavailableSources:["Tasks"]})).unavailable).toBe(false);
  const input={date:new Date(Date.now()+7*3600000).toISOString().slice(0,10),body:"Please prepare the selected room for the next shift.",sources:[{kind:"pms_record",id:taskId,sourceVersion:source.version}],recipientIds:[housekeeper.userId]};
  await expect(publishHandover(db,owner,snapshot,{...input,recipientIds:[outsider.userId]},key())).rejects.toMatchObject({code:"HANDOVER_RECIPIENT_ACCESS"});
  const publishKey=key(),published=await publishHandover(db,owner,snapshot,input,publishKey);
  expect(await publishHandover(db,owner,snapshot,input,publishKey)).toEqual(published);
  expect((await homePosts(db,housekeeper)).map(post=>post.id)).toContain(published.id);
  expect((await postView(db,housekeeper,published.id)).linkedSources[0]).toMatchObject({kind:"pms_record",id:taskId,available:true});
  expect((await acknowledgeHandover(db,housekeeper,published.id,key())).acknowledged).toBe(true);
  const cashierRole={...housekeeper,role:"pms_cashier",assignmentId:await f.grant(housekeeper,"pms_cashier")};
  expect((await homePosts(db,cashierRole)).map(post=>post.id)).not.toContain(published.id);
  await expect(acknowledgeHandover(db,cashierRole,published.id,key())).rejects.toMatchObject({status:404});
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(),housekeeper.assignmentId).run();
  await expect(readablePmsSource(db,housekeeper,taskId)).rejects.toMatchObject({status:404});
  expect((await pmsTaskPage(db,housekeeper)).items).toEqual([]);
  expect((await homePosts(db,housekeeper)).map(post=>post.id)).not.toContain(published.id);
  expect((await pmsHome(housekeeper,{loadedIds:[published.id]})).readablePostIds).toEqual([]);
  await expect(acknowledgeHandover(db,housekeeper,published.id,key())).rejects.toMatchObject({status:404});
});

it("rejects a handover from a stale PMS source and bounds task pages with cursors",async()=>{
  const receiver=await f.member("room receiver","pms_housekeeping");
  const source=await readablePmsSource(db,owner,roomId);
  await cmd("room.save",{id:roomId,number:"101",typeId,building:"A",floor:"1"});
  await expect(publishHandover(db,owner,{items:[],unavailableSources:[]},{date:new Date(Date.now()+7*3600000).toISOString().slice(0,10),body:"Prepare room",sources:[{kind:"pms_record",id:roomId,sourceVersion:source.version}],recipientIds:[receiver.userId]},key())).rejects.toMatchObject({code:"HANDOVER_SOURCE_CHANGED"});
  for(let i=0;i<14;i++)await cmd("room.save",{number:`H-${i}`,typeId});
  const page=await pmsTaskPage(db,owner);
  expect(page.items).toHaveLength(12);expect(page.nextCursor).toBeTruthy();
  const next=await pmsTaskPage(db,owner,{cursor:page.nextCursor!});
  expect(next.items.some(item=>page.items.some(previous=>previous.id===item.id))).toBe(false);
  await expect(pmsTaskPage(db,receiver,{cursor:page.nextCursor!})).rejects.toMatchObject({status:422});
});

it("projects durable PMS events once and combines room updates in the same Social post",async()=>{
  await clean();
  const events=(await db.prepare("SELECT * FROM outbox_events WHERE event_type IN ('pms.task.complete','pms.task.inspect') AND payload_json::jsonb->>'companyId'=? ORDER BY created_at,id").bind(f.company).all<{id:string;event_type:string;aggregate_id:string;aggregate_version:number;payload_json:string}>()).results;
  const messages=events.map(event=>({id:event.id,holdingId:"h001",eventType:event.event_type,aggregateId:event.aggregate_id,aggregateVersion:event.aggregate_version,payload:JSON.parse(event.payload_json)} satisfies EventMessage));
  await Promise.all(messages.map(message=>projectPmsSocialEvent(db,message)));
  await Promise.all(messages.map(message=>projectPmsSocialEvent(db,message)));
  expect(await db.prepare("SELECT count(*) FROM pms_social_event_receipts WHERE event_id IN (?,?)").bind(...events.map(event=>event.id)).first<number>("count")).toBe(2);
  const posts=await homePosts(db,owner);
  expect(posts.filter(post=>post.author_name==="PMS")).toHaveLength(1);
  expect(posts[0].linkedSources[0].available).toBe(true);
});

async function cutoverDefinition(opening={due:0,advance:0,advanceTax:0,deposit:0}){
  return {propertyId,expectedVersion:(await snapshot()).property.version,cutoverDate:"2026-09-01",currency:"THB",source:"legacy-test",
    roomTypes:[{key:"type",existingId:typeId,name:"Deluxe",capacity:2}],rooms:[{key:"room",existingId:roomId,number:"101",typeKey:"type"}],
    rates:[{key:"rate",existingId:rateId,name:"Standard",typeKey:"type",from:"2026-09-01",to:"2027-09-01",priceMinor:10000}],
    guests:[{key:"guest",existingId:guestId,name:"Test Guest"}],reservations:[{key:"legacy-1",guestKey:"guest",typeKey:"type",rateKey:"rate",roomKey:"room",arrival:"2026-08-31",departure:"2026-09-02",adults:2,state:"in_house",opening}]};
}
it("previews import errors without writing hotel records and requires independent unchanged approval",async()=>{
  const definition=await cutoverDefinition(),count=(await snapshot()).records.length;
  const preview=await previewPmsImport(db,owner,definition,key()) as {id:string;version:number;report:{canApprove:boolean}};
  expect(preview.report.canApprove).toBe(true);expect((await snapshot()).records).toHaveLength(count);
  await expect(approvePmsImport(db,owner,{id:preview.id,expectedVersion:1},key())).rejects.toMatchObject({code:"PMS_IMPORT_INDEPENDENT"});
  const invalid=await previewPmsImport(db,owner,{...definition,reservations:[{...definition.reservations[0],guestKey:"missing"}]},key()) as {report:{canApprove:boolean;errors:unknown[]}};
  expect(invalid.report.canApprove).toBe(false);expect(invalid.report.errors).toHaveLength(1);expect((await snapshot()).records).toHaveLength(count);
  await cmd("guest.save",{name:"Another guest"});
  await expect(approvePmsImport(db,reviewer,{id:preview.id,expectedVersion:1},key())).rejects.toMatchObject({code:"PMS_IMPORT_STALE"});
});

it("applies approved imported in-house stays atomically and resumes a repeated apply without past room revenue",async()=>{
  const preview=await previewPmsImport(db,owner,await cutoverDefinition(),key()) as {id:string};
  await approvePmsImport(db,reviewer,{id:preview.id,expectedVersion:1},key());
  const applyKey=key(),input={id:preview.id,expectedVersion:2};
  const applied=await applyPmsImport(db,owner,input,applyKey);
  expect(await applyPmsImport(db,owner,input,applyKey)).toEqual(applied);
  const imported=records(await snapshot(),"stay")[0];
  expect(imported.data.state).toBe("in_house");expect(imported.data.arrival).toBe("2026-08-31");
  await cmd("night.start");await drainPms(db);
  expect(records(await snapshot(),"entry").filter(entry=>entry.data.sourceKey.startsWith("night:")).map(entry=>entry.data.description)).toEqual(["Room charge 2026-09-01"]);
  expect(await balance(accounts.revenue)).toBe(-10000);
});

it("traces imported advance and deposit to the approved opening journal and reconciles partial refunds",async()=>{
  const input=await cutoverDefinition({due:0,advance:20000,advanceTax:0,deposit:5000});
  const preview=await previewPmsImport(db,owner,input,key()) as {id:string};
  await expect(approvePmsImport(db,reviewer,{id:preview.id,expectedVersion:1},key())).rejects.toMatchObject({code:"PMS_IMPORT_JOURNAL"});
  const journal=await createJournal(owner,{chartId:f.chart,periodId:f.period,entryDate:"2026-09-01",currency:"THB",memo:"Hotel cutover balances",isOpening:true,sourceType:"opening_import",lines:[
    {accountId:accounts.cash,debit:"250.00",credit:"0.00"},{accountId:accounts.advance,debit:"0.00",credit:"200.00"},{accountId:accounts.deposit,debit:"0.00",credit:"50.00"}]},key());
  const flow=await resolveFlow(db,owner,"journal",journal.id);
  await assignFlowPrimary(db,owner,"journal",journal.id,{stepKey:"draft",userId:reviewer.userId,reason:"Assign independent hotel cutover review",saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
  await postJournal(reviewer,journal.id,key(),{expectedVersion:journal.version});
  await approvePmsImport(db,reviewer,{id:preview.id,expectedVersion:1,openingJournalId:journal.id},key());
  await applyPmsImport(db,owner,{id:preview.id,expectedVersion:2},key());
  const home=await pmsFinanceOverview(owner);
  expect(home.hotels[0].differences).toEqual({receivable:0,advance:0,deposit:0});expect(home.exceptions).toEqual([]);
  const guestAccount=records(await snapshot(),"guest_account")[0],shift=(await cmd("shift.open",{name:"New cashier",opening:10000})).id;
  const refund=(await cmd("refund.request",{guestAccountId:guestAccount.id,pocket:"deposit",amount:2000,reason:"Partial cutover deposit return"})).id;
  await cmd("refund.approve",{id:refund},reviewer);await cmd("refund.pay",{id:refund,method:"cash",shiftId:shift});
  expect((await pmsFinanceOverview(owner)).hotels[0].differences).toEqual({receivable:0,advance:0,deposit:0});
  await cmd("shift.close",{id:shift,counted:8000,handover:"Deposit refund recorded"});
  await cmd("night.start");await drainPms(db);
  expect(record(await snapshot(),"guest_account",guestAccount.id).data.advance).toBe(9300);
});

it("settles the original opening receivable and invoices only new hotel service across two business days",async()=>{
  const partyId=key(),equityId=key();
  await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,'Opening equity','equity','THB',1,1)").bind(equityId,f.chart,f.company,equityId).run();
  await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,created_at,updated_at) VALUES(?,?,'customer',?,'Imported guest','THB','active',1,1)").bind(partyId,f.company,partyId).run();
  await db.prepare("UPDATE finance_accounts SET is_control=1,control_kind='receivable' WHERE id=?").bind(accounts.receivable).run();
  const definition=openingTrialBalanceSchema.parse({chartId:f.chart,periodId:f.period,entryDate:"2026-09-01",currency:"THB",
    lines:[{accountId:accounts.receivable,debit:"200.00",credit:"0",subledgerKind:"receivable"},{accountId:equityId,debit:"0",credit:"200.00",subledgerKind:"none"}],
    subledgerItems:[{kind:"receivable",partyId,reference:"LEGACY-HOTEL-1",dueDate:"2026-09-02",controlAccountId:accounts.receivable,offsetAccountId:equityId,amount:"200.00"}]});
  const financePreview=await previewOpeningTrialBalance(owner,definition);
  const journal=await importOpeningTrialBalance(owner,{...definition,previewToken:financePreview.previewToken},key());
  const flow=await resolveFlow(db,owner,"journal",journal.id);
  await assignFlowPrimary(db,owner,"journal",journal.id,{stepKey:"draft",userId:reviewer.userId,reason:"Assign independent opening balance posting",saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
  await postJournal(reviewer,journal.id,key(),{expectedVersion:journal.version});
  const originalInvoice=`${journal.id}-subledger-0`,cutover=await cutoverDefinition({due:20000,advance:0,advanceTax:0,deposit:0});
  const importDefinition={...cutover,guests:cutover.guests.map(guest=>({...guest,partyId})),reservations:cutover.reservations.map(reservation=>({...reservation,openingInvoiceId:originalInvoice}))};
  const preview=await previewPmsImport(db,owner,importDefinition,key()) as {id:string};
  await approvePmsImport(db,reviewer,{id:preview.id,expectedVersion:1,openingJournalId:journal.id},key());
  await applyPmsImport(db,owner,{id:preview.id,expectedVersion:2},key());
  const stay=records(await snapshot(),"stay")[0],guestAccountId=stay.data.guestAccountId,shift=(await cmd("shift.open",{name:"Cutover day",opening:0})).id;
  await cmd("payment.receive",{guestAccountId,amount:10000,method:"cash",shiftId:shift});
  expect(await db.prepare("SELECT outstanding_minor FROM finance_invoices WHERE id=?").bind(originalInvoice).first<number>("outstanding_minor")).toBe(10000);
  expect(await balance(accounts.revenue)).toBe(0);
  await cmd("shift.close",{id:shift,counted:10000,handover:"Partial opening collection"});await cmd("night.start");await drainPms(db);
  const second=(await cmd("shift.open",{name:"Second business day",opening:0})).id;
  await cmd("payment.receive",{guestAccountId,amount:20700,method:"cash",shiftId:second});await cmd("stay.check_out",{id:stay.id});
  const invoice=await cmd("guest_account.close",{id:guestAccountId});
  expect(await db.prepare("SELECT total_minor FROM finance_invoices WHERE id=?").bind(invoice.id).first<number>("total_minor")).toBe(10700);
  expect(await db.prepare("SELECT outstanding_minor FROM finance_invoices WHERE id=?").bind(originalInvoice).first<number>("outstanding_minor")).toBe(0);
  expect(await db.prepare("SELECT sum(amount_minor) amount FROM finance_allocations WHERE company_id=?").bind(f.company).first<number>("amount")).toBe(30700);
  expect((await pmsFinanceOverview(owner)).hotels[0].differences).toEqual({receivable:0,advance:0,deposit:0});
  await cmd("shift.close",{id:second,counted:20700,handover:"Opening and current invoices fully settled"});await cmd("night.start");await drainPms(db);
  expect((await snapshot()).property.businessDate).toBe("2026-09-03");expect(await balance(accounts.revenue)).toBe(-10000);
});
it("calculates inclusive prices with exact integer money", () => {
  expect(
    amounts(11770, {
      ...defaultConfig,
      taxBps: 700,
      serviceBps: 1000,
      taxInclusive: true,
    }),
  ).toEqual({ net: 10000, service: 1000, tax: 770, total: 11770 });
});
it("allocates new receipts after an earlier deposit was refunded", async () => {
  const b = await book(),
    sh = (await cmd("shift.open", { name: "Desk", opening: 0 })).id;
  const first = (
    await cmd("payment.receive", {
      guestAccountId: b.guestAccountId,
      amount: 10700,
      method: "cash",
      shiftId: sh,
    })
  ).id;
  const r = (
    await cmd("refund.request", {
      guestAccountId: b.guestAccountId,
      amount: 10700,
      pocket: "advance",
      reason: "Change payment",
    })
  ).id;
  await cmd("refund.approve", { id: r }, reviewer);
  await cmd("refund.pay", { id: r, method: "cash", shiftId: sh });
  const second = (
    await cmd("payment.receive", {
      guestAccountId: b.guestAccountId,
      amount: 10700,
      method: "cash",
      shiftId: sh,
    })
  ).id;
  await checkIn(b.stayId);
  await cmd("stay.check_out", { id: b.stayId });
  const invoice = await cmd("guest_account.close", { id: b.guestAccountId });
  const allocations = (
    await db
      .prepare(
        "SELECT d.snapshot_json,a.amount_minor FROM finance_allocations a JOIN pms_documents d ON d.id=a.payment_id WHERE a.invoice_id=?",
      )
      .bind(invoice.id)
      .all<{ snapshot_json: string; amount_minor: number }>()
  ).results;
  expect(allocations).toHaveLength(1);
  expect(JSON.parse(allocations[0].snapshot_json).entries[0].id).toBe(second);
  expect(JSON.parse(allocations[0].snapshot_json).entries[0].id).not.toBe(
    first,
  );
  expect(allocations[0].amount_minor).toBe(10700);
});
it("traces a partially refunded damage deposit used to settle a room charge", async () => {
  const b = await book(),
    sh = (await cmd("shift.open", { name: "Desk", opening: 0 })).id;
  await checkIn(b.stayId);
  await cmd("payment.deposit", {
    guestAccountId: b.guestAccountId,
    amount: 12700,
    method: "cash",
    shiftId: sh,
  });
  const r = (
    await cmd("refund.request", {
      guestAccountId: b.guestAccountId,
      amount: 2000,
      pocket: "deposit",
      reason: "Return excess",
    })
  ).id;
  await cmd("refund.approve", { id: r }, reviewer);
  await cmd("refund.pay", { id: r, method: "cash", shiftId: sh });
  await cmd("stay.preview_checkout", { id: b.stayId });
  await cmd("payment.apply_deposit", {
    guestAccountId: b.guestAccountId,
    amount: 10700,
    reason: "Guest approved",
  });
  await cmd("stay.check_out", { id: b.stayId });
  const i = await cmd("guest_account.close", { id: b.guestAccountId });
  expect(await balance(accounts.deposit)).toBe(0);
  expect(await balance(accounts.receivable)).toBe(0);
  expect(
    await db
      .prepare(
        "SELECT SUM(amount_minor) n FROM finance_allocations WHERE invoice_id=?",
      )
      .bind(i.id)
      .first(),
  ).toMatchObject({ n: 10700 });
});
it("splits and merges charges without repeating revenue and reverses the recipient entry once", async () => {
  const b = await book(),
    other = (
      await cmd("guest_account.create", {
        reservationId: b.reservationId,
        name: "Company services",
      })
    ).id;
  const e = (
    await cmd("charge.add", {
      guestAccountId: b.guestAccountId,
      price: 1000,
      description: "Breakfast",
    })
  ).id;
  await cmd("guest_account.transfer", {
    entryId: e,
    toGuestAccountId: other,
    reason: "Company pays",
  });
  expect(await balance(accounts.revenue)).toBe(-1000);
  const transfer = records(await snapshot(), "entry").find(
    (x) => x.data.guestAccountId === other && x.data.kind === "transfer",
  )!;
  await expect(
    cmd("charge.correction.request", { id: e, reason: "Invalid origin" }),
  ).rejects.toMatchObject({ code: "PMS_ALREADY_REVERSED" });
  const correction=await cmd("charge.correction.request", { id: transfer.id, reason: "Service cancelled" });
  await expect(cmd("charge.correction.review",{id:correction.id,expectedVersion:1,action:"approve",reason:"Independent cancellation review"},owner))
    .rejects.toMatchObject({code:"PMS_INDEPENDENT_REVIEW_REQUIRED"});
  await cmd("charge.correction.review",{id:correction.id,expectedVersion:1,action:"approve",reason:"Independent cancellation review"},reviewer);
  expect(await balance(accounts.revenue)).toBe(0);
  await cmd("charge.add", {
    guestAccountId: other,
    price: 2000,
    description: "Dinner",
  });
  await cmd("guest_account.merge", { id: other, toGuestAccountId: b.guestAccountId });
  expect(record(await snapshot(), "guest_account", b.guestAccountId).data.due).toBe(2140);
  expect(await balance(accounts.receivable)).toBe(2140);
});
it("settles a manually entered OTA reservation against approved credit and net bank proceeds", async () => {
  const party = key();
  await db
    .prepare(
      "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'OTA Agent','THB',1,1)",
    )
    .bind(party, f.company, party)
    .run();
  const credit = (
    await cmd("credit.request", { partyId: party, limit: 100000, terms: 30 })
  ).id;
  await cmd("credit.approve", { id: credit }, reviewer);
  const b = await book("2026-09-01", "2026-09-02", {
    source: "Agoda",
    externalRef: "MANUAL-SETTLEMENT",
  });
  await checkIn(b.stayId);
  await cmd("stay.preview_checkout", { id: b.stayId });
  await cmd("guest_account.credit", { id: b.guestAccountId, creditId: credit });
  await cmd("stay.check_out", { id: b.stayId });
  const invoice = await cmd("guest_account.close", { id: b.guestAccountId });
  const sh = (await cmd("shift.open", { name: "Reconciliation", opening: 0 }))
    .id;
  const beforeSettlement = await snapshot();
  const reconciliation = await seedAttachmentSet(db, owner, {
    targetType: "pms_property",
    targetId: propertyId,
    componentKind: "action",
    componentRef: "ota.settle",
    purposeKey: "ota_reconciliation",
    expectedTargetVersion: beforeSettlement.property.version,
  });
  await pmsCommand(
    db,
    owner,
    {
      propertyId,
      expectedVersion: beforeSettlement.property.version,
      operation: "ota.settle",
      input: {
        guestAccountId: b.guestAccountId,
        gross: 10700,
        commission: 1200,
        fees: 100,
        reference: "Statement 001",
        shiftId: sh,
      },
      attachmentSetIds: [reconciliation.setId],
    },
    key(),
  );
  expect(await attachmentBindingState(db, reconciliation.setId)).toMatchObject({
    set_state: "bound",
    link_state: "bound",
    binding_type: "pms_ota_settlement",
  });
  expect(await balance(accounts.bank)).toBe(9400);
  expect(await balance(accounts.commission)).toBe(1300);
  expect(await balance(accounts.receivable)).toBe(0);
  expect(await balance(accounts.revenue)).toBe(-10000);
  expect(
    await db
      .prepare(
        "SELECT outstanding_minor,status FROM finance_invoices WHERE id=?",
      )
      .bind(invoice.id)
      .first(),
  ).toMatchObject({ outstanding_minor: 0, status: "paid" });
});
it("moves an in-house guest, extends safely and preserves room history", async () => {
  const second = (
      await cmd("room.save", { number: "102", typeId, active: true })
    ).id,
    b = await book();
  await checkIn(b.stayId);
  const task = (
    await cmd("task.create", {
      roomId: second,
      type: "arrival",
      date: "2026-09-01",
    })
  ).id;
  await cmd("task.complete", {
    id: task,
    checked: record(await snapshot(), "task", task).data.checklist,
  });
  await cmd(
    "task.inspect",
    { id: task },
    await f.member("second-inspector", "pms_supervisor"),
  );
  await cmd("stay.move", {
    id: b.stayId,
    roomId: second,
    reason: "Guest request",
  });
  expect(record(await snapshot(), "room", roomId).data.cleanliness).toBe(
    "dirty",
  );
  await cmd("stay.change_dates", {
    id: b.stayId,
    arrival: "2026-09-01",
    departure: "2026-09-04",
    rateId,
    reason: "Extend",
  });
  expect(record(await snapshot(), "stay", b.stayId).data.prices).toHaveLength(
    3,
  );
  await expect(
    cmd("maintenance.open", {
      roomId: second,
      from: "2026-09-02",
      to: "2026-09-03",
      reason: "Repair",
    }),
  ).rejects.toMatchObject({ code: "PMS_ROOM_IN_USE" });
});
it("uses daily prices and stop-sell without repricing an existing reservation", async () => {
  await cmd("rate.calendar", {
    id: rateId,
    from: "2026-09-02",
    to: "2026-09-02",
    price: 20000,
    stopSell: false,
    closedArrival: false,
    closedDeparture: false,
  });
  const b = await book("2026-09-01", "2026-09-03");
  expect(
    record(await snapshot(), "stay", b.stayId).data.prices.map((n) => n.total),
  ).toEqual([10700, 21400]);
  const rate = record(await snapshot(), "rate", rateId).data;
  await cmd("rate.save", { ...rate, id: rateId, price: 12000 });
  expect(record(await snapshot(), "rate", rateId).data.calendar).toHaveLength(
    1,
  );
  await cmd("rate.calendar", {
    id: rateId,
    from: "2026-09-04",
    to: "2026-09-04",
    price: 10000,
    stopSell: true,
    closedArrival: false,
    closedDeparture: false,
  });
  await expect(book("2026-09-04", "2026-09-05")).rejects.toMatchObject({
    code: "PMS_RATE_UNAVAILABLE",
  });
});
it("posts minibar and inventory cost together, rolling back insufficient stock", async () => {
  const warehouse = key(),
    product = key();
  await db
    .prepare(
      "INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES(?,?,?,?,?)",
    )
    .bind(warehouse, f.company, f.branch, warehouse, "Hotel stock")
    .run();
  await db
    .prepare(
      "INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES(?,?,?,?,1000)",
    )
    .bind(product, f.company, product, "Water")
    .run();
  await db
    .prepare(
      "INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) VALUES(?,?,2000,600)",
    )
    .bind(warehouse, product)
    .run();
  const item = (
      await cmd("item.save", {
        name: "Water",
        kind: "minibar",
        productId: product,
        warehouseId: warehouse,
        price: 1000,
      })
    ).id,
    b = await book();
  await cmd("minibar.charge", {
    itemId: item,
    quantity: 1,
    guestAccountId: b.guestAccountId,
    roomId,
  });
  expect(await balance(accounts.inventory)).toBe(-300);
  expect(await balance(accounts.consumption)).toBe(300);
  expect(await balance(accounts.revenue)).toBe(-1000);
  await expect(
    cmd("minibar.charge", { itemId: item, quantity: 2, guestAccountId: b.guestAccountId }),
  ).rejects.toMatchObject({ code: "PMS_STOCK_SHORT" });
  expect(await balance(accounts.revenue)).toBe(-1000);
  expect(
    await db
      .prepare(
        "SELECT quantity_milli,value_minor FROM inventory_ledger WHERE source_type='pms' AND product_id=?",
      )
      .bind(product)
      .first(),
  ).toMatchObject({ quantity_milli: -1000, value_minor: -300 });
});
it("locks closing-day commands and blocks workers whose originating authority was revoked", async () => {
  const b = await book();
  await checkIn(b.stayId);
  await cmd("night.start");
  await expect(cmd("guest.save", { name: "Frozen" })).rejects.toMatchObject({
    code: "PMS_DAY_CLOSING",
  });
  await db
    .prepare("UPDATE assignments SET revoked_at=? WHERE id=?")
    .bind(Date.now(), owner.assignmentId)
    .run();
  await drainPms(db);
  expect(records(await snapshot(), "night")[0].data.state).toBe("blocked");
  expect(await balance(accounts.revenue)).toBe(0);
});
it("cancels an unarrived room in a partially checked-in group and ignores stale housekeeping cycles", async () => {
  await cmd("room.save", { number: "102", typeId, active: true });
  const group = await cmd("reservation.create", {
    guestId,
    source: "phone",
    rooms: [
      {
        typeId,
        rateId,
        arrival: "2026-09-01",
        departure: "2026-09-02",
        adults: 2,
        quantity: 2,
      },
    ],
  });
  const stays = records(await snapshot(), "stay").filter(
    (s) => s.data.reservationId === group.id,
  );
  await checkIn(stays[0].id);
  await cmd("stay.no_show", { id: stays[1].id, reason: "Group member absent" });
  const task = (
      await cmd("task.create", { roomId, type: "stayover", date: "2026-09-01" })
    ).id,
    sh = (await cmd("shift.open", { name: "Desk", opening: 0 })).id;
  await cmd("payment.receive", {
    guestAccountId: stays[0].data.guestAccountId,
    amount: 10700,
    method: "cash",
    shiftId: sh,
  });
  await cmd("stay.check_out", { id: stays[0].id });
  expect(record(await snapshot(), "task", task).data.state).toBe("superseded");
  await expect(
    cmd("task.complete", {
      id: task,
      checked: record(await snapshot(), "task", task).data.checklist,
    }),
  ).rejects.toMatchObject({ code: "PMS_CLEANING_STALE" });
});
it("loads real workspace selectors and redacts financial and guest data for housekeeping", async () => {
  await book();
  const full = await pmsWorkspace(owner, propertyId);
  expect(full.branches.some((b) => b.id === f.branch)).toBe(true);
  expect(
    full.accounts.filter((a) => Object.values(accounts).includes(a.id)),
  ).toHaveLength(14);
  const cleaner = await f.member("read-housekeeping", "pms_housekeeping");
  const restricted = await pmsWorkspace(cleaner, propertyId);
  expect(
    restricted.snapshot!.records.some(
      (r) => r.kind === "guest" || r.kind === "entry" || r.kind === "guest_account",
    ),
  ).toBe(false);
  expect(
    Object.values(restricted.snapshot!.property.config.accounts).every(
      (v) => v === "",
    ),
  ).toBe(true);
});
it("defines a POS room-charge contract that rejects foreign scope and already recognized revenue", async () => {
  const b = await book();
  await checkIn(b.stayId);
  const s = await snapshot(),
    input = {
      source: "pos",
      sourceId: "sale",
      sourceLineId: "line",
      companyId: f.company,
      branchId: f.branch,
      propertyId,
      stayId: b.stayId,
      currency: "THB",
      revenueOwner: "pms",
      sourceJournalId: null,
      description: "Cafe",
      net: 1000,
      service: 0,
      tax: 70,
      total: 1070,
    };
  expect(prepareRoomCharge(s, input)).toMatchObject({
    duplicate: false,
    guestAccountId: b.guestAccountId,
    sourceKey: "pos:sale:line",
  });
  expect(() =>
    prepareRoomCharge(s, {
      ...input,
      revenueOwner: "pos",
      sourceJournalId: "journal",
    }),
  ).toThrow();
  expect(() =>
    prepareRoomCharge(s, { ...input, companyId: "another" }),
  ).toThrow();
  s.records.push({
    id: "imported",
    kind: "entry",
    version: 1,
    updatedAt: 1,
    data: {
      guestAccountId: b.guestAccountId,
      stayId: b.stayId,
      date: s.property.businessDate,
      kind: "charge",
      description: "Cafe",
      net: 1000,
      service: 0,
      tax: 70,
      total: 1070,
      method: "",
      reference: "",
      sourceKey: "pos:sale:line",
      originalId: "",
      shiftId: "",
      actorId: owner.userId,
      at: 1,
    },
  });
  expect(prepareRoomCharge(s, input)).toMatchObject({
    duplicate: true,
    entryId: "imported",
  });
  expect(() =>
    prepareRoomCharge(s, { ...input, net: 2000, total: 2070 }),
  ).toThrow();
});
it("retains historical occupancy capacity and posts corrections in the open business date", async () => {
  const b = await book();
  await checkIn(b.stayId);
  await cmd("night.start");
  await drainPms(db);
  await cmd("room.save", { number: "new room", typeId, active: true });
  let s = await snapshot();
  expect(dailyRoomReport(s, "2026-09-01")).toEqual({
    capacity: 1,
    sold: 1,
    roomRevenue: 10000,
  });
  const night = records(s, "entry").find((e) =>
    e.data.sourceKey.startsWith("night:"),
  )!;
  const correction=await cmd("charge.correction.request", { id: night.id, reason: "Incorrect prior-night rate" });
  await cmd("charge.correction.review",{id:correction.id,expectedVersion:1,action:"approve",reason:"Approved correction"},reviewer);
  s = await snapshot();
  expect(dailyRoomReport(s, "2026-09-01").roomRevenue).toBe(10000);
  expect(dailyRoomReport(s, "2026-09-02").roomRevenue).toBe(-10000);
});
it("keeps overdue arrivals outside the current UI window as Night Audit blockers", async () => {
  const b=await book();
  // Explicit legacy date metadata; no historical revenue or GL is fabricated.
  await db.prepare("UPDATE pms_records SET data_json=jsonb_set(jsonb_set(data_json::jsonb,'{arrival}','\"2026-08-01\"'),'{departure}','\"2026-08-02\"')::text WHERE id=?")
    .bind(b.stayId).run();
  await expect(cmd('night.start')).rejects.toMatchObject({code:'PMS_NIGHT_BLOCKED'});
  expect((await snapshot()).property.state).toBe('open');
  expect(await db.prepare("SELECT COUNT(*)::int n FROM pms_jobs WHERE property_id=?").bind(propertyId).first('n')).toBe(0);
});
it("retains an older open shift behind reviewed history before closing a hotel day", async () => {
  const shift=(await cmd('shift.open',{name:'Older shift',opening:0})).id;
  await db.prepare("UPDATE pms_records SET data_json=jsonb_set(data_json::jsonb,'{date}','\"2026-08-01\"')::text WHERE id=?").bind(shift).run();
  const template=record(await snapshot(),'shift',shift).data;
  for(let i=0;i<61;i++) await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'shift',?,1,?)")
    .bind(key(),propertyId,JSON.stringify({...template,name:`Reviewed mock ${i}`,state:'reviewed',reviewedBy:reviewer.userId}),Date.now()+i).run();
  await expect(cmd('night.start')).rejects.toMatchObject({code:'PMS_NIGHT_BLOCKED'});
  expect(record(await snapshot(),'shift',shift).data.state).toBe('open');
});
it("does not treat an older closed shift with unknown variance as reconciled", async () => {
  const shift=(await cmd('shift.open',{name:'Legacy variance unknown',opening:0})).id;
  // Legacy metadata cannot assert a reviewed zero balance when variance is absent.
  await db.prepare("UPDATE pms_records SET data_json=(jsonb_set(jsonb_set(data_json::jsonb,'{state}','\"closed\"'),'{date}','\"2026-08-01\"')-'variance')::text WHERE id=?")
    .bind(shift).run();
  await expect(cmd('night.start')).rejects.toMatchObject({code:'PMS_NIGHT_BLOCKED'});
  expect((await snapshot()).property.state).toBe('open');
});
it("reuses complete next-day stayover tasks and preserves unrelated closed account metadata", async () => {
  const b=await book('2026-09-01','2026-09-03'); await checkIn(b.stayId);
  const task=(await cmd('task.create',{roomId,type:'stayover',date:'2026-09-02'})).id;
  await db.prepare("UPDATE pms_records SET data_json=jsonb_set(data_json::jsonb,'{stayId}',to_jsonb(?::text))::text WHERE id=?").bind(b.stayId,task).run();
  const s=await snapshot(),template=record(s,'task',task).data,account=record(s,'guest_account',b.guestAccountId).data;
  const historyIds:string[]=[];
  for(let i=0;i<61;i++) {
    await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'task',?,1,?)")
      .bind(key(),propertyId,JSON.stringify({...template,stayId:`mock-other-${i}`}),Date.now()+i).run();
    const id=key();historyIds.push(id);
    await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'guest_account',?,1,1)")
      .bind(id,propertyId,JSON.stringify({...account,state:'closed',reservationId:`mock-history-${i}`})).run();
  }
  const history=async()=> (await db.prepare("SELECT id,data_json,version,updated_at FROM pms_records WHERE id IN (SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY id")
    .bind(JSON.stringify(historyIds)).all()).results;
  const before=await history(),taskVersion=record(await snapshot(),'task',task).version;
  await cmd('night.start'); await drainPms(db);
  const after=await snapshot();expect(after.property.businessDate).toBe('2026-09-02');
  expect(records(after,'task').filter(r=>r.data.stayId===b.stayId&&r.data.date==='2026-09-02'&&r.data.type==='stayover')).toHaveLength(1);
  expect(record(after,'task',task).version).toBe(taskVersion); expect(await history()).toEqual(before);
  expect(record(after,'guest_account',b.guestAccountId).data.due).toBe(10700);
  expect(await balance(accounts.receivable)).toBe(10700); expect(await balance(accounts.revenue)).toBe(-10000);
});
it("refuses account closing while a future active stay is behind terminal account history", async () => {
  const b=await book('2026-10-01','2026-10-02'),stay=record(await snapshot(),'stay',b.stayId).data;
  for(let i=0;i<61;i++) await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'stay',?,1,?)")
    .bind(key(),propertyId,JSON.stringify({...stay,state:'cancelled'}),Date.now()+i).run();
  await expect(cmd('guest_account.close',{id:b.guestAccountId})).rejects.toMatchObject({code:'PMS_STAY_OPEN'});
  expect(record(await snapshot(),'guest_account',b.guestAccountId).data.invoiceId).toBeFalsy();
});
it("resumes a multi-chunk night from committed sources and serializes competing workers", async () => {
  for (let i = 1; i < 26; i++)
    await cmd("room.save", { number: `R${i}`, typeId, active: true });
  const res = await cmd("reservation.create", {
    guestId,
    source: "phone",
    rooms: [
      {
        typeId,
        rateId,
        arrival: "2026-09-01",
        departure: "2026-09-02",
        adults: 2,
        quantity: 26,
      },
    ],
  });
  const s = await snapshot(),
    rooms = records(s, "room"),
    stays = records(s, "stay").filter((st) => st.data.reservationId === res.id);
  // Pre-existing in-house fixture; room readiness/check-in is covered separately.
  for (const [i, stay] of stays.entries())
    await db
      .prepare(
        "UPDATE pms_records SET data_json=? WHERE id=? AND property_id=?",
      )
      .bind(
        JSON.stringify({
          ...stay.data,
          state: "in_house",
          roomId: rooms[i].id,
          segments: [
            { roomId: rooms[i].id, from: "2026-09-01", to: "2026-09-02" },
          ],
        }),
        stay.id,
        propertyId,
      )
      .run();
  await cmd("night.start");
  await drainPms(db);
  expect((await snapshot()).property.state).toBe("closing");
  expect(await balance(accounts.revenue)).toBe(-250000);
  await Promise.all([drainPms(db), drainPms(db)]);
  expect((await snapshot()).property.businessDate).toBe("2026-09-02");
  expect(await balance(accounts.revenue)).toBe(-260000);
  expect(
    records(await snapshot(), "entry").filter((e) =>
      e.data.sourceKey.startsWith("night:"),
    ),
  ).toHaveLength(26);
});
it("protects allocated receipt links and journal lines from deletion outside the subledger", async () => {
  const b = await book();
  await checkIn(b.stayId);
  const sh = (await cmd("shift.open", { name: "Desk", opening: 0 })).id;
  await cmd("payment.receive", {
    guestAccountId: b.guestAccountId,
    amount: 10700,
    method: "cash",
    shiftId: sh,
  });
  await cmd("stay.check_out", { id: b.stayId });
  const invoice = await cmd("guest_account.close", { id: b.guestAccountId });
  await expect(
    db
      .prepare("DELETE FROM finance_allocations WHERE invoice_id=?")
      .bind(invoice.id)
      .run(),
  ).rejects.toThrow("PMS_OWNER_WORKFLOW_REQUIRED");
  const journal = await db
    .prepare("SELECT journal_id FROM pms_postings WHERE property_id=? LIMIT 1")
    .bind(propertyId)
    .first<{ journal_id: string }>();
  await expect(
    db
      .prepare("DELETE FROM finance_journal_lines WHERE journal_id=?")
      .bind(journal!.journal_id)
      .run(),
  ).rejects.toThrow(/PMS_OWNER_WORKFLOW_REQUIRED|POSTED_JOURNAL_LINES_IMMUTABLE/);
});

it('prepares reviewed PMS mappings once and retains independent approval and the financial freeze',async()=>{
 const input={chartId:f.chart,taxBps:700,serviceBps:0,taxInclusive:false,advanceTax:true,financeReviewed:true};
 const s=await snapshot(),command={operation:'accounting.bootstrap',propertyId,expectedVersion:s.property.version,input},receipt=key();
 await expect(cmd('accounting.bootstrap',{...input,financeReviewed:false})).rejects.toThrow();
 const result=await pmsCommand(db,owner,command,receipt);
 const saved=await snapshot();
 expect(saved.property.config.approvedBy).toBeNull();
 expect(new Set(Object.values(saved.property.config.accounts)).size).toBe(pmsAccountKeys.length);
 expect(await pmsCommand(db,owner,command,receipt)).toEqual(result);
 await expect(cmd('accounting.approve',{},owner)).rejects.toMatchObject({code:'PMS_INDEPENDENT_APPROVER'});
 await cmd('accounting.approve',{},reviewer);
 await cmd('property.enable');
 const reservation=await book();
 await cmd('charge.add',{guestAccountId:reservation.guestAccountId,price:1000,description:'Financial freeze example'});
 await expect(cmd('accounting.bootstrap',input)).rejects.toMatchObject({code:'PMS_ACCOUNTING_IN_USE'});
});

it('rejects non-THB hotel charts and selected accounts without creating partial mappings',async()=>{
 const input={chartId:f.chart,taxBps:700,serviceBps:0,taxInclusive:false,advanceTax:true,financeReviewed:true};
 await db.prepare("UPDATE finance_charts SET base_currency='USD' WHERE id=?").bind(f.chart).run();
 await expect(cmd('accounting.bootstrap',input)).rejects.toMatchObject({code:'PMS_CHART_INVALID'});
 await db.prepare("UPDATE finance_charts SET base_currency='THB' WHERE id=?").bind(f.chart).run();
 await db.prepare("UPDATE finance_accounts SET currency='USD' WHERE id=?").bind(accounts.cash).run();
 const before=await db.prepare('SELECT count(*) n FROM finance_accounts WHERE company_id=?').bind(f.company).first('n');
 await expect(cmd('accounting.bootstrap',{...input,accounts:{cash:accounts.cash}})).rejects.toMatchObject({code:'PMS_ACCOUNT_INVALID'});
 expect(await db.prepare('SELECT count(*) n FROM finance_accounts WHERE company_id=?').bind(f.company).first('n')).toBe(before);
});

it('filters setup details and resumes from owner records without admitting other branches',async()=>{
 const {hospitalitySetup}=await import("../../../src/server/modules/pms/public/hospitality-setup");
 const data=await hospitalitySetup(owner,f.branch,propertyId);
 expect(data.hotel?.records.some(r=>r.kind==='room')).toBe(true);
 expect(data.hotel?.records.some(r=>r.kind==='guest')).toBe(false);
 expect(data.steps.find(r=>r.id==='hotel')?.blockers).toContain('Enter a cancellation policy');
 await cmd('property.save',{name:'Test Hotel',country:'TH',timezone:'Asia/Bangkok',checkIn:'14:00',checkOut:'12:00',cancellationPolicy:'Fictional flexible cancellation'});
 expect((await hospitalitySetup(owner,f.branch,propertyId)).steps.find(r=>r.id==='hotel')?.blockers).toEqual([]);
 await expect(hospitalitySetup(owner,'foreign-branch')).rejects.toMatchObject({code:'BRANCH_NOT_FOUND'});
 const cashier=await f.member('setup-cashier','pms_cashier');
 await expect(hospitalitySetup(cashier,f.branch,propertyId)).rejects.toMatchObject({code:'PERMISSION_REQUIRED'});
 const financeData=await hospitalitySetup(reviewer,f.branch,propertyId);
 expect(financeData.products).toEqual([]);
 expect(financeData.permissions.hotel).toBe(false);
 await db.prepare("UPDATE finance_charts SET base_currency='USD' WHERE id=?").bind(f.chart).run();
 expect((await hospitalitySetup(owner,f.branch,propertyId)).steps.find(r=>r.id==='finance')?.blockers).toContain('Finance must review a THB Commerce chart');
});

it('does not reuse an applied damage deposit as a later room-advance refund source',async()=>{
 const b=await book(),shift=(await cmd('shift.open',{name:'Desk',opening:0})).id;
 const deposit=(await cmd('payment.deposit',{guestAccountId:b.guestAccountId,amount:5000,method:'cash',shiftId:shift})).id;
 await cmd('charge.add',{guestAccountId:b.guestAccountId,price:3000,description:'Damage charge'});
 await cmd('payment.apply_deposit',{guestAccountId:b.guestAccountId,amount:3210,reason:'Apply damage deposit'});
 const receipt=(await cmd('payment.receive',{guestAccountId:b.guestAccountId,amount:5000,method:'cash',shiftId:shift})).id;
 const request=(await cmd('refund.request',{guestAccountId:b.guestAccountId,pocket:'advance',amount:5000,reason:'Return unused room advance'})).id;
 await cmd('refund.approve',{id:request},reviewer);const paid=(await cmd('refund.pay',{id:request,method:'cash',shiftId:shift})).id;
 expect(record(await snapshot(),'entry',paid).data.funding).toEqual([{entryId:receipt,amount:5000}]);
 expect(record(await snapshot(),'entry',paid).data.funding?.some(p=>p.entryId===deposit)).toBe(false);
});
it('serializes a damage-deposit refund against applying the same deposit to a charge',async()=>{
 const b=await book(),shift=(await cmd('shift.open',{name:'Desk',opening:0})).id;
 await cmd('payment.deposit',{guestAccountId:b.guestAccountId,amount:5000,method:'cash',shiftId:shift});
 await cmd('charge.add',{guestAccountId:b.guestAccountId,price:3000,description:'Damage charge'});
 const result=await Promise.allSettled([cmd('refund.request',{guestAccountId:b.guestAccountId,pocket:'deposit',amount:4000,reason:'Return damage deposit'}),cmd('payment.apply_deposit',{guestAccountId:b.guestAccountId,amount:3210,reason:'Apply damage deposit'})]);
 expect(result.filter(r=>r.status==='fulfilled')).toHaveLength(1);
 const f=record(await snapshot(),'guest_account',b.guestAccountId);expect(f.data.deposit).toBeGreaterThanOrEqual(0);expect(f.data.due).toBeGreaterThanOrEqual(0);
});

it('pages booking queries while retaining related records and honoring guest search and hotel scope',async()=>{
 const {plusDays}=await import("../../../src/server/modules/pms/public/rules/pms-model");const ids:string[]=[];
 for(let i=0;i<55;i++)ids.push((await cmd('reservation.create',{guestId,source:'direct',rooms:[{typeId,rateId,arrival:plusDays('2026-09-01',i),departure:plusDays('2026-09-02',i),adults:2,quantity:1}]})).id);
 const first=await pmsWorkspace(owner,propertyId,false,false,{view:'list',section:'reservations',page:0,q:'',status:''});
 expect(first.snapshot!.listMeta).toMatchObject({page:0,hasNext:true});expect(first.snapshot!.listMeta!.ids).toHaveLength(50);
 const second=await pmsWorkspace(owner,propertyId,false,false,{view:'list',section:'reservations',page:1,q:'',status:''});expect(second.snapshot!.listMeta!.ids).toHaveLength(5);expect(second.snapshot!.listMeta!.hasNext).toBe(false);
 expect(first.snapshot!.listMeta!.ids.some(id=>second.snapshot!.listMeta!.ids.includes(id))).toBe(false);
 const number=record(await snapshot(),'reservation',ids[54]).data.number;
 const found=await pmsWorkspace(owner,propertyId,false,false,{view:'list',section:'reservations',page:0,q:number,status:''});expect(found.snapshot!.listMeta!.ids).toEqual([ids[54]]);expect(records(found.snapshot!,'guest')).toHaveLength(1);expect(records(found.snapshot!,'stay')).toHaveLength(1);
 const desk=await pmsWorkspace(owner,propertyId,false,true,{view:'list',section:'front-desk',page:0,q:'',status:'arrivals'});expect(desk.snapshot!.listMeta!.ids).toHaveLength(1);expect(records(desk.snapshot!,'stay')[0].data.arrival).toBe('2026-09-01');
 await expect(pmsWorkspace(owner,propertyId,false,false,{view:'list',section:'reservations',page:0,q:'',status:'',from:'2026-01-01',to:'2026-12-31'})).rejects.toThrow();
});


it("retains guest preferences and immutable service request history across legacy edits and retries",async()=>{
 await cmd('guest.save',{id:guestId,name:'Test Guest',preferredLanguage:'th',roomPreference:'Quiet upper floor'});
 const state=await snapshot();
 const payload={operation:'guest.service_request',propertyId,expectedVersion:state.property.version,input:{guestId,text:'Extra towels'}};
 const idem=key(); const created=await pmsCommand(db,owner,payload,idem);
 expect(await pmsCommand(db,owner,payload,idem)).toEqual(created);
 await cmd('guest.save',{id:guestId,name:'Renamed guest'});
 let guest=record(await snapshot(),'guest',guestId);
 expect(guest.data.preferredLanguage).toBe('th'); expect(guest.data.roomPreference).toBe('Quiet upper floor');
 expect(guest.data.serviceRequests).toHaveLength(1);
 const request=guest.data.serviceRequests![0]; expect(request.createdBy).toBe(owner.userId);
 await cmd('guest.service_request',{guestId,requestId:request.id,text:'Extra towels delivered',state:'completed'});
 guest=record(await snapshot(),'guest',guestId);
 expect(guest.data.serviceRequests![0]).toMatchObject({createdAt:request.createdAt,createdBy:owner.userId,state:'completed'});
 await expect(cmd('guest.service_request',{guestId,requestId:request.id,text:'Rewrite closed',state:'open'})).rejects.toMatchObject({code:'PMS_REQUEST_CLOSED'});
 const cleaner=await f.member('guest-request-cleaner','pms_housekeeping');
 await expect(cmd('guest.service_request',{guestId,text:'No access'},cleaner)).rejects.toMatchObject({status:403});
 const other=(await cmd('guest.save',{name:'Other guest'})).id;
 await expect(cmd('guest.service_request',{guestId:other,requestId:request.id,text:'Wrong guest'})).rejects.toMatchObject({code:'PMS_REQUEST_NOT_FOUND'});
});


it("uses complete original receipt funding beyond a UI page while preserving unrelated closed account history", async () => {
  const booking = await book(), shiftId = (await cmd("shift.open",{name:"Receipt history cashier",opening:0})).id;
  const receiptIds: string[] = [];
  for (let i=0;i<60;i++) receiptIds.push((await cmd("payment.receive",{guestAccountId:booking.guestAccountId,amount:100,method:"cash",shiftId})).id);
  const account = record(await snapshot(),"guest_account",booking.guestAccountId);
  const unrelatedIds=Array.from({length:75},(_,i)=>`closed-history-${propertyId}-${i}`);
  await db.batch(unrelatedIds.map(id=>db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'guest_account',?,1,1)")
    .bind(id,propertyId,JSON.stringify({...account.data,state:"closed",due:0,advance:0,advanceTax:0,deposit:0}))));
  const history = async()=> (await db.prepare("SELECT id,data_json,version,updated_at FROM pms_records WHERE property_id=? AND kind='guest_account' AND id LIKE ? ORDER BY id").bind(propertyId,`closed-history-${propertyId}-%`).all()).results;
  const originalHistory = await history();
  const refundId=(await cmd("refund.request",{guestAccountId:booking.guestAccountId,amount:150,pocket:"advance",reason:"Original receipts beyond the first UI page"})).id;
  await cmd("refund.approve",{id:refundId},reviewer);
  await cmd("refund.pay",{id:refundId,method:"cash",shiftId});
  const after=await snapshot(), payout=records(after,"entry").find(r=>r.data.originalId===refundId)!;
  expect(payout.data.funding).toEqual([{entryId:receiptIds[0],amount:100},{entryId:receiptIds[1],amount:50}]);
  expect(record(after,"guest_account",booking.guestAccountId).data.advance).toBe(5850);
  expect(record(after,"shift",shiftId).data.cash).toBe(5850);
  expect(await balance(accounts.cash)).toBe(5850);
  expect(await balance(accounts.advance)).toBe(-5850);
  expect(await history()).toEqual(originalHistory);
});
it("retains every pending refund beyond the first UI page when rejecting a new receipt", async () => {
  const booking=await book(), shiftId=(await cmd("shift.open",{name:"Pending-history cashier",opening:0})).id;
  await cmd("payment.receive",{guestAccountId:booking.guestAccountId,amount:1000,method:"cash",shiftId});
  const pending=(await cmd("refund.request",{guestAccountId:booking.guestAccountId,amount:100,pocket:"advance",reason:"Old pending request"})).id;
  await db.batch(Array.from({length:60},(_,i)=>db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'refund',?,1,?)")
    .bind(`rejected-history-${propertyId}-${i}`,propertyId,JSON.stringify({guestAccountId:booking.guestAccountId,amount:1,pocket:"advance",reason:"Rejected legacy request",state:"rejected",requestedBy:owner.userId,approvedBy:operationalReviewer.userId}),Date.now()+i)));
  await expect(cmd("payment.receive",{guestAccountId:booking.guestAccountId,amount:100,method:"cash",shiftId})).rejects.toMatchObject({code:"PMS_REFUND_PENDING"});
  expect(record(await snapshot(),"refund",pending).data.state).toBe("requested");
  expect(await balance(accounts.cash)).toBe(1000);
});


it("room move supersedes every old-room task beyond a UI page without modifying unrelated closed-account history", async () => {
  const b=await book('2026-09-01','2026-09-03');await checkIn(b.stayId);
  const originalRoom=roomId;
  const second=(await cmd('room.save',{number:'102',typeId,active:true})).id;
  roomId=second;await clean();roomId=originalRoom;
  const template=records(await snapshot(),'task').find(t=>t.data.roomId===originalRoom)!;
  const tasks:string[]=[],history:string[]=[];
  for(let n=0;n<61;n++){
    const taskId=key();tasks.push(taskId);
    await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'task',?,1,1)").bind(taskId,propertyId,JSON.stringify({...template.data,state:'pending',type:'arrival',stayId:`legacy-task-${n}`,completedBy:'',inspectedBy:''})).run();
    const accountId=key();history.push(accountId);
    await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'guest_account',?,1,1)").bind(accountId,propertyId,JSON.stringify({guestId,reservationId:`unrelated-legacy-${n}`,name:`Closed history ${n}`,partyId:'',state:'closed',due:0,advance:0,advanceTax:0,deposit:0})).run();
  }
  const before=(await db.prepare('SELECT id,data_json,version,updated_at FROM pms_records WHERE id IN (SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY id').bind(JSON.stringify(history)).all()).results;
  await cmd('stay.move',{id:b.stayId,roomId:second,reason:'Ready replacement room'});
  const after=await snapshot();
  expect(record(after,'stay',b.stayId).data.roomId).toBe(second);
  expect(tasks.every(id=>record(after,'task',id).data.state==='superseded')).toBe(true);
  expect(record(after,'room',originalRoom).data.cleanliness).toBe('dirty');
  expect(records(after,'task').filter(t=>t.data.roomId===originalRoom&&t.data.stayId===b.stayId&&t.data.type==='departure')).toHaveLength(1);
  expect((await db.prepare('SELECT id,data_json,version,updated_at FROM pms_records WHERE id IN (SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY id').bind(JSON.stringify(history)).all()).results).toEqual(before);
});

it("reservation cancellation sees terminal booking siblings beyond a UI page and rolls back without changing the reserved room", async () => {
  const b=await book(),s=await snapshot(),stay=record(s,'stay',b.stayId);
  for(let n=0;n<61;n++)await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'stay',?,1,1)").bind(key(),propertyId,JSON.stringify({...stay.data,state:'checked_out'})).run();
  await expect(cmd('reservation.cancel',{id:b.reservationId,reason:'Whole booking cancellation'})).rejects.toMatchObject({code:'PMS_ALREADY_CHECKED_IN'});
  const after=await snapshot();
  expect(record(after,'stay',b.stayId)).toEqual(stay);
  expect(record(after,'reservation',b.reservationId).data.state).toBe('confirmed');
  expect(after.property.version).toBe(s.property.version);
});

it("checkout includes all same-party credit exposure beyond a page and excludes unrelated parties without losing account funding", async () => {
  const party=key(),otherParty=key();
  for(const id of [party,otherParty])await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Legacy credit payer','THB',1,1)").bind(id,f.company,id).run();
  const credit=(await cmd('credit.request',{partyId:party,limit:20000,terms:30})).id;
  await cmd('credit.approve',{id:credit},reviewer);
  const b=await book();await checkIn(b.stayId);await cmd('stay.preview_checkout',{id:b.stayId});
  await cmd('guest_account.credit',{id:b.guestAccountId,creditId:credit});
  const account=record(await snapshot(),'guest_account',b.guestAccountId),legacy:string[]=[];
  // Synthetic legacy credit metadata tests exposure, not fabricated historical GL.
  for(let n=0;n<61;n++){
    const id=key();legacy.push(id);
    await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'guest_account',?,1,1)").bind(id,propertyId,JSON.stringify({...account.data,reservationId:`legacy-credit-${n}`,name:`Legacy exposure ${n}`,due:200})).run();
  }
  const entriesBefore=records(await snapshot(),'entry').filter(e=>e.data.guestAccountId===b.guestAccountId);
  await expect(cmd('stay.check_out',{id:b.stayId})).rejects.toMatchObject({code:'PMS_CREDIT_LIMIT'});
  expect(record(await snapshot(),'stay',b.stayId).data.state).toBe('in_house');
  expect(records(await snapshot(),'entry').filter(e=>e.data.guestAccountId===b.guestAccountId)).toEqual(entriesBefore);
  for(const id of legacy)await db.prepare("UPDATE pms_records SET data_json=jsonb_set(data_json::jsonb,'{partyId}',to_jsonb(?::text))::text WHERE id=?").bind(otherParty,id).run();
  await cmd('stay.check_out',{id:b.stayId});
  expect(record(await snapshot(),'stay',b.stayId).data.state).toBe('checked_out');
  expect(record(await snapshot(),'guest_account',b.guestAccountId).data.due).toBe(10700);
  expect(records(await snapshot(),'entry').filter(e=>e.data.guestAccountId===b.guestAccountId)).toEqual(entriesBefore);
  expect(await balance(accounts.receivable)).toBe(10700);
});


it("checkout preserves original receipt allocations of another issued credit account for the same payer", async () => {
  const party=key();
  await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Shared company payer','THB',1,1)").bind(party,f.company,party).run();
  const credit=(await cmd('credit.request',{partyId:party,limit:100000,terms:30})).id;await cmd('credit.approve',{id:credit},reviewer);
  const first=await book();await checkIn(first.stayId);await cmd('stay.preview_checkout',{id:first.stayId});await cmd('guest_account.credit',{id:first.guestAccountId,creditId:credit});await cmd('stay.check_out',{id:first.stayId});
  const invoice=(await cmd('guest_account.close',{id:first.guestAccountId})).id;
  const shift=(await cmd('shift.open',{name:'Issued credit settlement',opening:0})).id;
  await cmd('payment.receive',{guestAccountId:first.guestAccountId,amount:1000,method:'cash',shiftId:shift});
  const allocationsBefore=(await db.prepare('SELECT payment_id,invoice_id,amount_minor FROM finance_allocations WHERE invoice_id=? ORDER BY payment_id').bind(invoice).all()).results;
  expect(allocationsBefore).toHaveLength(1);
  const second=await book();await checkIn(second.stayId);await cmd('stay.preview_checkout',{id:second.stayId});await cmd('guest_account.credit',{id:second.guestAccountId,creditId:credit});await cmd('stay.check_out',{id:second.stayId});
  expect((await db.prepare('SELECT payment_id,invoice_id,amount_minor FROM finance_allocations WHERE invoice_id=? ORDER BY payment_id').bind(invoice).all()).results).toEqual(allocationsBefore);
  expect(record(await snapshot(),'guest_account',first.guestAccountId).data.due).toBe(9700);
  expect(await db.prepare('SELECT outstanding_minor FROM finance_invoices WHERE id=?').bind(invoice).first('outstanding_minor')).toBe(9700);
  expect(await balance(accounts.cash)).toBe(1000);
});

function posCharge(stayId:string){return {source:'pos' as const,sourceId:'scoped-pos-sale',sourceLineId:'line',companyId:f.company,branchId:f.branch,propertyId,stayId,currency:'THB',revenueOwner:'pms' as const,sourceJournalId:null,description:'Cafe',net:1000,service:0,tax:70,total:1070};}
async function seedPosDependencies(rows:Array<{id:string;kind:string;data:object}>){
 await db.transaction(async tx=>{await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
  for(const row of rows)await tx.prepare('INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,?,?,1,1)').bind(row.id,propertyId,row.kind,JSON.stringify(row.data)).run();
 });
}
it('POS duplicate identity remains hotel-wide after checkout and beyond a UI page',async()=>{
 const b=await book();await checkIn(b.stayId);const charge=posCharge(b.stayId);let s=await snapshot();
 const posted=await db.transaction(tx=>applyPosRoomCharge(tx,owner,charge,s.property.version));
 await db.transaction(async tx=>{await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();await tx.prepare("UPDATE pms_records SET data_json=jsonb_set(data_json::jsonb,'{state}','\"checked_out\"')::text WHERE id=?").bind(b.stayId).run();await tx.prepare("UPDATE pms_records SET data_json=jsonb_set(data_json::jsonb,'{state}','\"closed\"')::text WHERE id=?").bind(b.guestAccountId).run();});
 await seedPosDependencies(Array.from({length:61},(_,n)=>({id:`pos-unrelated-${n}`,kind:'entry',data:{guestAccountId:'unrelated',sourceKey:`pos:unrelated:${n}`,total:1}})));
 s=await snapshot();const before=JSON.stringify(s);
 expect(await db.transaction(tx=>applyPosRoomCharge(tx,owner,charge,s.property.version))).toEqual(posted);
 await expect(db.transaction(tx=>applyPosRoomCharge(tx,owner,{...charge,stayId:'another-stay'},s.property.version))).rejects.toMatchObject({code:'PMS_CHARGE_CONFLICT'});
 await expect(db.transaction(tx=>applyPosRoomCharge(tx,owner,{...charge,description:'Changed cafe'},s.property.version))).rejects.toMatchObject({code:'PMS_CHARGE_CONFLICT'});
 expect(JSON.stringify(await snapshot())).toBe(before);
});
it('POS charge reads every pending refund in the target account before allocating advance',async()=>{
 const b=await book();await checkIn(b.stayId);
 await seedPosDependencies([...Array.from({length:61},(_,n)=>({id:`pos-refund-history-${n}`,kind:'refund',data:{guestAccountId:b.guestAccountId,state:'rejected',amount:1}})),{id:'pos-hidden-pending-refund',kind:'refund',data:{guestAccountId:b.guestAccountId,state:'requested',amount:1}}]);
 const s=await snapshot(),before=JSON.stringify(s);
 await expect(db.transaction(tx=>applyPosRoomCharge(tx,owner,posCharge(b.stayId),s.property.version))).rejects.toMatchObject({code:'PMS_REFUND_PENDING'});
 expect(JSON.stringify(await snapshot())).toBe(before);
});
it('POS reversal detects an original reversal outside the charged account',async()=>{
 const b=await book();await checkIn(b.stayId);let s=await snapshot();
 const posted=await db.transaction(tx=>applyPosRoomCharge(tx,owner,posCharge(b.stayId),s.property.version));
 await seedPosDependencies([...Array.from({length:61},(_,n)=>({id:`pos-other-reversal-${n}`,kind:'entry',data:{guestAccountId:'other-account',originalId:`other-${n}`,total:-1}})),{id:'pos-hidden-original-reversal',kind:'entry',data:{guestAccountId:'other-account',originalId:posted.entryId,total:-1070}}]);
 s=await snapshot();const before=JSON.stringify(s);
 await expect(db.transaction(tx=>reversePosRoomCharge(tx,owner,{propertyId,sourceKey:'pos:scoped-pos-sale:line',expectedPropertyVersion:s.property.version,reason:'Wrong room',total:1070}))).rejects.toMatchObject({code:'PMS_ALREADY_REVERSED'});
 expect(JSON.stringify(await snapshot())).toBe(before);
});

it('POS room charge allocates complete original advance sources beyond a UI page',async()=>{
 const b=await book();await checkIn(b.stayId);const shift=(await cmd('shift.open',{name:'POS funding cash',opening:0})).id;
 const first=await cmd('payment.receive',{guestAccountId:b.guestAccountId,amount:535,method:'cash',shiftId:shift});
 const second=await cmd('payment.receive',{guestAccountId:b.guestAccountId,amount:535,method:'cash',shiftId:shift});
 await seedPosDependencies(Array.from({length:61},(_,n)=>({id:`pos-funding-history-${n}`,kind:'entry',data:{guestAccountId:b.guestAccountId,kind:'charge',description:'Historical zero line',net:0,service:0,tax:0,total:0,at:n,sourceKey:''}})));
 const before=await snapshot();expect(record(before,'guest_account',b.guestAccountId).data.advance).toBe(1070);
 const posted=await db.transaction(tx=>applyPosRoomCharge(tx,owner,posCharge(b.stayId),before.property.version));
 const after=await snapshot(),account=record(after,'guest_account',b.guestAccountId),allocation=records(after,'entry').find(e=>e.data.guestAccountId===b.guestAccountId&&e.data.kind==='allocation'&&e.data.description==='Allocate advance payment')!;
 expect(account.data).toMatchObject({advance:0,advanceTax:0,due:0});
 expect(allocation.data.funding).toEqual(expect.arrayContaining([{entryId:first.id,amount:535},{entryId:second.id,amount:535}]));expect(allocation.data.funding).toHaveLength(2);
 expect(records(after,'entry').filter(e=>e.data.sourceKey==='pos:scoped-pos-sale:line').map(e=>e.id)).toEqual([posted.entryId]);
 const money=await db.prepare("SELECT amount_units FROM money_transaction_snapshots WHERE source_type='pms_entry' AND source_id=? AND amount_kind LIKE 'funding:%' ORDER BY amount_kind").bind(allocation.id).all<{amount_units:number}>();
 expect(money.results.map(m=>m.amount_units)).toEqual([535,535]);
});

it('proves cross-hotel serializable cache retries and pre-snapshot coordination without changing owner counters',async()=>{
 const branch=key();await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Cache mock branch',?,?,?)").bind(branch,branch,f.company,f.company,branch).run();
 const other=(await pmsCommand(db,owner,{operation:'property.create',input:{name:'Cache mock hotel',branchId:branch,businessDate:'2026-09-01'}},key())).id;
 const left=key(),right=key();
 await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'guest','{\"name\":\"Left mock\"}',1,1),(?,?,'guest','{\"name\":\"Right mock\"}',1,1)").bind(left,propertyId,right,other).run();
 const revision=async()=>Number(await db.prepare("SELECT revision FROM work_cache_revisions WHERE table_name='pms_records'").first('revision'));
 const before=await revision(),a=await db.pool.connect(),b=await db.pool.connect(),namespace='test:pms:cache',coord=key();
 const waitForBlocked=async(pid:number)=>{const deadline=Date.now()+5000;while(Date.now()<deadline){if((await db.pool.query('SELECT cardinality(pg_blocking_pids($1))>0 waiting',[pid])).rows[0].waiting)return;await new Promise(r=>setTimeout(r,5));}throw Error('NATIVE_QUEUED_CACHE_LOCK_REQUIRED');};
 const edit='UPDATE pms_records SET version=version+1 WHERE id=$1';
 try{
  await a.query('BEGIN ISOLATION LEVEL SERIALIZABLE');await b.query('BEGIN ISOLATION LEVEL SERIALIZABLE');
  await a.query('SELECT version FROM pms_properties WHERE id=$1 FOR UPDATE',[propertyId]);await b.query('SELECT version FROM pms_properties WHERE id=$1 FOR UPDATE',[other]);
  await a.query(edit,[left]);const pid=Number((await b.query('SELECT pg_backend_pid() pid')).rows[0].pid);
  const blocked=b.query(edit,[right]).then(()=>null,error=>error as {code:string});await waitForBlocked(pid);await a.query('COMMIT');
  expect(await blocked).toMatchObject({code:'40001'});await b.query('ROLLBACK');
  expect(await revision()).toBe(before+1);expect(await db.prepare('SELECT version FROM pms_records WHERE id=?').bind(right).first('version')).toBe(1);
  // The shared dependency lock precedes both SERIALIZABLE snapshots. Each owner
  // still locks its own Property. No unrelated domain uses this coordinator.
  await a.query('SELECT pg_advisory_lock(hashtext($1),hashtext($2))',[namespace,coord]);await a.query('BEGIN ISOLATION LEVEL SERIALIZABLE');
  await a.query('SELECT version FROM pms_properties WHERE id=$1 FOR UPDATE',[propertyId]);await a.query(edit,[left]);
  const coordinated=(async()=>{await b.query('SELECT pg_advisory_lock(hashtext($1),hashtext($2))',[namespace,coord]);await b.query('BEGIN ISOLATION LEVEL SERIALIZABLE');await b.query('SELECT version FROM pms_properties WHERE id=$1 FOR UPDATE',[other]);await b.query(edit,[right]);await b.query('COMMIT');})();
  const joined=coordinated.then(()=>null,error=>error as {code:string});await waitForBlocked(pid);await a.query('COMMIT');await a.query('SELECT pg_advisory_unlock(hashtext($1),hashtext($2))',[namespace,coord]);expect(await joined).toBeNull();
  expect(await revision()).toBe(before+3);expect(await db.prepare('SELECT version FROM pms_records WHERE id=?').bind(right).first('version')).toBe(2);
 }finally{for(const client of [a,b]){await client.query('ROLLBACK').catch(()=>{});await client.query('SELECT pg_advisory_unlock_all()').catch(()=>{});client.release();}}
});

it('commits independent verified Property cache scopes under SERIALIZABLE without losing logical revision increments',async()=>{
 const branch=key();await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Counter scope mock',?,?,?)").bind(branch,branch,f.company,f.company,branch).run();
 const other=(await pmsCommand(db,owner,{operation:'property.create',input:{name:'Independent counter scope',branchId:branch,businessDate:'2026-09-01'}},key())).id;
 const left=key(),right=key();await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'guest','{\"name\":\"Left mock\"}',1,1),(?,?,'guest','{\"name\":\"Right mock\"}',1,1)").bind(left,propertyId,right,other).run();
 const revisions=async()=>Object.fromEntries((await db.prepare("SELECT table_name,revision FROM work_cache_revisions WHERE table_name IN ('pms_records','pms_properties') ORDER BY table_name").all<{table_name:string;revision:number|string}>()).results.map(r=>[r.table_name,Number(r.revision)]));
 const before=await revisions(),a=await db.pool.connect(),b=await db.pool.connect();
 try{
  for(const [client,id] of [[a,propertyId],[b,other]] as const){
   await client.query('BEGIN ISOLATION LEVEL SERIALIZABLE');
   await client.query("SELECT set_config('erpx.pms_write','on',true),set_config('erpx.pms_property',$1,true)",[id]);
   await client.query('SELECT version FROM pms_properties WHERE id=$1 FOR UPDATE',[id]);
  }
  await a.query('UPDATE pms_records SET version=version+1 WHERE id=$1',[left]);
  await a.query('UPDATE pms_properties SET version=version+1 WHERE id=$1',[propertyId]);
  const second=(async()=>{await b.query('UPDATE pms_records SET version=version+1 WHERE id=$1',[right]);await b.query('UPDATE pms_properties SET version=version+1 WHERE id=$1',[other]);await b.query('COMMIT');})().then(()=>null,error=>({code:(error as {code?:string}).code??'UNKNOWN'}));
  await a.query('COMMIT');expect(await second).toBeNull();
  expect(await revisions()).toEqual({pms_records:before.pms_records+2,pms_properties:before.pms_properties+2});
  expect(Number(await db.prepare('SELECT version FROM pms_records WHERE id=?').bind(right).first('version'))).toBe(2);
  await a.query('BEGIN ISOLATION LEVEL SERIALIZABLE');await a.query("SELECT set_config('erpx.pms_property',$1,true)",[propertyId]);await a.query('UPDATE pms_records SET version=version+1 WHERE id=$1',[left]);await a.query('ROLLBACK');
  expect(await revisions()).toEqual({pms_records:before.pms_records+2,pms_properties:before.pms_properties+2});
  expect((await a.query("SELECT NULLIF(current_setting('erpx.pms_property',true),'') scope")).rows[0].scope).toBeNull();
 }finally{for(const client of [a,b]){await client.query('ROLLBACK').catch(()=>{});client.release();}}
});

async function settleBeforePropertyUnlock<T>(pending:Promise<T>):Promise<T|null>{
 let timer:ReturnType<typeof setTimeout>|undefined;
 try{return await Promise.race([pending,new Promise<null>(resolve=>{timer=setTimeout(()=>resolve(null),1500);})]);}
 finally{if(timer!==undefined)clearTimeout(timer);}
}
it('does not consume the remaining pool connections with local waiters for one Property',async()=>{
 const small=new PostgresDatabase({...db.pool.options,max:2}),lock={namespace:'test:pms:pool',key:key()};
 let release!:()=>void,entered!:()=>void;const gate=new Promise<void>(resolve=>{release=resolve;}),started=new Promise<void>(resolve=>{entered=resolve;});
 let first:Promise<unknown>|undefined,second:Promise<unknown>|undefined,other:Promise<unknown>|undefined;
 try{
  first=small.transaction(async tx=>{await tx.prepare('SELECT 1').first();entered();await gate;return 'first';},{sessionLock:lock});await started;
  second=small.transaction(async tx=>{await tx.prepare('SELECT 1').first();return 'second';},{sessionLock:lock});
  await new Promise(resolve=>setImmediate(resolve));
  other=small.transaction(async tx=>{await tx.prepare('SELECT 1').first();return 'other Property';},{sessionLock:{...lock,key:key()}});
  expect(await settleBeforePropertyUnlock(other)).toBe('other Property');
  release();expect(await first).toBe('first');expect(await second).toBe('second');
 }finally{release();await Promise.allSettled([first,second,other].filter((p):p is Promise<unknown>=>!!p));await small.close();}
});
it('retains the PostgreSQL Property lock across clients while local waiters leave capacity for other Properties',async()=>{
 const small=new PostgresDatabase({...db.pool.options,max:2}),external=await db.pool.connect(),lock={namespace:'test:pms:external',key:key()};
 let first:Promise<unknown>|undefined,second:Promise<unknown>|undefined,other:Promise<unknown>|undefined;
 try{
  await external.query('SELECT pg_advisory_lock(hashtext($1),hashtext($2))',[lock.namespace,lock.key]);const pid=Number((await external.query('SELECT pg_backend_pid() pid')).rows[0].pid);
  first=small.transaction(async tx=>{await tx.prepare('SELECT 1').first();return 'first';},{sessionLock:lock});
  const deadline=Date.now()+5000;let queued=false;
  while(Date.now()<deadline){queued=Boolean((await external.query('SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND $1=ANY(pg_blocking_pids(pid))) queued',[pid])).rows[0].queued);if(queued)break;await new Promise(resolve=>setTimeout(resolve,5));}expect(queued).toBe(true);
  second=small.transaction(async()=>{throw Error('Release this failed waiter');},{sessionLock:lock}).then(()=>null,error=>(error as Error).message);
  await new Promise(resolve=>setImmediate(resolve));other=small.transaction(async tx=>{await tx.prepare('SELECT 1').first();return 'other Property';},{sessionLock:{...lock,key:key()}});
  expect(await settleBeforePropertyUnlock(other)).toBe('other Property');
  await external.query('SELECT pg_advisory_unlock(hashtext($1),hashtext($2))',[lock.namespace,lock.key]);expect(await first).toBe('first');expect(await second).toBe('Release this failed waiter');
  expect(await small.transaction(async tx=>{await tx.prepare('SELECT 1').first();return 'after failure';},{sessionLock:lock})).toBe('after failure');
 }finally{await external.query('SELECT pg_advisory_unlock_all()');external.release();await Promise.allSettled([first,second,other].filter((p):p is Promise<unknown>=>!!p));await small.close();}
});

it('expires a local lock waiter without letting the next command bypass the active owner',async()=>{
 const small=new PostgresDatabase({...db.pool.options,max:2}),lock={namespace:'test:pms:expiry',key:key()};
 let release!:()=>void,entered!:()=>void;const gate=new Promise<void>(resolve=>{release=resolve;}),started=new Promise<void>(resolve=>{entered=resolve;});
 let first:Promise<unknown>|undefined,expired:Promise<unknown>|undefined,next:Promise<unknown>|undefined,timer:ReturnType<typeof setTimeout>|undefined;
 try{
  first=small.transaction(async tx=>{await tx.prepare('SELECT 1').first();entered();await gate;return 'first';},{sessionLock:lock});await started;
  expired=small.transaction(async()=>{throw Error('Expired waiter must never execute');},{sessionLock:lock}).then(result=>({result}),error=>({error}));
  const result=await Promise.race([expired,new Promise<null>(resolve=>{timer=setTimeout(()=>resolve(null),12_000);})]);if(timer!==undefined)clearTimeout(timer);
  expect(result).toMatchObject({error:{code:'55P03'}});
  next=small.transaction(async tx=>{await tx.prepare('SELECT 1').first();return 'next';},{sessionLock:lock});expect(await settleBeforePropertyUnlock(next)).toBeNull();
  release();expect(await first).toBe('first');expect(await next).toBe('next');
 }finally{if(timer!==undefined)clearTimeout(timer);release();await Promise.allSettled([first,expired,next].filter((p):p is Promise<unknown>=>!!p));await small.close();}
});

it('releases the local capacity turn between serialization retries so a queued Property command can progress',async()=>{
 const small=new PostgresDatabase({...db.pool.options,max:2}),lock={namespace:'test:pms:retry',key:key()},order:string[]=[];
 let release!:()=>void,entered!:()=>void,attempt=0;const gate=new Promise<void>(resolve=>{release=resolve;}),started=new Promise<void>(resolve=>{entered=resolve;});
 let first:Promise<unknown>|undefined,second:Promise<unknown>|undefined;
 try{
  first=small.transaction(async tx=>{await tx.prepare('SELECT 1').first();if(attempt++===0){order.push('first');entered();await gate;throw Object.assign(Error('Retry fixture'),{code:'40001'});}order.push('retry');return 'retried';},{sessionLock:lock});await started;
  second=small.transaction(async tx=>{await tx.prepare('SELECT 1').first();order.push('second');return 'queued';},{sessionLock:lock});release();
  expect(await first).toBe('retried');expect(await second).toBe('queued');expect(order).toEqual(['first','second','retry']);
 }finally{release();await Promise.allSettled([first,second].filter((p):p is Promise<unknown>=>!!p));await small.close();}
});

it('selects transaction-local indexed reads for actual PMS commands without changing their Property lock',async()=>{
 const transaction=vi.spyOn(db,'transaction');
 try{
  await cmd('guest.save',{name:'Indexed mock guest'});
  expect(transaction.mock.calls.some(([,options])=>options?.preferIndexedReads===true&&options.pipelineBatches===true&&options.sessionLock.namespace==='erpx:pms:property'&&options.sessionLock.key===propertyId)).toBe(true);
 }finally{transaction.mockRestore();}
});

it('retains the original Property lock before rejecting a stale guest envelope',async()=>{
 const stale=(await snapshot()).property.version;await cmd('guest.save',{name:'Advance mock hotel version'});
 let release!:()=>void,entered!:()=>void;const gate=new Promise<void>(resolve=>{release=resolve;}),started=new Promise<void>(resolve=>{entered=resolve;});
 const held=db.transaction(async()=>{entered();await gate;},{sessionLock:{namespace:'erpx:pms:property',key:propertyId}});
 await started;const transaction=vi.spyOn(db,'transaction'),commandKey=key();
 const rejected=pmsCommand(db,owner,{propertyId,expectedVersion:stale,operation:'guest.save',input:{name:'Stale mock guest'}},commandKey).then(value=>({value}),error=>({error}));
 try{
  expect(await settleBeforePropertyUnlock(rejected)).toBeNull();
  expect(transaction.mock.calls.some(([,options])=>options?.sessionLock.key===propertyId)).toBe(true);
  release();expect(await rejected).toMatchObject({error:{code:'PMS_VERSION_CONFLICT'}});
  expect(await db.prepare('SELECT count(*)::integer n FROM command_receipts WHERE command_id=?').bind(commandKey).first('n')).toBe(0);
 }finally{transaction.mockRestore();release();await Promise.allSettled([held,rejected]);}
});

it('preserves receipt replay and key-conflict precedence for a guest envelope after the Property version advances',async()=>{
 const body={propertyId,expectedVersion:(await snapshot()).property.version,operation:'guest.save',input:{name:'Receipt guarded mock guest'}},commandKey=key();
 const first=await pmsCommand(db,owner,body,commandKey);await cmd('guest.save',{name:'Later mock version'});
 expect(await pmsCommand(db,owner,body,commandKey)).toEqual(first);
 await expect(pmsCommand(db,owner,{...body,input:{name:'Changed mock guest'}},commandKey)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
});

it('does not expose a stale guest Property version to another Branch',async()=>{
 const stale=(await snapshot()).property.version;await cmd('guest.save',{name:'Scoped mock version'});
 await expect(pmsCommand(db,{...owner,branchId:key()},{propertyId,expectedVersion:stale,operation:'guest.save',input:{name:'Wrong branch'}},key())).rejects.toMatchObject({code:'PMS_NOT_FOUND'});
});

it('waits for an in-flight identical guest command and replays its atomically committed receipt',async()=>{
 const body={propertyId,expectedVersion:(await snapshot()).property.version,operation:'guest.save',input:{name:'Atomic mock guest receipt'}},commandKey=key();
 let release!:()=>void,entered!:()=>void;const gate=new Promise<void>(resolve=>{release=resolve;}),prepared=new Promise<void>(resolve=>{entered=resolve;});
 const original=db.transaction.bind(db);let held=false;
 const transaction=vi.spyOn(db,'transaction').mockImplementation((work,options)=>original(async tx=>{
  const result=await work(tx);
  if(!held){held=true;entered();await gate;}
  return result;
 },options));
 const first=pmsCommand(db,owner,body,commandKey);let second:typeof first|undefined;
 try{
  await prepared;second=pmsCommand(db,owner,body,commandKey);
  expect(await settleBeforePropertyUnlock(second)).toBeNull();
  release();expect(await second).toEqual(await first);
  expect(await db.prepare('SELECT count(*)::integer n FROM command_receipts WHERE command_id=?').bind(commandKey).first('n')).toBe(1);
 }finally{release();await Promise.allSettled([first,...second?[second]:[]]);transaction.mockRestore();}
});

it('rejects guest writes and committed receipt replay while the production module is disabled',async()=>{
 const body={propertyId,expectedVersion:(await snapshot()).property.version,operation:'guest.save',input:{name:'Module guarded mock guest'}},commandKey=key();
 await pmsCommand(db,owner,body,commandKey);const version=(await snapshot()).property.version;
 vi.stubEnv('NODE_ENV','production');
 await expect(pmsCommand(db,owner,body,commandKey)).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});
 const freshKey=key();await expect(pmsCommand(db,owner,{...body,expectedVersion:version},freshKey)).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});
 expect(await db.prepare('SELECT count(*)::integer n FROM command_receipts WHERE command_id=?').bind(freshKey).first('n')).toBe(0);
 expect(await db.prepare('SELECT version FROM pms_properties WHERE id=?').bind(propertyId).first('version')).toBe(version);
});

it('uses the same production pilot availability predicate for a guest write and its replay',async()=>{
 await db.prepare("INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,version,reason,changed_by,changed_at) VALUES(?,'pms','pilot',1,'Fictional native module acceptance',?,?)")
  .bind(owner.companyId,owner.userId,Date.now()).run();
 const body={propertyId,expectedVersion:(await snapshot()).property.version,operation:'guest.save',input:{name:'Activated mock guest'}},commandKey=key();
 vi.stubEnv('NODE_ENV','production');
 const first=await pmsCommand(db,owner,body,commandKey);
 expect(await pmsCommand(db,owner,body,commandKey)).toEqual(first);
 await db.prepare("UPDATE company_feature_rollouts SET rollout_state='off',version=version+1 WHERE company_id=? AND module_id='pms'").bind(owner.companyId).run();
 await expect(pmsCommand(db,owner,body,commandKey)).rejects.toMatchObject({code:'FEATURE_COMING_SOON'});
});

// Additional acceptance proofs for linen custody, early departure and DND.
it("preserves linen custody and original stock cost through replay, laundry and loss", async () => {
  const warehouse = key(), product = key();
  await db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES(?,?,?,?,?)")
    .bind(warehouse, f.company, f.branch, warehouse, "Linen store").run();
  await db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES(?,?,?,?,0)")
    .bind(product, f.company, product, "Bath towel").run();
  await db.prepare("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) VALUES(?,?,6000,1200)")
    .bind(warehouse, product).run();
  const item = (await cmd("item.save", {name:"Bath towel", kind:"linen", productId:product, warehouseId:warehouse, price:0})).id;
  const cleaner = await f.member("linen-custodian", "pms_housekeeping");
  const before = await snapshot();
  const body = {propertyId, expectedVersion:before.property.version, operation:"linen.receive", input:{itemId:item, quantity:4}};
  const receiveKey = key();
  const received = await pmsCommand(db, cleaner, body, receiveKey);
  const receivedSnapshot = await snapshot();
  expect(await pmsCommand(db, cleaner, body, receiveKey)).toEqual(received);
  expect((await snapshot()).property.version).toBe(receivedSnapshot.property.version);
  expect(await db.prepare("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?")
    .bind(warehouse,product).first()).toMatchObject({quantity_milli:2000,value_minor:400});
  expect(await db.prepare("SELECT count(*)::int n FROM inventory_ledger WHERE source_type='pms' AND product_id=?")
    .bind(product).first("n")).toBe(1);
  expect(await balance(accounts.inventory)).toBe(-800);
  expect(await balance(accounts.consumption)).toBe(800);
  expect(await balance(accounts.revenue)).toBe(0);
  await expect(cmd("linen.receive", {itemId:item,quantity:3},cleaner)).rejects.toMatchObject({code:"PMS_STOCK_SHORT"});
  expect(record(await snapshot(),"item",item)).toEqual(record(receivedSnapshot,"item",item));
  expect(records(await snapshot(),"movement")).toHaveLength(1);
  await cmd("linen.issue", {itemId:item,quantity:3,roomId},cleaner);
  await cmd("linen.return", {itemId:item,quantity:1,roomId},cleaner);
  await cmd("linen.wash", {itemId:item,quantity:1,roomId},cleaner);
  await cmd("linen.clean", {itemId:item,quantity:2,roomId},cleaner);
  await expect(cmd("linen.loss", {itemId:item,quantity:1,roomId},cleaner)).rejects.toMatchObject({code:"PMS_LINEN_SHORT"});
  await cmd("linen.loss", {itemId:item,quantity:1,roomId,reason:"Torn towel"},cleaner);
  const completed = await snapshot();
  expect(record(completed,"item",item).data).toMatchObject({clean:3,issued:0,laundry:0,lost:1});
  await expect(cmd("linen.issue", {itemId:item,quantity:4,roomId},cleaner)).rejects.toMatchObject({code:"PMS_LINEN_SHORT"});
  await expect(cmd("linen.return", {itemId:item,quantity:1,roomId},cleaner)).rejects.toMatchObject({code:"PMS_LINEN_SHORT"});
  expect(record(await snapshot(),"item",item)).toEqual(record(completed,"item",item));
  expect(records(await snapshot(),"movement")).toHaveLength(6);
  expect(await balance(accounts.inventory)).toBe(-800);
  expect(await balance(accounts.consumption)).toBe(800);
  expect(await db.prepare("SELECT count(*)::int n FROM inventory_ledger WHERE source_type='pms' AND product_id=?")
    .bind(product).first("n")).toBe(1);
});

it("releases unused future room nights on early departure without duplicate revenue or checkout", async () => {
  const booked = await book("2026-09-01","2026-09-05");
  await checkIn(booked.stayId);
  await cmd("stay.preview_checkout", {id:booked.stayId});
  const prepared = await snapshot();
  expect(record(prepared,"guest_account",booked.guestAccountId).data.due).toBe(10700);
  await expect(cmd("stay.check_out", {id:booked.stayId})).rejects.toMatchObject({code:"PMS_BALANCE_DUE"});
  expect((await snapshot()).property.version).toBe(prepared.property.version);
  const shift = (await cmd("shift.open", {name:"Early departure desk",opening:0})).id;
  await cmd("payment.receive", {guestAccountId:booked.guestAccountId,amount:10700,method:"cash",shiftId:shift});
  const ready = await snapshot();
  const body = {propertyId,expectedVersion:ready.property.version,operation:"stay.check_out",input:{id:booked.stayId,reason:"Guest leaves early"}};
  const checkoutKey = key(), completed = await pmsCommand(db,owner,body,checkoutKey);
  const departed = await snapshot();
  expect(await pmsCommand(db,owner,body,checkoutKey)).toEqual(completed);
  expect((await snapshot()).property.version).toBe(departed.property.version);
  const stay = record(departed,"stay",booked.stayId);
  expect(stay.data).toMatchObject({state:"checked_out",departure:"2026-09-02"});
  expect(stay.data.segments.every(segment => segment.to <= "2026-09-02")).toBe(true);
  expect(record(departed,"room",roomId).data.cleanliness).toBe("dirty");
  expect(record(departed,"guest_account",booked.guestAccountId).data).toMatchObject({due:0,advance:0,deposit:0});
  expect(records(departed,"entry").filter(entry => entry.data.sourceKey === `night:${booked.stayId}:2026-09-01`)).toHaveLength(1);
  expect(await balance(accounts.revenue)).toBe(-10000);
  expect(await balance(accounts.tax)).toBe(-700);
  expect(availability(departed,typeId,"2026-09-03").available).toBe(1);
  await book("2026-09-03","2026-09-04");
  expect(await balance(accounts.revenue)).toBe(-10000);
});

it("honors DND for room work and resumes the same cycle only after its release", async () => {
  const cleaner = await f.member("dnd-cleaner", "pms_housekeeping");
  const task = (await cmd("task.create", {roomId,type:"arrival",date:"2026-09-01"})).id;
  await cmd("room.dnd", {id:roomId,dnd:true},cleaner);
  const protectedSnapshot = await snapshot();
  const checked = record(protectedSnapshot,"task",task).data.checklist;
  await expect(cmd("task.start", {id:task},cleaner)).rejects.toMatchObject({code:"PMS_DND"});
  await expect(cmd("task.complete", {id:task,checked},cleaner)).rejects.toMatchObject({code:"PMS_DND"});
  const refused = await snapshot();
  expect(refused.property.version).toBe(protectedSnapshot.property.version);
  expect(record(refused,"task",task)).toEqual(record(protectedSnapshot,"task",task));
  expect(record(refused,"room",roomId)).toEqual(record(protectedSnapshot,"room",roomId));
  await cmd("room.dnd", {id:roomId,dnd:false},cleaner);
  await cmd("task.start", {id:task},cleaner);
  await cmd("task.complete", {id:task,checked},cleaner);
  expect(record(await snapshot(),"task",task).data.state).toBe("clean");
  expect(record(await snapshot(),"room",roomId).data.cleanliness).toBe("clean");
  await cmd("task.inspect", {id:task});
  expect(record(await snapshot(),"task",task).data.state).toBe("done");
  expect(record(await snapshot(),"room",roomId).data.cleanliness).toBe("inspected");
});


it("commits an in-house room upgrade once with version fencing and immutable original receipt evidence", async () => {
  const booking = await book('2026-09-01', '2026-09-04');
  await checkIn(booking.stayId);
  const suiteType = (await cmd('room_type.save', { name: 'Suite', capacity: 4, amenities: 'Living room' })).id;
  const suiteRoom = (await cmd('room.save', { number: '201', typeId: suiteType, active: true })).id;
  const suiteRate = (await cmd('rate.save', { name: 'Suite Flexible', typeId: suiteType, from: '2026-09-01', to: '2026-12-31', price: 20000 })).id;
  const taskId = (await cmd('task.create', { roomId: suiteRoom, type: 'arrival', date: '2026-09-01' })).id;
  await cmd('task.complete', { id: taskId, checked: record(await snapshot(), 'task', taskId).data.checklist });
  await cmd('task.inspect', { id: taskId }, await f.member('upgrade-inspector', 'pms_supervisor'));
  const shiftId = (await cmd('shift.open', { name: 'Upgrade desk', opening: 0 })).id;
  await cmd('payment.receive', { guestAccountId: booking.guestAccountId, shiftId, amount: 1000, method: 'cash', reference: 'Original advance' });
  const before = await snapshot();
  const originalEntries = records(before, 'entry');
  const originalDocuments = (await db.prepare('SELECT id,snapshot_json FROM pms_documents WHERE property_id=? ORDER BY id').bind(propertyId).all()).results;
  const input = { operation: 'stay.upgrade', propertyId, expectedVersion: before.property.version, input: { id: booking.stayId, roomId: suiteRoom, rateId: suiteRate, reason: 'Guest accepted suite price' } };
  const replayKey = key();
  const result = await pmsCommand(db, owner, input, replayKey);
  expect(await pmsCommand(db, owner, input, replayKey)).toEqual(result);
  const after = await snapshot(), stay = record(after, 'stay', booking.stayId);
  expect(stay.data).toMatchObject({ typeId: suiteType, roomId: suiteRoom, rateId: suiteRate });
  expect(stay.data.upgrades).toHaveLength(1);
  expect(records(after, 'entry')).toEqual(originalEntries);
  expect((await db.prepare('SELECT id,snapshot_json FROM pms_documents WHERE property_id=? ORDER BY id').bind(propertyId).all()).results).toEqual(originalDocuments);
  await expect(pmsCommand(db, owner, input, key())).rejects.toMatchObject({ code: 'PMS_VERSION_CONFLICT' });
  const cleaner = await f.member('upgrade-cleaner', 'pms_housekeeping');
  await expect(cmd('stay.upgrade', input.input, cleaner)).rejects.toMatchObject({ code: 'PERMISSION_REQUIRED' });
  expect(record(await snapshot(), 'stay', booking.stayId).data.upgrades).toHaveLength(1);
});

it("serializes concurrent last group pickups and returns quota after cancellation without erasing pickup history", async () => {
  const blockId = (await cmd('block.create', { name: 'One-room group', typeId, from: '2026-09-03', to: '2026-09-05', releaseDate: '2026-09-02', quantity: 1 })).id;
  const s = await snapshot();
  const command = { operation: 'reservation.create', propertyId, expectedVersion: s.property.version, input: { guestId, source: 'phone', blockId, rooms: [{ typeId, rateId, arrival: '2026-09-03', departure: '2026-09-05', adults: 2, quantity: 1 }] } };
  const results = await Promise.allSettled([pmsCommand(db, owner, command, key()), pmsCommand(db, owner, command, key())]);
  expect(results.filter(result => result.status === 'fulfilled')).toHaveLength(1);
  expect(results.filter(result => result.status === 'rejected')).toHaveLength(1);
  const first = record(await snapshot(), 'block', blockId).data.pickedStayIds[0];
  await cmd('stay.cancel', { id: first, reason: 'Cancelled pickup' });
  const replacement = await book('2026-09-03', '2026-09-05', { blockId });
  const final = await snapshot();
  expect(record(final, 'block', blockId).data.pickedStayIds).toEqual([first, replacement.stayId]);
  expect(availability(final, typeId, '2026-09-03')).toMatchObject({ occupied: 1, held: 0, available: 0 });
});
