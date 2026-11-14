import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env, type TestMigration } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { financeWorkbench, paymentObligationsWorkspace, paymentRecordsWorkspace } from "@erpx/server-finance/public/finance-workspace";
import {createInvoice,createParty,voidDraftInvoice} from '@erpx/server-finance/public/finance';
import {getInvoice} from '@erpx/server-finance/public/finance-invoice-read';
import { financePackageObligations } from "@erpx/server-finance/public/package-obligations";
import { ownerPackageObligations, readOwnerPackageObligations, withOwnerPackageObligations } from "@erpx/server-commercial/public/owner-package-obligations";
import { packageGrantStore, packageGrantRevision, readCompanyPackageGrants } from "@erpx/server-commercial/public/package-grants";
import type { CompanyPackageGrant } from "@erpx/server-commercial/public/packages";

vi.mock("server-only", () => ({}));
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }), runtimeEnv: async () => env }));

let f: Awaited<ReturnType<typeof expenseFixture>>;

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS as TestMigration[]);
});

beforeEach(async () => {
  f = await expenseFixture();
});
afterEach(() => vi.unstubAllEnvs());

const obligationInput = (companyId = f.company) => ({ companyId, packageId: "finance", packageVersion: 1, moduleId: "finance", now: Date.now() });
const observeFinance = (companyId = f.company) => withOwnerPackageObligations(db, obligationInput(companyId), tx => financePackageObligations(db, tx, obligationInput(companyId)));
const commercial = () => packageGrantStore(db, {
  // Only the authority port is a disposable fixture. Sources and Commercial
  // Preview/Apply, versions, idempotency, retained rights and audit are real.
  authorize: async () => ({ userId: "isolated-finance-lifecycle-review" }),
  obligations: (tx, companyId, packageId) => ownerPackageObligations(db, tx, companyId, packageId),
});
const change = (state: CompanyPackageGrant["state"], version: number) => ({ expectedRevision: null as string | null, expectedVersion: version - 1, idempotencyKey: key(), grant: {
  companyId: f.company, packageId: "finance", packageVersion: 1, state, startsAt: 0, endsAt: null, version,
  reason: "Reviewed Finance settlement and retained history", evidence: "test:finance-owner-lifecycle",
} });
async function lifecycle(request: ReturnType<typeof change>, apply = false) {
  request.expectedRevision ??= packageGrantRevision(f.company, await readCompanyPackageGrants(db, f.company));
  const batch = { companyId: f.company, expectedRevision: request.expectedRevision,
    idempotencyKey: request.idempotencyKey, changes: [{ grant: request.grant, expectedVersion: request.expectedVersion }] };
  return withOwnerPackageObligations(db, obligationInput(), tx => apply ? commercial().batch.applyIn(tx, batch, async () => {}) : commercial().batch.previewIn(tx, batch));
}
async function closeFinance() {
  // Native period-close owns this transition; seed the persisted closed state.
  await db.prepare("UPDATE finance_periods SET state='closed',closed_by=?,closed_at=?,close_version=close_version+1 WHERE id=? AND company_id=? AND state='open'").bind(f.manager.userId,Date.now(),f.period,f.company).run();
}
async function draftInvoice() {
  const supplier = await createParty(f.reviewer, { kind: "supplier", code: key(), name: "Private lifecycle supplier", currency: "THB", paymentTermsDays: 30, creditLimit: "0" }, key());
  return createInvoice(f.reviewer, { partyId: String(supplier.id), kind: "payable", invoiceNumber: key(), invoiceDate: "2026-09-10", dueDate: "2026-10-10", currency: "THB", subtotal: "10", tax: "0", debitAccountId: f.accounts.expense, creditAccountId: f.accounts.payable }, key());
}

it("Finance lifecycle requires its exact fresh source lease and includes existing work without a purchase or personal queue", async () => {
  await expect(financePackageObligations(db, db, obligationInput())).rejects.toThrow("TRANSACTION_NOT_ACTIVE_OR_OWNED");
  await db.transaction(async tx => { await expect(financePackageObligations(db, tx, obligationInput())).rejects.toThrow("FINANCE_PACKAGE_OBLIGATION_SOURCE_REQUIRED"); });
  const first = await observeFinance(); expect(first.outstanding).toBe(1); // the real, open Company period
  const invoice = await draftInvoice(), second = await observeFinance(); expect(second.outstanding).toBe(2); expect(second.revision).not.toBe(first.revision);
  expect(JSON.stringify(second)).not.toContain("Private lifecycle supplier");
  const other = await expenseFixture(); expect((await observeFinance(other.company)).outstanding).toBe(1);
  await withOwnerPackageObligations(db, obligationInput(), async tx => {
    await expect(financePackageObligations(db, tx, obligationInput(other.company))).rejects.toThrow("FINANCE_PACKAGE_OBLIGATION_SOURCE_REQUIRED");
    await expect(financePackageObligations(db, tx, { ...obligationInput(), packageVersion: 2 })).rejects.toThrow("FINANCE_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
    expect((await readOwnerPackageObligations(db, tx, obligationInput())).total).toBe(2);
  });
  expect(await db.prepare("SELECT status FROM finance_invoices WHERE id=?").bind(invoice.id).first("status")).toBe("draft");
});

it("Finance lifecycle settles a real draft and independently closes books before Preview/Apply, replay and retained history", async () => {
  await commercial().apply(change("active", 1)); vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT", "enforced");
  const invoice = await draftInvoice(); await commercial().apply(change("draining", 2));
  const inactive = change("inactive", 3), before = await db.prepare("SELECT count(*) n FROM company_package_grant_events WHERE company_id=?").bind(f.company).first("n");
  await expect(lifecycle(inactive)).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN"); await expect(lifecycle(inactive, true)).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");
  await voidDraftInvoice(f.reviewer, String(invoice.id), 1, key()); expect((await observeFinance()).outstanding).toBe(1);
  await closeFinance(); expect((await observeFinance()).outstanding).toBe(0);
  const retained = await db.prepare("SELECT * FROM finance_invoices WHERE id=?").bind(invoice.id).first();
  expect((await lifecycle(inactive)).after[0]).toMatchObject({ state: "inactive", version: 3 });
  expect(await db.prepare("SELECT count(*) n FROM company_package_grant_events WHERE company_id=?").bind(f.company).first("n")).toBe(before);
  const result = await lifecycle(inactive, true); expect(await lifecycle(inactive, true)).toEqual(result);
  expect(await db.prepare("SELECT * FROM finance_invoices WHERE id=?").bind(invoice.id).first()).toEqual(retained);
  expect((await getInvoice(f.owner, String(invoice.id))).invoice).toMatchObject({ id: invoice.id, status: "voided" });
  await expect(draftInvoice()).rejects.toMatchObject({ code: "PACKAGE_INACTIVE" });
  expect(await db.prepare("SELECT count(*) n FROM company_package_change_sets WHERE id=?").bind(inactive.idempotencyKey).first("n")).toBe(1);
});

it("Finance lifecycle fences absent money, statutory, job and group rows until its owner transaction ends", async () => {
  await withOwnerPackageObligations(db, obligationInput(), async tx => {
    expect((await financePackageObligations(db, tx, obligationInput())).outstanding).toBe(1);
    for (const [table, column] of [["finance_invoices", "version"], ["finance_bank_transactions", "version"], ["finance_supplier_instructions", "version"], ["finance_tax_entries", "version"], ["finance_withholding_remittances", "version"], ["finance_due_drafts", "version"], ["finance_instrument_editions", "version"], ["finance_consolidation_runs", "version"], ["finance_ai_usage", "lease_until"]]) {
      await expect(db.transaction(async competing => {
        await competing.prepare("SET LOCAL lock_timeout='75ms'").run();
        await competing.prepare(`UPDATE ${table} SET ${column}=${column} WHERE false`).run();
      })).rejects.toMatchObject({ code: "55P03" });
    }
  });
});

it("Finance lifecycle rolls a late deferred reopening and all withdrawal receipts back before commit", async () => {
  await commercial().apply(change("active", 1)); await closeFinance(); await commercial().apply(change("draining", 2));
  expect((await observeFinance()).outstanding).toBe(0);
  const fault = `finance_settlement_${key().replaceAll("-", "").slice(0, 12)}`, inactive = change("inactive", 3);
  expect(f.company).toMatch(/^[a-z0-9-]+$/); expect(f.period).toMatch(/^[a-z0-9-]+$/);
  const period = await db.prepare("SELECT * FROM finance_periods WHERE id=?").bind(f.period).first();
  await db.exec(`CREATE SEQUENCE ${fault}_attempt; CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}_attempt'); UPDATE finance_periods SET state='open',close_version=close_version+1 WHERE id='${f.period}';RETURN NEW;END $$;
    CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON company_package_change_sets DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.company_id='${f.company}') EXECUTE FUNCTION ${fault}();`);
  try {
    await expect(lifecycle(inactive, true)).rejects.toThrow("PACKAGE_OBLIGATIONS_REMAIN");
    expect(await db.prepare(`SELECT is_called FROM ${fault}_attempt`).first("is_called")).toBe(true);
    expect(await db.prepare("SELECT * FROM finance_periods WHERE id=?").bind(f.period).first()).toEqual(period);
    expect(await db.prepare("SELECT state,version FROM company_package_grants WHERE company_id=?").bind(f.company).first()).toEqual({ state: "draining", version: 2 });
    expect(await db.prepare("SELECT 1 FROM company_package_change_sets WHERE id=?").bind(inactive.idempotencyKey).first()).toBeNull();
    expect(await db.prepare("SELECT 1 FROM company_package_grant_events WHERE id=?").bind(inactive.idempotencyKey).first()).toBeNull();
  } finally { await db.exec(`DROP TRIGGER IF EXISTS ${fault} ON company_package_change_sets;DROP FUNCTION IF EXISTS ${fault}();DROP SEQUENCE IF EXISTS ${fault}_attempt;`); }
  await lifecycle(inactive, true);
});

it("finds settled payment evidence beyond the action queue with scoped, currency-preserving pagination",async()=>{
  const other=await expenseFixture(),prefix=`History-${key()}`;
  for(const [index,company,currency] of [[1,f.company,"THB"],[2,f.company,"USD"],[3,f.company,"THB"],[4,other.company,"USD"]] as const){
    await db.prepare("INSERT INTO finance_payments(id,company_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,status,created_by,created_at,updated_at) VALUES(?,?,'receipt',?,'2026-09-25',?,10000,0,'settled',?,1,1)")
      .bind(`${prefix}-${index}`,company,`${prefix}-${index}`,currency,f.owner.userId).run();
  }
  const first=await paymentRecordsWorkspace(f.owner,{q:prefix,paymentStatus:"settled"},2);
  expect(first.items.map(item=>item.id)).toEqual([`${prefix}-3`,`${prefix}-2`]);
  expect(first.items.map(item=>item.currency)).toEqual(["THB","USD"]);
  const second=await paymentRecordsWorkspace(f.owner,{q:prefix,paymentStatus:"settled",paymentCursor:first.nextCursor!},2);
  expect(second.items.map(item=>item.id)).toEqual([`${prefix}-1`]);
  expect(second.nextCursor).toBeNull();
  await expect(paymentRecordsWorkspace(other.owner,{q:prefix,paymentStatus:"settled",paymentCursor:first.nextCursor!},2)).rejects.toMatchObject({code:"INVALID_CURSOR"});
  await expect(paymentRecordsWorkspace({...f.owner,role:"employee"},{q:prefix})).rejects.toMatchObject({status:403});
});

it("combines payment sources into one finance queue while redacting people payments from preparers", async () => {
  const genericId = `payment-${key()}`;
  const payrollBatchId = `payroll-batch-${key()}`;
  await db.batch([
    db.prepare("INSERT INTO finance_payments(id,company_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,status,created_by,created_at,updated_at) VALUES(?,?, 'payment',?,'2026-09-25','THB',10000,10000,'approved',?,?,?)").bind(genericId, f.company, `PAY-${key()}`, f.reviewer.userId, 1, 1),
    db.prepare("INSERT INTO hr_payment_batches(id,company_id,source_type,source_id,currency,status,total_minor,created_by,created_at,updated_at) VALUES(?,?, 'payroll',?,'THB','draft',20000,?,?,?)").bind(payrollBatchId, f.company, `payroll-run-${key()}`, f.reviewer.userId, 1, 1),
    db.prepare("INSERT INTO hr_payment_batch_items(id,batch_id,reference,payee_name,amount_minor,currency) VALUES(?,?,?,'Demo employee',20000,'THB')").bind(`payroll-item-${key()}`, payrollBatchId, `PAYEE-${key()}`),
  ]);

  const finance = await paymentObligationsWorkspace(f.owner);
  expect(finance).toEqual(expect.arrayContaining([
    expect.objectContaining({ id: genericId, sourceType: "generic_payment", sourceLabel: "Supplier payment", amountMinor: 10000, nextAction: "Release through Treasury" }),
    expect.objectContaining({ id: payrollBatchId, sourceType: "people_payment", sourceLabel: "Payroll payment batch", amountMinor: 20000, href: expect.stringContaining("people/payroll/") }),
  ]));

  const preparer = { ...f.reviewer, role: "finance_preparer" };
  const restricted = await paymentObligationsWorkspace(preparer);
  expect(restricted.some(item => item.id === payrollBatchId)).toBe(false);
  expect(restricted.some(item => item.id === genericId)).toBe(true);
  const workbench = await financeWorkbench(f.owner);
  expect(workbench.attention.find(item => item.id === "payment-review")?.count).toBeGreaterThanOrEqual(2);
  expect((await paymentObligationsWorkspace(f.owner, { sourceType: "generic_payment" })).map(item => item.id)).toEqual([genericId]);
  expect((await paymentObligationsWorkspace(f.owner, { status: "approved" })).map(item => item.id)).toContain(genericId);
});

it("keeps overdue queue amounts in their transaction currencies across Company books",async()=>{
  const party=key();
  await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Mixed currency customer','THB',1,1)").bind(party,f.company,party).run();
  for(const [currency,amount] of [["THB",10000],["USD",20000]] as const){
    const id=key();await db.prepare("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,status,created_by,created_at,updated_at) VALUES(?,?,?,'receivable',?,'2026-01-01','2026-01-01',?,?,?,?,?,?,'posted',?,1,1)").bind(id,f.company,party,id,currency,amount,amount,amount,f.accounts.cash,f.accounts.revenue,f.reviewer.userId).run();
  }
  const queue=(await financeWorkbench(f.owner)).attention.find(item=>item.id==="receivable-overdue");
  expect(queue).toMatchObject({count:2,currency:null,amountMinor:null,amounts:[{currency:"THB",amountMinor:10000},{currency:"USD",amountMinor:20000}]});
});

it('Finance lifecycle retains a real unmatched statement row, then accepts independently settled and closed bank history without inventing a statement status',async()=>{
 const {createAccount,postInvoice,createPayment,approvePayment,releasePayment,confirmPayment,importBankStatement,calculateCloseReconciliations}=await import('@erpx/server-finance/public/finance'),{assignFlowPrimary,resolveFlow}=await import('@erpx/server-workflow/public/flow-routing');
 await commercial().apply(change('active',1));vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
 const bank=await createAccount(f.reviewer,{chartId:f.chart,code:`BNK-${key().slice(0,8)}`,name:'Original native lifecycle bank account',category:'asset',currency:'THB',isControl:true,controlKind:'bank'},key()),bankId=String(bank.id),supplier=await createParty(f.reviewer,{kind:'supplier',code:key(),name:'Original bank lifecycle supplier',currency:'THB',paymentTermsDays:30,creditLimit:'0'},key()),invoice=await createInvoice(f.reviewer,{partyId:String(supplier.id),kind:'payable',invoiceNumber:key(),invoiceDate:'2026-09-10',dueDate:'2026-09-30',currency:'THB',subtotal:'15',tax:'0',debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable},key()),invoiceId=String(invoice.id),flow=await resolveFlow(db,f.owner,'invoice',invoiceId);
 await assignFlowPrimary(db,f.owner,'invoice',invoiceId,{stepKey:'draft',userId:f.manager.userId,reason:'Independent normal review of the original supplier liability',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await postInvoice(f.manager,invoiceId,{periodId:f.period,entryDate:'2026-09-10',expectedVersion:1},key());
 const firstReference=key(),lastReference=key(),statementKey=key(),statement=await importBankStatement(f.reviewer,{bankAccount:bankId,statementNumber:key(),statementDate:'2026-12-31',opening:'0',closing:'-15',sourceHash:key(),rows:[{externalId:firstReference,transactionDate:'2026-09-12',amount:'-10',currency:'THB',description:'Actual first supplier settlement'},{externalId:lastReference,transactionDate:'2026-09-13',amount:'-5',currency:'THB',description:'Actual final supplier settlement'}]},statementKey),statementId=String(statement.id);
 await commercial().apply(change('draining',2));
 const settle=async(amount:string,date:string,reference:string)=>{
  const payment=await createPayment(f.reviewer,{partyId:String(supplier.id),kind:'payment',paymentNumber:key(),paymentDate:date,currency:'THB',amount,bankAccount:bankId,settlementInvoiceId:invoiceId},key()),id=String(payment.id);await approvePayment(f.manager,id,{expectedVersion:1,proof:'Independent review of the original invoice and exact settlement amount'},key());await releasePayment(f.payer,id,{expectedVersion:2,proof:'Independent Treasury review of beneficiary and original bank evidence'},key());
  const row=await db.prepare('SELECT id,version FROM finance_bank_transactions WHERE company_id=? AND statement_id=? AND external_id=?').bind(f.company,statementId,reference).first<{id:string;version:number}>();if(!row)throw Error('Actual imported bank movement missing');const command=key(),body={expectedVersion:3,periodId:f.period,bankTransactionId:row.id,bankVersion:row.version,bankReference:reference,proof:'Original signed bank debit matched to independently approved invoice payment',allocations:[{invoiceId,amount}]},result=await confirmPayment(f.payer,id,body,command);expect(await confirmPayment(f.payer,id,body,command)).toEqual(result);return {id,command,result};
 };
 const first=await settle('10','2026-09-12',firstReference);expect(await db.prepare('SELECT status,outstanding_minor FROM finance_invoices WHERE id=?').bind(invoiceId).first()).toEqual({status:'partially_paid',outstanding_minor:500});expect(await db.prepare("SELECT count(*)::int n FROM finance_bank_transactions WHERE statement_id=? AND match_status='unmatched'").bind(statementId).first('n')).toBe(1);const inactive=change('inactive',3);await expect(lifecycle(inactive)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 const last=await settle('5','2026-09-13',lastReference);expect(await db.prepare('SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?').bind(invoiceId).first()).toEqual({status:'paid',outstanding_minor:0,base_outstanding_minor:0});expect(await db.prepare("SELECT count(*)::int n FROM finance_bank_transactions WHERE statement_id=? AND match_status='matched' AND matched_type='payment' AND matched_id IN (?,?)").bind(statementId,first.id,last.id).first('n')).toBe(2);
 const period=await db.prepare('SELECT * FROM finance_periods WHERE id=?').bind(f.period).first<Record<string,string|number|null>>();if(!period)throw Error('Original accounting period missing');const reconciliation=await calculateCloseReconciliations(db,f.reviewer,period);expect(reconciliation.ready).toBe(true);expect(reconciliation.rows.find(row=>row.accountId===bankId)).toMatchObject({kind:'bank',ledgerMinor:-1500,sourceMinor:-1500,differenceMinor:0,source:{statementId,evidenceComplete:true}});await closeFinance();
 expect(await db.prepare('SELECT status FROM finance_bank_statements WHERE id=?').bind(statementId).first('status')).toBe('imported');const retained=await db.prepare('SELECT to_jsonb(s)::text image FROM finance_bank_statements s WHERE id=?').bind(statementId).first('image');
 expect((await observeFinance()).outstanding).toBe(0);
 expect((await lifecycle(inactive)).after[0]).toMatchObject({state:'inactive',version:3});const applied=await lifecycle(inactive,true);expect(await lifecycle(inactive,true)).toEqual(applied);expect(await db.prepare('SELECT to_jsonb(s)::text image FROM finance_bank_statements s WHERE id=?').bind(statementId).first('image')).toBe(retained);expect(await db.prepare('SELECT command_id FROM command_receipts WHERE command_id=?').bind(statementKey).first('command_id')).toBe(statementKey);
});

// Retained shared Finance/Recruit source-lease regression: no Field commands or
// Field fixture authority. Keep until those owners cut over; all original
// committed-source, debt and zero-hidden-retry assertions remain unchanged.
it('Service no-charge raw Finance source leases remain query-free until every later package owner is locked',async()=>{
 const {withOwnerPackageObligations,ownerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics');for(const packageId of ["finance","recruit"]){const current=(await readCompanyPackageGrants(db,f.company)).find(row=>row.packageId===packageId);await packageGrantStore(db,{authorize:async()=>({userId:"isolated-provider-service-reviewer"}),obligations:async()=>{throw Error("Concrete owner lifecycle sources remain required");}}).apply({grant:{companyId:f.company,packageId,packageVersion:1,state:"active",startsAt:0,endsAt:null,version:(current?.version??0)+1,reason:"Native Service package-only review",evidence:"test:normal-commercial-apply"},expectedVersion:current?.version??0,idempotencyKey:key()});}const actor={...f.owner,branchId:f.branch,departmentId:f.department,modules:['talent']},candidateId=`talent-candidate-${key()}`,talentNow=Date.now();await db.batch([db.prepare("INSERT INTO talent_candidates(id,company_id,legal_name,email,phone,consent_evidence,consented_at,retention_until,status,created_by,version,created_at,updated_at) VALUES(?,?,?,?,NULL,?,?,?,'active',?,1,?,?)").bind(candidateId,actor.companyId,'Actual later package source',`candidate-${key()}@example.test`,'Original consent for the isolated source lease proof',talentNow,'2027-09-30',actor.userId,talentNow,talentNow),db.prepare("INSERT INTO talent_candidate_events(id,candidate_id,version,action,actor_id,reason,status,created_at) VALUES(?,?,1,'consent',?,'Candidate consent recorded','active',?)").bind(`talent-candidate-event-${candidateId}`,candidateId,actor.userId,talentNow)]);const candidate={id:candidateId};
 const measured=await measureReads(()=>committedSourceVisibility(db,tx=>tx.prepare("UPDATE talent_candidates SET retention_until='2028-09-30',version=version+1 WHERE id=?").bind(candidate.id).run(),()=>withOwnerPackageObligations(db,[{companyId:f.company,packageId:'finance',packageVersion:1},{companyId:f.company,packageId:'recruit',packageVersion:1}],async tx=>({retention:await tx.prepare('SELECT retention_until FROM talent_candidates WHERE id=?').bind(candidate.id).first('retention_until'),facts:await ownerPackageObligations(db,tx,f.company,'recruit')}))));expect(measured.result.retention).toBe('2028-09-30');expect(measured.result.facts).toBe(1);for(const metric of ['deadlockFailures','serializationFailures','transactionRetries'] as const)expect(measured.metrics[metric]??0).toBe(0);
});
