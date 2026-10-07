import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "./helpers/expense-fixture";
import {
  prepareBusinessContract,
  decideBusinessContract,
  prepareBusinessBilling,
  prepareRecognitionRun,
  decideRecognitionRun,
  reverseRecognitionRun,
  businessContractSchema,
  decideNoChargeBilling,
  prepareBusinessBillingRevision,
} from "../src/server/finance-business-contracts";
import { contractBillingCalculation } from "../src/lib/finance-contract-policy";
import { setFinanceCompanyTool } from "../src/server/finance-company-tools";
import {scheduleFinanceDraft,drainFinanceDueDrafts,decideFinanceDueDraft} from "../src/server/finance-due-drafts";
import {
  postInvoice,
  createInvoice,
  invoiceSchema,
  trialBalance,
  beginClose,cancelClose,
  voidDraftInvoice,reverseJournal,
} from "../src/server/finance";
vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let f: Awaited<ReturnType<typeof expenseFixture>>,
  ar: string,
  deferred: string,
  income: string,
  party: string;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
  f = await expenseFixture();
  f.owner.role = "finance_preparer";
  f.owner.assignmentId = await f.grant(f.owner, "finance_preparer");
  f.manager.role = "finance_approver";
  f.manager.assignmentId = await f.grant(f.manager, "finance_approver");
  ar = key();
  deferred = key();
  income = key();
  party = key();
  for (const [id, code, category, control] of [
    [ar, "AR", "asset", "receivable"],
    [deferred, "DEF", "liability", "none"],
    [income, "REV", "revenue", "none"],
  ])
    await db
      .prepare(
        "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',?,?,1,1)",
      )
      .bind(
        id,
        f.chart,
        f.company,
        code,
        code,
        category,
        control === "none" ? 0 : 1,
        control,
      )
      .run();
  await db
    .prepare(
      "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Billing customer','THB',1,1)",
    )
    .bind(party, f.company, party)
    .run();
  await setFinanceCompanyTool(
    f.owner,
    {
      tool: "business_contracts",
      enabled: true,
      expectedVersion: 0,
      reason: "Disposable accounting fixture",
    },
    key(),
  );
});
function input() {
  return businessContractSchema.parse({
    chartId: f.chart,
    partyId: party,
    number: key(),
    name: "Service contract",
    currency: "THB",
    startDate: "2026-09-16",
    endDate: "2026-10-31",
    reason: "Signed service evidence",
    cycle: "monthly",
    pricing: { kind: "flat", amount: "100.01" },
    receivableAccountId: ar,
    deferredAccountId: deferred,
    obligations: [
      {
        key: "support",
        name: "Monthly service",
        standalonePrice: "3.00",
        revenueAccountId: income,
        mode: "monthly",
      },
      {
        key: "delivery",
        name: "Delivery",
        standalonePrice: "1.00",
        revenueAccountId: income,
        mode: "point",
      },
    ],
  });
}
async function active() {
  const source = input(),
    r = await prepareBusinessContract(f.owner, source, key());
  await decideBusinessContract(
    f.manager,
    String(r.id),
    { expectedVersion: 1, reason: "Independent contract approval" },
    "approve",
    key(),
  );
  return { id: String(r.id), source };
}
it('retains the original unposted invoice and allocation while independently posting its corrected billing edition',async()=>{
 const c=await active(),original=await prepareBusinessBilling(f.owner,c.id,{expectedVersion:2,month:'2026-09',invoiceDate:'2026-09-16',dueDate:'2026-09-30'},key()),before=await db.prepare("SELECT input_json,result_json,source_hash FROM finance_business_billing_runs WHERE id=?").bind(original.id).first();
 const request={expectedVersion:1,invoiceExpectedVersion:1,billing:{month:'2026-09',invoiceDate:'2026-09-30',dueDate:'2026-10-15'},pricing:{kind:'flat' as const,amount:'80.01'},reason:'Correct the unposted service price after checking the agreed September charge'},command=key(),corrected=await prepareBusinessBillingRevision(f.owner,String(original.id),request,command);
 expect(corrected).toMatchObject({result:{subtotal:'40.01',allocations:[{key:'support',amount:'30.01'},{key:'delivery',amount:'10.00'}]}});expect(await prepareBusinessBillingRevision(f.owner,String(original.id),request,command)).toEqual(corrected);
 expect(await db.prepare("SELECT status,version,outstanding_minor FROM finance_invoices WHERE id=?").bind(original.invoiceId).first()).toEqual({status:'voided',version:2,outstanding_minor:0});expect(await db.prepare("SELECT input_json,result_json,source_hash FROM finance_business_billing_runs WHERE id=?").bind(original.id).first()).toEqual(before);
 await expect(postInvoice(f.manager,String(original.invoiceId),{periodId:f.period,entryDate:'2026-09-30',expectedVersion:2},key())).rejects.toMatchObject({code:'INVOICE_NOT_FOUND'});
 await postInvoice(f.manager,String(corrected.invoiceId),{periodId:f.period,entryDate:'2026-09-30',expectedVersion:1},key());const r=await prepareRecognitionRun(f.owner,{billingRunId:String(corrected.id),itemKey:'support:2026-09-30',periodId:f.period,entryDate:'2026-09-30',reason:'Review corrected September allocation and delivered support'},key());await decideRecognitionRun(f.manager,String(r.id),{expectedVersion:1,reason:'Independent review of the corrected deferral and retained previous bill'},'post',key());
 const tb=await trialBalance(f.owner,undefined,f.chart);expect(tb.find(a=>a.id===ar)?.balance).toBe('40.01');expect(tb.find(a=>a.id===income)?.balance).toBe('-30.01');expect(tb.find(a=>a.id===deferred)?.balance).toBe('-10.00');
 await expect(prepareBusinessBillingRevision(f.owner,String(corrected.id),{...request,invoiceExpectedVersion:2},key())).rejects.toMatchObject({code:'BILLING_POSTED_CORRECTION_REQUIRED'});expect(await db.prepare("SELECT count(*) n FROM finance_business_billing_runs WHERE root_id=?").bind(c.id).first('n')).toBe(2);
});
it('revises rejected and approved zero readings through immutable editions, including a positive draft corrected back to zero',async()=>{
 const terms=businessContractSchema.parse({...input(),cycle:'usage',pricing:{kind:'flat',amount:'1.00'}}),c=await prepareBusinessContract(f.owner,terms,key());await decideBusinessContract(f.manager,String(c.id),{expectedVersion:1,reason:'Independent review of usage service pricing'},'approve',key());
 const base={month:'2026-09',invoiceDate:'2026-09-30',dueDate:'2026-10-15',usageUnits:0,usageEvidence:'Original meter reading zero, pending source inspection'},original=await prepareBusinessBilling(f.owner,String(c.id),{expectedVersion:2,...base},key());await decideNoChargeBilling(f.manager,String(original.id),{expectedVersion:1,reason:'Reject zero reading pending a verified meter source'},'reject',key());
 const positive=await prepareBusinessBillingRevision(f.owner,String(original.id),{expectedVersion:2,billing:{...base,usageUnits:10,usageEvidence:'Corrected meter increment 10 independently checked at the site'},reason:'Retain the rejected reading and correct the measured consumption'},key());expect(positive).toMatchObject({result:{subtotal:'10.00'}});
 const final=await prepareBusinessBillingRevision(f.owner,String(positive.id),{expectedVersion:1,invoiceExpectedVersion:1,billing:{...base,usageEvidence:'Final meter evidence confirms the 10 units belonged to the next service window'},reason:'Correct the unposted ten-unit bill back to the verified zero reading'},key());expect(final).toMatchObject({invoiceId:null,result:{subtotal:'0.00'}});await decideNoChargeBilling(f.manager,String(final.id),{expectedVersion:1,reason:'Independent final meter and billing window review'},'approve',key());
 expect(await db.prepare("SELECT no_charge_status FROM finance_business_billing_runs WHERE id=?").bind(original.id).first('no_charge_status')).toBe('rejected');expect(await db.prepare("SELECT status FROM finance_invoices WHERE id=?").bind(positive.invoiceId).first('status')).toBe('voided');expect(await db.prepare("SELECT count(*) n FROM finance_journals WHERE company_id=?").bind(f.company).first('n')).toBe(0);
 const next=await prepareBusinessBillingRevision(f.owner,String(final.id),{expectedVersion:2,billing:{...base,usageEvidence:'Amended reference to the same verified zero reading'},reason:'Retain the approved source and correct its reading reference'},key());await decideNoChargeBilling(f.manager,String(next.id),{expectedVersion:1,reason:'Review the fourth retained reading edition'},'approve',key());expect(await db.prepare("SELECT bill_edition FROM finance_business_billing_runs WHERE id=?").bind(next.id).first('bill_edition')).toBe(4);
 await expect(db.prepare("UPDATE finance_business_billing_runs SET contract_content_json='{}' WHERE id=?").bind(next.id).run()).rejects.toThrow('BILLING_EVIDENCE_IMMUTABLE');
});
it('rejects changed billing windows and keeps one complete replacement under concurrent correction commands',async()=>{
 const c=await active(),bill=await prepareBusinessBilling(f.owner,c.id,{expectedVersion:2,month:'2026-09',invoiceDate:'2026-09-16',dueDate:'2026-09-30'},key()),request={expectedVersion:1,invoiceExpectedVersion:1,billing:{month:'2026-09',invoiceDate:'2026-09-30',dueDate:'2026-10-15'},reason:'Retain and correct this unposted September billing edition'};
 await expect(prepareBusinessBillingRevision(f.owner,String(bill.id),{...request,billing:{...request.billing,month:'2026-10'}},key())).rejects.toMatchObject({code:'BILLING_REVISION_WINDOW_CHANGED'});expect(await db.prepare("SELECT status FROM finance_invoices WHERE id=?").bind(bill.invoiceId).first('status')).toBe('draft');
 const results=await Promise.allSettled([prepareBusinessBillingRevision(f.owner,String(bill.id),request,key()),prepareBusinessBillingRevision(f.owner,String(bill.id),request,key())]);expect(results.filter(r=>r.status==='fulfilled')).toHaveLength(1);expect(await db.prepare("SELECT count(*) n FROM finance_business_billing_runs WHERE parent_run_id=?").bind(bill.id).first('n')).toBe(1);expect(await db.prepare("SELECT count(*) n FROM finance_invoices WHERE company_id=?").bind(f.company).first('n')).toBe(2);
 await expect(prepareBusinessBillingRevision({...f.owner,companyId:key()},String(bill.id),request,key())).rejects.toMatchObject({code:'BILLING_RUN_NOT_FOUND'});
});
it("retains and independently reviews zero usage without inventing an invoice or revenue journal",async()=>{
 const terms=businessContractSchema.parse({...input(),cycle:"usage",pricing:{kind:"flat",amount:"1.00"}}),c=await prepareBusinessContract(f.owner,terms,key());await decideBusinessContract(f.manager,String(c.id),{expectedVersion:1,reason:"Independent meter pricing and service-window review"},"approve",key());const request={expectedVersion:2,month:"2026-09",invoiceDate:"2026-09-30",dueDate:"2026-10-15",usageUnits:0,usageEvidence:"Verified opening and closing meter both show 1042 units"},command=key(),bill=await prepareBusinessBilling(f.owner,String(c.id),request,command);expect(bill).toMatchObject({invoiceId:null,status:"no_charge_draft",result:{subtotal:"0.00",tax:"0.00"}});expect(await prepareBusinessBilling(f.owner,String(c.id),request,command)).toEqual(bill);expect(await db.prepare("SELECT count(*) n FROM finance_invoices WHERE company_id=?").bind(f.company).first("n")).toBe(0);
 await expect(decideNoChargeBilling({...f.owner,role:"finance_approver"},String(bill.id),{expectedVersion:1,reason:"Cannot approve our own usage evidence"},"approve",key())).rejects.toMatchObject({code:"SELF_REVIEW_FORBIDDEN"});const review={expectedVersion:1,reason:"Independent review confirmed zero charge and original meter reading"},reviewKey=key();await decideNoChargeBilling(f.manager,String(bill.id),review,"approve",reviewKey);await decideNoChargeBilling(f.manager,String(bill.id),review,"approve",reviewKey);expect(await db.prepare("SELECT no_charge_status,version FROM finance_business_billing_runs WHERE id=?").bind(bill.id).first()).toMatchObject({no_charge_status:"approved",version:2});expect(await db.prepare("SELECT count(*) n FROM finance_journals WHERE company_id=?").bind(f.company).first("n")).toBe(0);await expect(db.prepare("UPDATE finance_business_billing_runs SET result_json='{}' WHERE id=?").bind(bill.id).run()).rejects.toThrow("BILLING_EVIDENCE_IMMUTABLE");
});
it("prepares due billing once under the current delegated assignment and leaves actual posting to independent review",async()=>{
 const c=await active(),input={expectedVersion:2,month:"2026-09",invoiceDate:"2026-09-16",dueDate:"2026-09-30"},command=key(),request={kind:"billing" as const,sourceId:c.id,runOn:"2026-09-16",input,reason:"Prepare retained September service evidence when due"},scheduled=await scheduleFinanceDraft(f.owner,request,command);expect(await scheduleFinanceDraft(f.owner,request,command)).toEqual(scheduled);
 await drainFinanceDueDrafts(db,f.owner.holdingId,new Date("2026-09-15T12:00:00Z"));expect(await db.prepare("SELECT status FROM finance_due_drafts WHERE id=?").bind(scheduled.id).first("status")).toBe("scheduled");
 await Promise.all([drainFinanceDueDrafts(db,f.owner.holdingId,new Date("2026-09-16T12:00:00Z")),drainFinanceDueDrafts(db,f.owner.holdingId,new Date("2026-09-16T12:00:00Z"))]);const row=await db.prepare("SELECT status,result_json FROM finance_due_drafts WHERE id=?").bind(scheduled.id).first<{status:string;result_json:string}>();expect(row?.status).toBe("prepared");const result=JSON.parse(row!.result_json);expect(await db.prepare("SELECT status,total_minor FROM finance_invoices WHERE id=?").bind(result.invoiceId).first()).toMatchObject({status:"draft",total_minor:5001});expect(await db.prepare("SELECT count(*) n FROM finance_business_billing_runs WHERE root_id=?").bind(c.id).first("n")).toBe(1);await expect(db.prepare("UPDATE finance_due_drafts SET input_json='{}' WHERE id=?").bind(scheduled.id).run()).rejects.toThrow("FINANCE_SCHEDULE_EVIDENCE_IMMUTABLE");
});
it("routes a changed delegated assignment to a retained exception and cancels without creating an invoice",async()=>{
 const c=await active(),r=await scheduleFinanceDraft(f.owner,{kind:"billing",sourceId:c.id,runOn:"2026-09-16",input:{expectedVersion:2,month:"2026-09",invoiceDate:"2026-09-16",dueDate:"2026-09-30"},reason:"Retain independent source version and delegated authority"},key());await db.prepare("UPDATE assignments SET version=version+1 WHERE id=?").bind(f.owner.assignmentId).run();await drainFinanceDueDrafts(db,f.owner.holdingId,new Date("2026-09-16T12:00:00Z"));expect(await db.prepare("SELECT status,last_code,version FROM finance_due_drafts WHERE id=?").bind(r.id).first()).toMatchObject({status:"blocked",last_code:"FINANCE_SCHEDULE_AUTHORITY_CHANGED",version:2});expect(await db.prepare("SELECT count(*) n FROM finance_business_billing_runs WHERE root_id=?").bind(c.id).first("n")).toBe(0);await decideFinanceDueDraft(f.owner,String(r.id),{expectedVersion:2,reason:"Cancel stale delegation and prepare a new reviewed schedule"},"cancel",key());await drainFinanceDueDrafts(db,f.owner.holdingId,new Date("2026-09-17T12:00:00Z"));expect(await db.prepare("SELECT status FROM finance_due_drafts WHERE id=?").bind(r.id).first("status")).toBe("cancelled");
 await expect(scheduleFinanceDraft({...f.owner,companyId:key()}, {kind:"billing",sourceId:c.id,runOn:"2026-09-16",input:{expectedVersion:2,month:"2026-09",invoiceDate:"2026-09-16",dueDate:"2026-09-30"},reason:"Reject another Company source"},key())).rejects.toMatchObject({code:"FINANCE_SCHEDULE_SOURCE_CHANGED"});
});
it("retains a closed-period recognition exception and retries its unchanged source input after an authorized reopen",async()=>{
 const c=await active(),b=await prepareBusinessBilling(f.owner,c.id,{expectedVersion:2,month:"2026-09",invoiceDate:"2026-09-16",dueDate:"2026-09-30"},key());await postInvoice(f.manager,String(b.invoiceId),{periodId:f.period,entryDate:"2026-09-16",expectedVersion:1},key());const r=await scheduleFinanceDraft(f.owner,{kind:"recognition",sourceId:String(b.id),runOn:"2026-09-30",input:{billingRunId:b.id,itemKey:"support:2026-09-30",periodId:f.period,entryDate:"2026-09-30",reason:"Prepare the retained September service allocation"},reason:"Due recognition with actual approved deferral"},key());await expect(beginClose(f.owner,f.period,key())).rejects.toMatchObject({code:"CLOSE_NOT_READY"});/* Isolated fault injection models a period state that changed outside this owner. */await db.prepare("UPDATE finance_periods SET state='closing',close_started_by=?,close_version=close_version+1 WHERE id=?").bind(f.owner.userId,f.period).run();const started={version:Number(await db.prepare("SELECT close_version FROM finance_periods WHERE id=?").bind(f.period).first("close_version"))};await drainFinanceDueDrafts(db,f.owner.holdingId,new Date("2026-09-30T12:00:00Z"));expect(await db.prepare("SELECT status,last_code,version FROM finance_due_drafts WHERE id=?").bind(r.id).first()).toMatchObject({status:"blocked",last_code:"PERIOD_NOT_OPEN",version:2});expect(await db.prepare("SELECT count(*) n FROM finance_recognition_runs WHERE billing_run_id=?").bind(b.id).first("n")).toBe(0);await cancelClose(f.owner,f.period,{expectedVersion:Number(started.version),reason:"Cancel the prepared close to resolve due revenue recognition first"},key());await decideFinanceDueDraft(f.owner,String(r.id),{expectedVersion:2,reason:"Period reopened normally; retry the original retained service evidence"},"retry",key());await drainFinanceDueDrafts(db,f.owner.holdingId,new Date("2026-09-30T12:00:00Z"));const prepared=await db.prepare("SELECT status,result_json FROM finance_due_drafts WHERE id=?").bind(r.id).first<{status:string;result_json:string}>();expect(prepared?.status).toBe("prepared");const result=JSON.parse(prepared!.result_json);expect(await db.prepare("SELECT status,amount_minor FROM finance_recognition_runs WHERE id=?").bind(result.id).first()).toMatchObject({status:"draft",amount_minor:3751});expect((await trialBalance(f.owner,undefined,f.chart)).find(a=>a.id===deferred)?.balance).toBe("-50.01");
});
it("rates calendar-day prorates, graduated tiers and SSP independently of the posting service", () => {
  const c = input(),
    r = contractBillingCalculation(c, {
      expectedVersion: 2,
      month: "2026-09",
      invoiceDate: "2026-09-16",
      dueDate: "2026-09-30",
    });
  // 15 / 30 days × 100.01, half-away rounding = 50.01; SSP 3:1.
  expect(r.subtotal).toBe("50.01");
  expect(r.allocations).toEqual([
    { key: "support", amount: "37.51" },
    { key: "delivery", amount: "12.50" },
  ]);
  const tier = businessContractSchema.parse({
    ...c,
    cycle: "usage",
    pricing: {
      kind: "tier",
      tiers: [
        { upTo: 10, unitPrice: "2.00" },
        { upTo: null, unitPrice: "1.00" },
      ],
    },
  });
  expect(
    contractBillingCalculation(tier, {
      expectedVersion: 2,
      month: "2026-09",
      invoiceDate: "2026-09-30",
      dueDate: "2026-10-15",
      usageUnits: 15,
      usageEvidence: "Meter readings 0 to 15 reviewed",
    }).subtotal,
  ).toBe("25.00");
  expect(() =>
    contractBillingCalculation(tier, {
      expectedVersion: 2,
      month: "2026-09",
      invoiceDate: "2026-09-30",
      dueDate: "2026-10-15",
      usageUnits: 15,
    }),
  ).toThrow("usage evidence");
});
it("retains approval, invoice, exact deferred balance, recognition and reversal without duplicate billing", async () => {
  const c = await active(),
    billInput = {
      expectedVersion: 2,
      month: "2026-09",
      invoiceDate: "2026-09-16",
      dueDate: "2026-09-30",
    },
    billKey = key(),
    bill = await prepareBusinessBilling(f.owner, c.id, billInput, billKey);
  expect(
    await prepareBusinessBilling(f.owner, c.id, billInput, billKey),
  ).toEqual(bill);
  await expect(
    prepareBusinessBilling(f.owner, c.id, billInput, key()),
  ).rejects.toMatchObject({ code: "BILLING_ALREADY_PREPARED" });
  await expect(
    createInvoice(
      f.owner,
      invoiceSchema.parse({
        partyId: party,
        kind: "receivable",
        invoiceNumber: "tamper",
        invoiceDate: "2026-09-16",
        dueDate: "2026-09-30",
        currency: "THB",
        subtotal: "999.00",
        debitAccountId: ar,
        creditAccountId: deferred,
        expectedVersion: 1,
      }),
      key(),
      String(bill.invoiceId),
    ),
  ).rejects.toMatchObject({ code: "BUSINESS_BILLING_OWNER_REQUIRED" });
  await expect(voidDraftInvoice(f.owner,String(bill.invoiceId),1,key())).rejects.toMatchObject({code:'BUSINESS_BILLING_OWNER_REQUIRED'});
  await postInvoice(
    f.manager,
    String(bill.invoiceId),
    { periodId: f.period, entryDate: "2026-09-16", expectedVersion: 1 },
    key(),
  );
  let tb = await trialBalance(f.owner, undefined, f.chart);
  expect(tb.find((r) => r.id === deferred)?.balance).toBe("-50.01");
  const r = await prepareRecognitionRun(
    f.owner,
    {
      billingRunId: String(bill.id),
      itemKey: "support:2026-09-30",
      periodId: f.period,
      entryDate: "2026-09-30",
      reason: "Service delivered for September",
    },
    key(),
  );
  const self = {
    ...f.owner,
    role: "finance_approver",
    assignmentId: await f.grant(f.owner, "finance_approver"),
  };
  await expect(
    decideRecognitionRun(
      self,
      String(r.id),
      { expectedVersion: 1, reason: "Self review refused" },
      "post",
      key(),
    ),
  ).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  const posted = await decideRecognitionRun(
    f.manager,
    String(r.id),
    { expectedVersion: 1, reason: "Delivery and retained allocation reviewed" },
    "post",
    key(),
  );
  tb = await trialBalance(f.owner, undefined, f.chart);
  expect(tb.find((r) => r.id === deferred)?.balance).toBe("-12.50");
  expect(tb.find((r) => r.id === income)?.balance).toBe("-37.51");
  await reverseRecognitionRun(
    f.manager,
    String(r.id),
    {
      expectedVersion: 2,
      periodId: f.period,
      entryDate: "2026-09-30",
      reason: "Correction retaining original evidence",
    },
    key(),
  );
  tb = await trialBalance(f.owner, undefined, f.chart);
  expect(tb.find((r) => r.id === deferred)?.balance).toBe("-50.01");
  expect(tb.find((r) => r.id === income)?.balance).toBe("0.00");
  const reversal=await db.prepare("SELECT reversal_journal_id FROM finance_recognition_runs WHERE id=?").bind(r.id).first('reversal_journal_id');
  await expect(reverseJournal(f.manager,String(reversal),key(),{periodId:f.period,entryDate:'2026-09-30',memo:'Cannot bypass the original recognition owner'})).rejects.toMatchObject({code:'REVERSE_AT_SOURCE'});
  expect(
    await db
      .prepare("SELECT status FROM finance_journals WHERE id=?")
      .bind(posted.journalId)
      .first("status"),
  ).toBe("reversed");
  expect(
    await db
      .prepare(
        "SELECT COUNT(*) n FROM finance_business_billing_runs WHERE invoice_id=?",
      )
      .bind(bill.invoiceId)
      .first("n"),
  ).toBe(1);
});
it("rejects self approval, stale sources, cross-Company access and billed-window amendments", async () => {
  const source = input(),
    r = await prepareBusinessContract(f.owner, source, key()),
    self = {
      ...f.owner,
      role: "finance_approver",
      assignmentId: await f.grant(f.owner, "finance_approver"),
    };
  await expect(
    decideBusinessContract(
      self,
      String(r.id),
      { expectedVersion: 1, reason: "Self review refused" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  await db
    .prepare("UPDATE finance_parties SET version=version+1 WHERE id=?")
    .bind(party)
    .run();
  await expect(
    decideBusinessContract(
      f.manager,
      String(r.id),
      { expectedVersion: 1, reason: "Stale source refused" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONTRACT_SOURCE_CHANGED" });
  await expect(
    decideBusinessContract(
      { ...f.manager, companyId: key() },
      String(r.id),
      { expectedVersion: 1, reason: "Cross company refused" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONTRACT_NOT_FOUND" });
  const c = await active();
  await prepareBusinessBilling(
    f.owner,
    c.id,
    {
      expectedVersion: 2,
      month: "2026-09",
      invoiceDate: "2026-09-16",
      dueDate: "2026-09-30",
    },
    key(),
  );
  await expect(
    prepareBusinessContract(
      f.owner,
      {
        ...c.source,
        parentId: c.id,
        expectedVersion: 2,
        reason: "Invalid backdated amendment",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONTRACT_BILLED_WINDOW" });
  const amendment = await prepareBusinessContract(
    f.owner,
    {
      ...c.source,
      parentId: c.id,
      expectedVersion: 2,
      startDate: "2026-10-01",
      endDate: "2026-11-30",
      reason: "Renewed service agreement",
    },
    key(),
  );
  await decideBusinessContract(
    f.manager,
    String(amendment.id),
    { expectedVersion: 1, reason: "Independent amendment review" },
    "approve",
    key(),
  );
  expect(
    await db
      .prepare("SELECT status FROM finance_business_contracts WHERE id=?")
      .bind(c.id)
      .first("status"),
  ).toBe("superseded");
  await decideBusinessContract(
    f.manager,
    String(amendment.id),
    { expectedVersion: 2, reason: "Customer requested cancellation" },
    "cancel",
    key(),
  );
  await expect(
    prepareBusinessBilling(
      f.owner,
      String(amendment.id),
      {
        expectedVersion: 3,
        month: "2026-10",
        invoiceDate: "2026-10-01",
        dueDate: "2026-10-31",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONTRACT_CHANGED" });
});
it("rolls back prepared billing invoice and receipts if scheduled completion fails, then retries its retained source exactly once",async()=>{
 const c=await active(),scheduled=await scheduleFinanceDraft(f.owner,{kind:"billing",sourceId:c.id,runOn:"2026-09-16",input:{expectedVersion:2,month:"2026-09",invoiceDate:"2026-09-16",dueDate:"2026-09-30"},reason:"Prepare original source under retained delegation"},key()),id=String(scheduled.id),command=`${id}:draft`,fault=`finance_due_fault_${crypto.randomUUID().replaceAll("-","")}`,now=new Date("2026-09-16T12:00:00Z");
 const retained=await db.prepare("SELECT input_json,input_hash,source_version,assignment_version FROM finance_due_drafts WHERE id=?").bind(id).first();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'FINANCE_DUE_COMPLETION_FAULT' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER UPDATE ON finance_due_drafts FOR EACH ROW WHEN (NEW.status='prepared') EXECUTE FUNCTION ${fault}();`);
 try{
  expect(await drainFinanceDueDrafts(db,f.owner.holdingId,now)).toBe(0);
  expect(await db.prepare("SELECT status,last_code,version,attempts,result_json FROM finance_due_drafts WHERE id=?").bind(id).first()).toMatchObject({status:"blocked",last_code:"FINANCE_SCHEDULE_RETRY",version:2,attempts:1,result_json:null});
  expect(Number(await db.prepare("SELECT count(*) n FROM finance_business_billing_runs WHERE root_id=?").bind(c.id).first("n"))).toBe(0);
  expect(Number(await db.prepare("SELECT count(*) n FROM finance_invoices WHERE company_id=?").bind(f.company).first("n"))).toBe(0);
  for(const table of ["command_receipts","audit_events","outbox_events"])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first("n"))).toBe(0);
  expect(await db.prepare("SELECT input_json,input_hash,source_version,assignment_version FROM finance_due_drafts WHERE id=?").bind(id).first()).toEqual(retained);
 }finally{await db.exec(`DROP TRIGGER ${fault} ON finance_due_drafts;DROP FUNCTION ${fault}();`);}
 await decideFinanceDueDraft(f.owner,id,{expectedVersion:2,reason:"Retry unchanged retained source after worker fault"},"retry",key());
 expect(await drainFinanceDueDrafts(db,f.owner.holdingId,now)).toBe(1);expect(await drainFinanceDueDrafts(db,f.owner.holdingId,now)).toBe(0);
 expect(Number(await db.prepare("SELECT count(*) n FROM finance_business_billing_runs WHERE root_id=?").bind(c.id).first("n"))).toBe(1);expect(Number(await db.prepare("SELECT count(*) n FROM finance_invoices WHERE company_id=?").bind(f.company).first("n"))).toBe(1);
 for(const table of ["command_receipts","audit_events","outbox_events"])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first("n"))).toBe(1);
 expect(await db.prepare("SELECT status,version,attempts,last_code FROM finance_due_drafts WHERE id=?").bind(id).first()).toEqual({status:"prepared",version:4,attempts:2,last_code:null});
 expect(await db.prepare("SELECT input_json,input_hash,source_version,assignment_version FROM finance_due_drafts WHERE id=?").bind(id).first()).toEqual(retained);
});
it("restores the retained invoice and billing edition on final revision outbox failure, then retries and replays once",async()=>{
 const c=await active(),original=await prepareBusinessBilling(f.owner,c.id,{expectedVersion:2,month:'2026-09',invoiceDate:'2026-09-16',dueDate:'2026-09-30'},key());
 const request={expectedVersion:1,invoiceExpectedVersion:1,billing:{month:'2026-09',invoiceDate:'2026-09-30',dueDate:'2026-10-15'},pricing:{kind:'flat' as const,amount:'80.01'},reason:'Retain the original bill while reviewing corrected service pricing'},command=key(),fault=`business_revision_fault_${crypto.randomUUID().replaceAll('-','')}`;
 const snapshots=async()=>Promise.all([
  db.prepare('SELECT * FROM finance_business_contracts WHERE id=?').bind(c.id).first(),
  db.prepare('SELECT * FROM finance_business_billing_runs WHERE root_id=? ORDER BY id').bind(c.id).all().then(rows=>rows.results),
  db.prepare('SELECT * FROM finance_invoices WHERE company_id=? ORDER BY id').bind(f.company).all().then(rows=>rows.results),
  db.prepare('SELECT * FROM finance_invoice_line_versions WHERE invoice_id IN(SELECT id FROM finance_invoices WHERE company_id=?) ORDER BY id').bind(f.company).all().then(rows=>rows.results),
  db.prepare('SELECT * FROM money_transaction_snapshots WHERE company_id=? ORDER BY source_type,source_id,amount_kind').bind(f.company).all().then(rows=>rows.results),
 ]),before=await snapshots();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'BUSINESS_REVISION_FINAL_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='finance.business-billing.revise') EXECUTE FUNCTION ${fault}();`);
 try{
  await expect(prepareBusinessBillingRevision(f.owner,String(original.id),request,command)).rejects.toThrow('BUSINESS_REVISION_FINAL_OUTBOX');
  expect(await snapshots()).toEqual(before);
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id IN (?,?)`).bind(command,`${command}-invoice`).first('n'))).toBe(0);
 }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
 const corrected=await prepareBusinessBillingRevision(f.owner,String(original.id),request,command);
 expect(await prepareBusinessBillingRevision(f.owner,String(original.id),request,command)).toEqual(corrected);
 expect(corrected).toMatchObject({result:{subtotal:'40.01'}});
 expect(await db.prepare('SELECT status,version FROM finance_invoices WHERE id=?').bind(original.invoiceId).first()).toEqual({status:'voided',version:2});
 expect(Number(await db.prepare('SELECT count(*) n FROM finance_business_billing_runs WHERE parent_run_id=?').bind(original.id).first('n'))).toBe(1);
 expect(Number(await db.prepare('SELECT count(*) n FROM finance_invoices WHERE company_id=?').bind(f.company).first('n'))).toBe(2);
 for(const table of ['command_receipts','audit_events','outbox_events'])for(const receipt of [command,`${command}-invoice`])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(receipt).first('n'))).toBe(1);
});
