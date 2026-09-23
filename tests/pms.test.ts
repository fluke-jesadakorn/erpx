import { prepareRoomCharge } from "../src/lib/pms-room-charge";
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
import type { WorkContext } from "../src/server/authorization";

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
  expect((await pmsWork(owner)).some((item) => item.id === pending.id)).toBe(false);
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
  await drainPms(db);
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
  await cmd("refund.pay", { id: refund, method: "cash", shiftId: sh });
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
  ).rejects.toThrow("PMS_OWNER_WORKFLOW_REQUIRED");
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
    cmd("charge.reverse", { id: e, reason: "Invalid origin" }),
  ).rejects.toMatchObject({ code: "PMS_ALREADY_REVERSED" });
  await cmd("charge.reverse", { id: transfer.id, reason: "Service cancelled" });
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
  await cmd("ota.settle", {
    folioId: b.folioId,
    gross: 10700,
    commission: 1200,
    fees: 100,
    reference: "Statement 001",
    shiftId: sh,
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
  await cmd("charge.reverse", { id: night.id, reason: "Approved correction" });
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
  ).rejects.toThrow("PMS_OWNER_WORKFLOW_REQUIRED");
});
