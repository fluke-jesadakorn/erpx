import { prepareRoomCharge } from "../src/lib/pms-room-charge";
import { frontOperationAllowed } from "../src/server/front-app";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import { seedExpenseCompany } from "../fixtures/expenses/company";
import { pmsCommand, loadPms, drainPms, pmsWorkspace, pmsWork } from "../src/server/pms";
import {
  records,
  record,
  pmsAccountKeys,
  defaultConfig,
  amounts,
  type Config,
} from "../src/lib/pms";
import { availability, dailyRoomReport } from "../src/lib/pms-engine";
import { pmsHome,pmsTaskPage } from "../src/server/pms/home";
import { pmsFinanceOverview } from "../src/server/pms/finance";
import { readablePmsSource } from "../src/server/pms/source";
import { previewPmsImport,approvePmsImport,applyPmsImport } from "../src/server/pms/import";
import { publishHandover,acknowledgeHandover,handoverView } from "../src/server/handover";
import { homePosts,postView } from "../src/server/social-home";
import { projectPmsSocialEvent } from "../src/server/pms/social-events";
import type { EventMessage } from "../src/server/job-projector";
import { createJournal,postJournal,previewOpeningTrialBalance,importOpeningTrialBalance,openingTrialBalanceSchema } from "../src/server/finance";
import { assignFlowPrimary,resolveFlow } from "../src/server/flow-routing";
import type { WorkContext } from "../src/server/authorization";
import {
  attachmentBindingState,
  enableAttachmentV2ForTest,
  seedAttachmentSet,
} from "./helpers/attachment-v2";

it("allows staff payout of approved PMS refunds while keeping independent decisions in ERPX", () => {
  expect(frontOperationAllowed("pms", "refund.pay")).toBe(true);
  for (const operation of ["refund.approve", "refund.reject", "payment.verify_transfer", "shift.review"])
    expect(frontOperationAllowed("pms", operation)).toBe(false);
  expect(frontOperationAllowed("pos", "refund.pay")).toBe(false);
});

vi.mock("../src/server/runtime", async () => {
  const { env } = await import("./runtime-env");
  return { holdingResources: async () => ({ write: env.H001_DB }) };
});
const db = env.H001_DB,
  key = () => crypto.randomUUID();
let f: Awaited<ReturnType<typeof seedExpenseCompany>>,
  owner: WorkContext,
  reviewer: WorkContext,
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
beforeEach(async () => {
  f = await seedExpenseCompany(db, env.H001_FILES);
  owner = f.owner;
  reviewer = await f.member("hotel-reviewer", "finance_approver");
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
  rateId = (
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
  return { reservationId: r.id, stayId: stay.id, folioId: stay.data.folioId };
}
it("blocks a new booking when the selected currency lacks the hotel's account setup or manual rate", async () => {
  await expect(book("2026-09-01", "2026-09-02", { currency: "USD" })).rejects.toMatchObject({ code: "PMS_CURRENCY_SETUP_REQUIRED" });
  await expect(book("2026-09-01", "2026-09-02", { currency: "THB" })).rejects.toMatchObject({ code: "MANUAL_RATE_REQUIRED" });
  const rateId=`pms-rate-${key()}`;
  await db.prepare("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at,note) VALUES(?,?,?,?,?,?,?,?)")
    .bind(rateId, f.company, "THB", "2026-09-01", "36", reviewer.userId, Date.now(), "manual test quote").run();
  const booked=await book("2026-09-01", "2026-09-02", { currency: "THB" });
  expect(await db.prepare("SELECT currency,amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE company_id=? AND source_type='pms_reservation' AND source_id=?")
    .bind(f.company,booked.reservationId).first()).toMatchObject({currency:"THB",amount_units:"10700",usd_units:"297",fx_rate_id:rateId});
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
  const { changeCompanyFeatureRollout } = await import("../src/server/hospitality-rollout");
  await changeCompanyFeatureRollout(owner, { moduleId: "pms", state: "pilot", expectedVersion: 0, reason: "Hotel pilot acceptance rehearsal" }, key());
  vi.stubEnv("NODE_ENV", "production");
  expect((await pmsWorkspace(owner, propertyId)).availablePaymentMethods).toEqual(["cash"]);
  const request = { propertyId, expectedVersion: before.property.version, operation: "payment.receive", input: { folioId: booking.folioId, amount: 10700, method: "transfer", reference: "TYPED-ONLY", shiftId: shift } };
  await expect(pmsCommand(db, owner, request, key())).rejects.toMatchObject({ code: "PMS_BANK_VERIFICATION_REQUIRED" });
  await expect(pmsCommand(db, owner, { ...request, operation: "payment.deposit" }, key())).rejects.toMatchObject({ code: "PMS_BANK_VERIFICATION_REQUIRED" });
  await expect(pmsCommand(db, owner, { ...request, input: { ...request.input, method: "edc" } }, key())).rejects.toMatchObject({ code: "PMS_BANK_VERIFICATION_REQUIRED" });
  await expect(pmsCommand(db, owner, { ...request, operation: "refund.pay", input: { id: "not-a-refund", method: "transfer", reference: "TYPED-ONLY", shiftId: shift } }, key())).rejects.toMatchObject({ code: "PMS_BANK_VERIFICATION_REQUIRED" });
  const after = await snapshot();
  expect(after.property.version).toBe(before.property.version);
  expect(records(after, "entry").length).toBe(records(before, "entry").length);
  expect(await balance(accounts.bank)).toBe(accountBalance);
  await expect(pmsCommand(db, owner, { ...request, input: { folioId: booking.folioId, amount: 10700, method: "cash", shiftId: shift } }, key())).resolves.toMatchObject({ propertyId });
});

it("settles a pending hotel transfer only after independent exact bank evidence", async () => {
  const booking=await book();
  await checkIn(booking.stayId);
  const shift=(await cmd("shift.open",{name:"AM",opening:0})).id;
  const {changeCompanyFeatureRollout}=await import("../src/server/hospitality-rollout");
  await changeCompanyFeatureRollout(owner,{moduleId:"pms",state:"pilot",expectedVersion:0,reason:"Hotel transfer pilot rehearsal"},key());
  vi.stubEnv("NODE_ENV","production");
  const initial=await snapshot();
  const request={propertyId,expectedVersion:initial.property.version,operation:"payment.request_transfer",input:{folioId:booking.folioId,shiftId:shift,amount:10700,reference:"PMS-BANK-001"}};
  const requestKey=key();
  const pending=await pmsCommand(db,owner,request,requestKey);
  expect(pending).toMatchObject({state:"pending",transferVersion:1});
  expect(await pmsCommand(db,owner,request,requestKey)).toEqual(pending);
  expect((await snapshot()).property.version).toBe(initial.property.version);
  expect(await balance(accounts.bank)).toBe(0);
  const {hospitalityDayControls}=await import("../src/server/hospitality-day-controls");
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

it("cancels an unverified hotel transfer without changing folio or ledger", async () => {
  const booking=await book();
  await checkIn(booking.stayId);
  const shift=(await cmd("shift.open",{name:"AM",opening:0})).id;
  const before=await snapshot();
  const request={propertyId,expectedVersion:before.property.version,operation:"payment.request_transfer",input:{folioId:booking.folioId,shiftId:shift,amount:10700,reference:"PMS-CANCEL-001"}};
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
        folioId: b.folioId,
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
  const doc = await cmd("folio.close", { id: b.folioId });
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
    folioId: b.folioId,
    amount: 10700,
    method: "cash",
    shiftId: sh,
  });
  await cmd("payment.deposit", {
    folioId: b.folioId,
    amount: 5000,
    method: "cash",
    shiftId: sh,
  });
  expect(await balance(accounts.tax)).toBe(-700);
  const refund = (
    await cmd("refund.request", {
      folioId: b.folioId,
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
it("blocks closed accounting periods and keeps the payment and folio atomic", async () => {
  const b = await book(),
    sh = (await cmd("shift.open", { name: "Desk", opening: 0 })).id;
  await db
    .prepare("UPDATE finance_periods SET state='closed' WHERE id=?")
    .bind(f.period)
    .run();
  await expect(
    cmd("payment.receive", {
      folioId: b.folioId,
      amount: 10700,
      method: "cash",
      shiftId: sh,
    }),
  ).rejects.toMatchObject({ code: "PMS_PERIOD_CLOSED" });
  expect(record(await snapshot(), "folio", b.folioId).data.advance).toBe(0);
  expect(records(await snapshot(), "entry")).toHaveLength(0);
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
    folioId: b.folioId,
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
  await cmd("charge.add",{folioId:booking.folioId,description:"Private guest service",price:10000});
  const home=await pmsHome(owner);
  expect(home.hotels.map(hotel=>hotel.id)).toContain(propertyId);
  expect(home.tasks.items.some(item=>item.id===booking.stayId)).toBe(true);
  expect(home.hotels[0].receivable).toBe(10700);
  const housekeeper=await f.member("housekeeper","pms_housekeeping");
  housekeeper.branchId=f.branch;housekeeper.assignmentScope="branch";
  await db.prepare("UPDATE assignments SET scope_kind='branch',scope_node_id=? WHERE id=?").bind(f.branch,housekeeper.assignmentId).run();
  const privateHome=await pmsHome(housekeeper);
  expect(privateHome.hotels[0].receivable).toBeNull();
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
  const folio=records(await snapshot(),"folio")[0],shift=(await cmd("shift.open",{name:"New cashier",opening:10000})).id;
  const refund=(await cmd("refund.request",{folioId:folio.id,pocket:"deposit",amount:2000,reason:"Partial cutover deposit return"})).id;
  await cmd("refund.approve",{id:refund},reviewer);await cmd("refund.pay",{id:refund,method:"cash",shiftId:shift});
  expect((await pmsFinanceOverview(owner)).hotels[0].differences).toEqual({receivable:0,advance:0,deposit:0});
  await cmd("shift.close",{id:shift,counted:8000,handover:"Deposit refund recorded"});
  await cmd("night.start");await drainPms(db);
  expect(record(await snapshot(),"folio",folio.id).data.advance).toBe(9300);
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
  const stay=records(await snapshot(),"stay")[0],folioId=stay.data.folioId,shift=(await cmd("shift.open",{name:"Cutover day",opening:0})).id;
  await cmd("payment.receive",{folioId,amount:10000,method:"cash",shiftId:shift});
  expect(await db.prepare("SELECT outstanding_minor FROM finance_invoices WHERE id=?").bind(originalInvoice).first<number>("outstanding_minor")).toBe(10000);
  expect(await balance(accounts.revenue)).toBe(0);
  await cmd("shift.close",{id:shift,counted:10000,handover:"Partial opening collection"});await cmd("night.start");await drainPms(db);
  const second=(await cmd("shift.open",{name:"Second business day",opening:0})).id;
  await cmd("payment.receive",{folioId,amount:20700,method:"cash",shiftId:second});await cmd("stay.check_out",{id:stay.id});
  const invoice=await cmd("folio.close",{id:folioId});
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
      folioId: b.folioId,
      amount: 10700,
      method: "cash",
      shiftId: sh,
    })
  ).id;
  const r = (
    await cmd("refund.request", {
      folioId: b.folioId,
      amount: 10700,
      pocket: "advance",
      reason: "Change payment",
    })
  ).id;
  await cmd("refund.approve", { id: r }, reviewer);
  await cmd("refund.pay", { id: r, method: "cash", shiftId: sh });
  const second = (
    await cmd("payment.receive", {
      folioId: b.folioId,
      amount: 10700,
      method: "cash",
      shiftId: sh,
    })
  ).id;
  await checkIn(b.stayId);
  await cmd("stay.check_out", { id: b.stayId });
  const invoice = await cmd("folio.close", { id: b.folioId });
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
    folioId: b.folioId,
    amount: 12700,
    method: "cash",
    shiftId: sh,
  });
  const r = (
    await cmd("refund.request", {
      folioId: b.folioId,
      amount: 2000,
      pocket: "deposit",
      reason: "Return excess",
    })
  ).id;
  await cmd("refund.approve", { id: r }, reviewer);
  await cmd("refund.pay", { id: r, method: "cash", shiftId: sh });
  await cmd("stay.preview_checkout", { id: b.stayId });
  await cmd("payment.apply_deposit", {
    folioId: b.folioId,
    amount: 10700,
    reason: "Guest approved",
  });
  await cmd("stay.check_out", { id: b.stayId });
  const i = await cmd("folio.close", { id: b.folioId });
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
      await cmd("folio.create", {
        reservationId: b.reservationId,
        name: "Company services",
      })
    ).id;
  const e = (
    await cmd("charge.add", {
      folioId: b.folioId,
      price: 1000,
      description: "Breakfast",
    })
  ).id;
  await cmd("folio.transfer", {
    entryId: e,
    toFolioId: other,
    reason: "Company pays",
  });
  expect(await balance(accounts.revenue)).toBe(-1000);
  const transfer = records(await snapshot(), "entry").find(
    (x) => x.data.folioId === other && x.data.kind === "transfer",
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
    folioId: other,
    price: 2000,
    description: "Dinner",
  });
  await cmd("folio.merge", { id: other, toFolioId: b.folioId });
  expect(record(await snapshot(), "folio", b.folioId).data.due).toBe(2140);
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
  await cmd("folio.credit", { id: b.folioId, creditId: credit });
  await cmd("stay.check_out", { id: b.stayId });
  const invoice = await cmd("folio.close", { id: b.folioId });
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
        folioId: b.folioId,
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
    folioId: b.folioId,
    roomId,
  });
  expect(await balance(accounts.inventory)).toBe(-300);
  expect(await balance(accounts.consumption)).toBe(300);
  expect(await balance(accounts.revenue)).toBe(-1000);
  await expect(
    cmd("minibar.charge", { itemId: item, quantity: 2, folioId: b.folioId }),
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
    folioId: stays[0].data.folioId,
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
      (r) => r.kind === "guest" || r.kind === "entry" || r.kind === "folio",
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
    folioId: b.folioId,
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
      folioId: b.folioId,
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
    folioId: b.folioId,
    amount: 10700,
    method: "cash",
    shiftId: sh,
  });
  await cmd("stay.check_out", { id: b.stayId });
  const invoice = await cmd("folio.close", { id: b.folioId });
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
 await cmd('charge.add',{folioId:reservation.folioId,price:1000,description:'Financial freeze example'});
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
 const {hospitalitySetup}=await import('../src/server/hospitality-setup');
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
