import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env, type TestMigration } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { financeWorkbench, paymentObligationsWorkspace, paymentRecordsWorkspace } from "@erpx/server-finance/public/finance-workspace";
import { beginClose, closePeriod, createInvoice, createParty, getInvoice, voidDraftInvoice } from "@erpx/server-finance/public/finance";
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
  const prepared = await beginClose(f.reviewer, f.period, key());
  await closePeriod(f.manager, f.period, { expectedVersion: Number(prepared.version), proof: "Independent source and bank reconciliation review" }, key());
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
 const first=await settle('10','2026-09-12',firstReference);expect(await db.prepare('SELECT status,outstanding_minor FROM finance_invoices WHERE id=?').bind(invoiceId).first()).toEqual({status:'partially_paid',outstanding_minor:500});expect(await db.prepare("SELECT count(*)::int n FROM finance_bank_transactions WHERE statement_id=? AND match_status='unmatched'").bind(statementId).first('n')).toBe(1);await expect(closeFinance()).rejects.toMatchObject({code:'CLOSE_NOT_READY'});const inactive=change('inactive',3);await expect(lifecycle(inactive)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 const last=await settle('5','2026-09-13',lastReference);expect(await db.prepare('SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?').bind(invoiceId).first()).toEqual({status:'paid',outstanding_minor:0,base_outstanding_minor:0});expect(await db.prepare("SELECT count(*)::int n FROM finance_bank_transactions WHERE statement_id=? AND match_status='matched' AND matched_type='payment' AND matched_id IN (?,?)").bind(statementId,first.id,last.id).first('n')).toBe(2);
 const period=await db.prepare('SELECT * FROM finance_periods WHERE id=?').bind(f.period).first<Record<string,string|number|null>>();if(!period)throw Error('Original accounting period missing');const reconciliation=await calculateCloseReconciliations(db,f.reviewer,period);expect(reconciliation.ready).toBe(true);expect(reconciliation.rows.find(row=>row.accountId===bankId)).toMatchObject({kind:'bank',ledgerMinor:-1500,sourceMinor:-1500,differenceMinor:0,source:{statementId,evidenceComplete:true}});await closeFinance();
 expect(await db.prepare('SELECT status FROM finance_bank_statements WHERE id=?').bind(statementId).first('status')).toBe('imported');const retained=await db.prepare('SELECT to_jsonb(s)::text image FROM finance_bank_statements s WHERE id=?').bind(statementId).first('image');
 expect((await observeFinance()).outstanding).toBe(0);
 expect((await lifecycle(inactive)).after[0]).toMatchObject({state:'inactive',version:3});const applied=await lifecycle(inactive,true);expect(await lifecycle(inactive,true)).toEqual(applied);expect(await db.prepare('SELECT to_jsonb(s)::text image FROM finance_bank_statements s WHERE id=?').bind(statementId).first('image')).toBe(retained);expect(await db.prepare('SELECT command_id FROM command_receipts WHERE command_id=?').bind(statementKey).first('command_id')).toBe(statementKey);
});

it('Finance lifecycle retains unresolved supplier instructions and accepts their normal independent bank confirmations as statement history',async()=>{
 const {importAccounts,postInvoice,importBankStatement,calculateCloseReconciliations}=await import('@erpx/server-finance/public/finance'),{assignFlowPrimary,resolveFlow}=await import('@erpx/server-workflow/public/flow-routing'),{createSupplierBatch,decideSupplierBatch,exportSupplierBatch,recoverSupplierExport,confirmSupplierInstruction,supplierBatchWorkspace}=await import('@erpx/server-procurement/public/supplier-payments');
 await commercial().apply(change('active',1));vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
 const bankId=f.accounts.cash,configured=await db.prepare('SELECT code,name FROM finance_accounts WHERE id=? AND company_id=?').bind(bankId,f.company).first<{code:string;name:string}>();if(!configured)throw Error('Original configured bank ledger missing');await importAccounts(f.reviewer,f.chart,[{code:configured.code,name:configured.name,category:'asset',currency:'THB',controlKind:'bank'}],key());const supplier=await createParty(f.reviewer,{kind:'supplier',code:key(),name:'Original independently paid supplier',currency:'THB',paymentTermsDays:30,creditLimit:'0'},key());
 const payable=async(amount:string)=>{const invoice=await createInvoice(f.reviewer,{partyId:String(supplier.id),kind:'payable',invoiceNumber:key(),invoiceDate:'2026-09-10',dueDate:'2026-09-30',currency:'THB',subtotal:amount,tax:'0',debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable},key()),id=String(invoice.id),flow=await resolveFlow(db,f.owner,'invoice',id);await assignFlowPrimary(db,f.owner,'invoice',id,{stepKey:'draft',userId:f.manager.userId,reason:'Independent review of the original supplier liability',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await postInvoice(f.manager,id,{periodId:f.period,entryDate:'2026-09-10',expectedVersion:1},key());return id;};
 const firstInvoice=await payable('10'),lastInvoice=await payable('5'),firstReference=key(),lastReference=key(),statementKey=key(),statement=await importBankStatement(f.reviewer,{bankAccount:bankId,statementNumber:key(),statementDate:'2026-12-31',opening:'0',closing:'-15',sourceHash:key(),rows:[{externalId:firstReference,transactionDate:'2026-09-12',amount:'-10',currency:'THB',description:'Original supplier batch first debit'},{externalId:lastReference,transactionDate:'2026-09-13',amount:'-5',currency:'THB',description:'Original supplier batch final debit'}]},statementKey),statementId=String(statement.id);
 await commercial().apply(change('draining',2));
 const batchKey=key(),body={bankAccountId:bankId,plannedDate:'2026-09-12',items:[{invoiceId:firstInvoice,expectedVersion:2,amount:'10',beneficiaryName:'Original supplier',beneficiaryAccount:'ISOLATED-SUPPLIER-001'},{invoiceId:lastInvoice,expectedVersion:2,amount:'5',beneficiaryName:'Original supplier',beneficiaryAccount:'ISOLATED-SUPPLIER-001'}]},draft=await createSupplierBatch(f.reviewer,body,batchKey),batchId=String(draft.id);expect(await createSupplierBatch(f.reviewer,body,batchKey)).toEqual(draft);
 await decideSupplierBatch(f.manager,batchId,{expectedVersion:1,reason:'Independent review of both exact invoices and beneficiary'},key(),'approve');const exportKey=key(),exportBody={expectedVersion:2},exported=await exportSupplierBatch(f.payer,batchId,exportBody,exportKey);expect(await exportSupplierBatch(f.payer,batchId,exportBody,exportKey)).toEqual(exported);expect(await recoverSupplierExport(f.payer,batchId)).toMatchObject({content:exported.content,sha256:exported.sha256});
 const workspace=await supplierBatchWorkspace(f.payer,batchId),confirm=async(invoiceId:string,reference:string)=>{const item=workspace.items.find(row=>row.invoice_id===invoiceId),movement=await db.prepare('SELECT id,version FROM finance_bank_transactions WHERE statement_id=? AND company_id=? AND external_id=?').bind(statementId,f.company,reference).first<{id:string;version:number}>();if(!item||!movement)throw Error('Original supplier instruction or bank movement missing');const command=key(),input={expectedVersion:Number(item.version),bankTransactionId:movement.id,bankVersion:movement.version,periodId:f.period,bankReference:reference,evidence:'Original bank debit and independently approved beneficiary checked'},result=await confirmSupplierInstruction(f.payer,String(item.id),input,command);expect(await confirmSupplierInstruction(f.payer,String(item.id),input,command)).toEqual(result);return result;};
 const first=await confirm(firstInvoice,firstReference);expect(await db.prepare("SELECT count(*)::int n FROM finance_supplier_instructions WHERE batch_id=? AND status='exported'").bind(batchId).first('n')).toBe(1);expect(await db.prepare("SELECT count(*)::int n FROM finance_bank_transactions WHERE statement_id=? AND match_status='unmatched'").bind(statementId).first('n')).toBe(1);expect(await db.prepare('SELECT status,outstanding_minor FROM finance_invoices WHERE id=?').bind(lastInvoice).first()).toEqual({status:'posted',outstanding_minor:500});await expect(closeFinance()).rejects.toMatchObject({code:'CLOSE_NOT_READY'});const inactive=change('inactive',3);await expect(lifecycle(inactive)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 const last=await confirm(lastInvoice,lastReference);for(const id of [firstInvoice,lastInvoice])expect(await db.prepare('SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?').bind(id).first()).toEqual({status:'paid',outstanding_minor:0,base_outstanding_minor:0});expect(await db.prepare("SELECT count(*)::int n FROM finance_bank_transactions WHERE statement_id=? AND match_status='matched' AND matched_type='payment' AND matched_id IN (?,?)").bind(statementId,first.paymentId,last.paymentId).first('n')).toBe(2);
 const period=await db.prepare('SELECT * FROM finance_periods WHERE id=?').bind(f.period).first<Record<string,string|number|null>>();if(!period)throw Error('Original accounting period missing');const reconciliation=await calculateCloseReconciliations(db,f.reviewer,period);expect(reconciliation.ready).toBe(true);expect(reconciliation.rows.find(row=>row.accountId===bankId)).toMatchObject({kind:'bank',ledgerMinor:-1500,sourceMinor:-1500,differenceMinor:0,source:{statementId,evidenceComplete:true}});await closeFinance();
 const retained=await db.prepare('SELECT to_jsonb(s)::text image FROM finance_bank_statements s WHERE id=?').bind(statementId).first('image'),retainedBatch=await supplierBatchWorkspace(f.payer,batchId);expect(await db.prepare('SELECT status FROM finance_bank_statements WHERE id=?').bind(statementId).first('status')).toBe('imported');expect(retainedBatch.batch).toMatchObject({status:'exported'});expect(retainedBatch.items.every(item=>item.status==='confirmed')).toBe(true);
 await withOwnerPackageObligations(db,obligationInput(),async()=>{for(const [table,column] of [['finance_allocations','amount_minor'],['finance_financial_events','source_version']] as const)await expect(db.transaction(async writer=>{await writer.prepare("SET LOCAL lock_timeout='75ms'").run();await writer.prepare(`UPDATE ${table} SET ${column}=${column} WHERE false`).run();})).rejects.toMatchObject({code:'55P03'});});
 expect((await observeFinance()).outstanding).toBe(0);
 expect((await lifecycle(inactive)).after[0]).toMatchObject({state:'inactive',version:3});const applied=await lifecycle(inactive,true);expect(await lifecycle(inactive,true)).toEqual(applied);expect(await db.prepare('SELECT to_jsonb(s)::text image FROM finance_bank_statements s WHERE id=?').bind(statementId).first('image')).toBe(retained);expect(await supplierBatchWorkspace(f.payer,batchId)).toEqual(retainedBatch);expect(await recoverSupplierExport(f.payer,batchId)).toMatchObject({content:exported.content,sha256:exported.sha256});expect(await db.prepare('SELECT command_id FROM command_receipts WHERE command_id=?').bind(statementKey).first('command_id')).toBe(statementKey);
});

it('Finance lifecycle retains uncertain supplier cash, then preserves an independently verified unpaid instruction after its replacement settles the original invoice',async()=>{
 const {importAccounts,postInvoice,importBankStatement,calculateCloseReconciliations}=await import('@erpx/server-finance/public/finance'),{assignFlowPrimary,resolveFlow}=await import('@erpx/server-workflow/public/flow-routing'),{createSupplierBatch,decideSupplierBatch,exportSupplierBatch,recordSupplierResult,confirmSupplierInstruction,supplierBatchWorkspace,supplierBatchWork}=await import('@erpx/server-procurement/public/supplier-payments');
 await commercial().apply(change('active',1));vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
 const bankId=f.accounts.cash,configured=await db.prepare('SELECT code,name FROM finance_accounts WHERE id=? AND company_id=?').bind(bankId,f.company).first<{code:string;name:string}>();if(!configured)throw Error('Original configured supplier bank missing');await importAccounts(f.reviewer,f.chart,[{code:configured.code,name:configured.name,category:'asset',currency:'THB',controlKind:'bank'}],key());
 const supplier=await createParty(f.reviewer,{kind:'supplier',code:key(),name:'Original independently reviewed retry supplier',currency:'THB',paymentTermsDays:30,creditLimit:'0'},key()),invoice=await createInvoice(f.reviewer,{partyId:String(supplier.id),kind:'payable',invoiceNumber:key(),invoiceDate:'2026-09-10',dueDate:'2026-09-30',currency:'THB',subtotal:'10',tax:'0',debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable},key()),invoiceId=String(invoice.id),flow=await resolveFlow(db,f.owner,'invoice',invoiceId);await assignFlowPrimary(db,f.owner,'invoice',invoiceId,{stepKey:'draft',userId:f.manager.userId,reason:'Independent original supplier liability review',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await postInvoice(f.manager,invoiceId,{periodId:f.period,entryDate:'2026-09-10',expectedVersion:1},key());
 const bankReference=key(),statement=await importBankStatement(f.reviewer,{bankAccount:bankId,statementNumber:key(),statementDate:'2026-12-31',opening:'0',closing:'-10',sourceHash:key(),rows:[{externalId:bankReference,transactionDate:'2026-09-13',amount:'-10',currency:'THB',description:'Only actual replacement supplier debit'}]},key()),statementId=String(statement.id);await commercial().apply(change('draining',2));
 const prepare=async()=>{const version=await db.prepare('SELECT version FROM finance_invoices WHERE id=?').bind(invoiceId).first<number>('version');if(!version)throw Error('Original payable version missing');return createSupplierBatch(f.reviewer,{bankAccountId:bankId,plannedDate:'2026-09-12',items:[{invoiceId,expectedVersion:version,amount:'10',beneficiaryName:'Original supplier',beneficiaryAccount:'ISOLATED-RETRY-SUPPLIER'}]},key());};
 const first=await prepare(),firstId=String(first.id);await decideSupplierBatch(f.manager,firstId,{expectedVersion:1,reason:'Independent original beneficiary and payable review'},key(),'approve');await exportSupplierBatch(f.owner,firstId,{expectedVersion:2},key());const original=(await supplierBatchWorkspace(f.owner,firstId)).items[0];if(!original)throw Error('Original exported supplier instruction missing');const originalId=String(original.id),unknownKey=key(),unknownInput={expectedVersion:2,outcome:'unknown' as const,evidence:'Original bank result is uncertain; no instruction is treated as paid or safe to retry yet'};await recordSupplierResult(f.owner,originalId,unknownInput,unknownKey);await expect(prepare()).rejects.toMatchObject({code:'CONFLICT'});
 const failedKey=key(),failedInput={expectedVersion:3,outcome:'failed' as const,evidence:`Independent bank operations review: original instruction ${originalId} was not paid; no debit was made for that original export`};await expect(recordSupplierResult(f.owner,originalId,failedInput,key())).rejects.toMatchObject({code:'INDEPENDENT_REVIEW_REQUIRED'});const failed=await recordSupplierResult(f.manager,originalId,failedInput,failedKey);expect(await recordSupplierResult(f.manager,originalId,failedInput,failedKey)).toEqual(failed);
 const originalRow=await db.prepare('SELECT * FROM finance_supplier_instructions WHERE id=?').bind(originalId).first<Record<string,string|number|null>>();expect(originalRow).toMatchObject({status:'failed',version:4,resolved_by:f.manager.userId,evidence:failedInput.evidence,payment_id:null,bank_transaction_id:null});expect(await db.prepare("SELECT count(*)::int n FROM finance_journals WHERE company_id=? AND source_type='supplier_batch' AND source_id=?").bind(f.company,originalId).first('n')).toBe(0);
 const failedReceipt=await db.prepare('SELECT operation,actor_id,record_id,result_json,authority_json,request_hash FROM command_receipts WHERE command_id=?').bind(failedKey).first<{operation:string;actor_id:string;record_id:string;result_json:string;authority_json:string;request_hash:string}>();if(!failedReceipt)throw Error('Original independent no-payment decision receipt missing');expect(failedReceipt).toMatchObject({operation:'finance.supplier_batch.result',actor_id:f.manager.userId,record_id:originalId});expect(JSON.parse(failedReceipt.result_json)).toEqual({id:originalId,status:'failed',version:4});expect(JSON.parse(failedReceipt.authority_json)).toMatchObject({companyId:f.company,assignmentId:f.manager.assignmentId,role:f.manager.role});
 const expectedDigest=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(JSON.stringify({actor:f.manager.userId,assignment:f.manager.assignmentId,role:f.manager.role,authority:f.manager.authority,company:f.manager.companyId,branch:f.manager.branchId,department:f.manager.departmentId,operation:'finance.supplier_batch.result',recordId:originalId,input:failedInput})));expect(failedReceipt.request_hash).toBe(Array.from(new Uint8Array(expectedDigest),b=>b.toString(16).padStart(2,'0')).join(''));
 expect(await db.prepare('SELECT status,outstanding_minor FROM finance_invoices WHERE id=?').bind(invoiceId).first()).toEqual({status:'posted',outstanding_minor:1000});const inactive=change('inactive',3);await expect(lifecycle(inactive)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect((await supplierBatchWork(f.manager)).find(row=>row.id===firstId)).toMatchObject({queue:'todo'});
 const replacement=await prepare(),replacementId=String(replacement.id);await decideSupplierBatch(f.manager,replacementId,{expectedVersion:1,reason:'Independent exact retry after verified original no-payment decision'},key(),'approve');await exportSupplierBatch(f.payer,replacementId,{expectedVersion:2},key());const item=(await supplierBatchWorkspace(f.payer,replacementId)).items[0],movement=await db.prepare('SELECT id,version FROM finance_bank_transactions WHERE statement_id=? AND external_id=? AND company_id=?').bind(statementId,bankReference,f.company).first<{id:string;version:number}>();if(!item||!movement)throw Error('Original replacement instruction or actual bank debit missing');const confirmKey=key(),confirmation={expectedVersion:2,bankTransactionId:movement.id,bankVersion:movement.version,periodId:f.period,bankReference,evidence:'Only replacement instruction debit matched to exact approved beneficiary and original payable'},confirmed=await confirmSupplierInstruction(f.payer,String(item.id),confirmation,confirmKey);expect(await confirmSupplierInstruction(f.payer,String(item.id),confirmation,confirmKey)).toEqual(confirmed);
 expect(await db.prepare('SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?').bind(invoiceId).first()).toEqual({status:'paid',outstanding_minor:0,base_outstanding_minor:0});expect(await db.prepare('SELECT match_status,matched_type,matched_id FROM finance_bank_transactions WHERE id=?').bind(movement.id).first()).toEqual({match_status:'matched',matched_type:'payment',matched_id:confirmed.paymentId});expect(await db.prepare('SELECT count(*)::int n FROM finance_allocations WHERE payment_id=? AND invoice_id=? AND company_id=? AND amount_minor=1000 AND base_invoice_minor=1000 AND base_payment_minor=1000').bind(confirmed.paymentId,invoiceId,f.company).first('n')).toBe(1);expect(await db.prepare("SELECT count(*)::int n FROM finance_financial_events WHERE journal_id=? AND company_id=? AND source_type='supplier_batch' AND source_id=? AND legacy_snapshot=0").bind(confirmed.journalId,f.company,item.id).first('n')).toBe(1);
 expect((await supplierBatchWork(f.manager)).find(row=>row.id===firstId)).toMatchObject({queue:'completed'});expect(await db.prepare('SELECT * FROM finance_supplier_instructions WHERE id=?').bind(originalId).first()).toEqual(originalRow);expect(await db.prepare("SELECT count(*)::int n FROM finance_supplier_instructions WHERE batch_id IN (?,?) AND status IN ('reserved','exported','unknown')").bind(firstId,replacementId).first('n')).toBe(0);
 const period=await db.prepare('SELECT * FROM finance_periods WHERE id=?').bind(f.period).first<Record<string,string|number|null>>();if(!period)throw Error('Original close period missing');const reconciliation=await calculateCloseReconciliations(db,f.reviewer,period);expect(reconciliation.ready).toBe(true);expect(reconciliation.rows.find(row=>row.accountId===bankId)).toMatchObject({kind:'bank',ledgerMinor:-1000,sourceMinor:-1000,differenceMinor:0,source:{statementId,evidenceComplete:true}});await closeFinance();
 expect((await observeFinance()).outstanding).toBe(0);
 expect((await lifecycle(inactive)).after[0]).toMatchObject({state:'inactive',version:3});const applied=await lifecycle(inactive,true);expect(await lifecycle(inactive,true)).toEqual(applied);expect(await db.prepare('SELECT * FROM finance_supplier_instructions WHERE id=?').bind(originalId).first()).toEqual(originalRow);
});

it('Finance lifecycle retains a failed supplier source until two independently confirmed partial replacements fully settle it',async()=>{
 const {importAccounts,postInvoice,importBankStatement,calculateCloseReconciliations}=await import('@erpx/server-finance/public/finance');
 const {assignFlowPrimary,resolveFlow}=await import('@erpx/server-workflow/public/flow-routing');
 const {createSupplierBatch,decideSupplierBatch,exportSupplierBatch,recordSupplierResult,confirmSupplierInstruction,supplierBatchWorkspace,supplierBatchWork}=await import('@erpx/server-procurement/public/supplier-payments');
 await commercial().apply(change('active',1));vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
 const bankId=f.accounts.cash,configured=await db.prepare('SELECT code,name FROM finance_accounts WHERE id=? AND company_id=?').bind(bankId,f.company).first<{code:string;name:string}>();
 if(!configured)throw Error('Original configured bank ledger missing');
 await importAccounts(f.reviewer,f.chart,[{code:configured.code,name:configured.name,category:'asset',currency:'THB',controlKind:'bank'}],key());
 const supplier=await createParty(f.reviewer,{kind:'supplier',code:key(),name:'Original supplier with split replacement settlement',currency:'THB',paymentTermsDays:30,creditLimit:'0'},key());
 const invoice=await createInvoice(f.reviewer,{partyId:String(supplier.id),kind:'payable',invoiceNumber:key(),invoiceDate:'2026-09-10',dueDate:'2026-09-30',currency:'THB',subtotal:'10',tax:'0',debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable},key()),invoiceId=String(invoice.id);
 const flow=await resolveFlow(db,f.owner,'invoice',invoiceId);
 await assignFlowPrimary(db,f.owner,'invoice',invoiceId,{stepKey:'draft',userId:f.manager.userId,reason:'Independent original supplier liability review',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
 await postInvoice(f.manager,invoiceId,{periodId:f.period,entryDate:'2026-09-10',expectedVersion:1},key());
 const firstReference=key(),lastReference=key();
 const statement=await importBankStatement(f.reviewer,{bankAccount:bankId,statementNumber:key(),statementDate:'2026-12-31',opening:'0',closing:'-10',sourceHash:key(),rows:[{externalId:firstReference,transactionDate:'2026-09-12',amount:'-6',currency:'THB',description:'First actual partial replacement debit'},{externalId:lastReference,transactionDate:'2026-09-13',amount:'-4',currency:'THB',description:'Final actual partial replacement debit'}]},key()),statementId=String(statement.id);
 await commercial().apply(change('draining',2));
 const prepare=async(amount:string)=>{
  const version=await db.prepare('SELECT version FROM finance_invoices WHERE id=?').bind(invoiceId).first<number>('version');if(!version)throw Error('Original source version missing');
  return createSupplierBatch(f.reviewer,{bankAccountId:bankId,plannedDate:'2026-09-12',items:[{invoiceId,expectedVersion:version,amount,beneficiaryName:'Original supplier',beneficiaryAccount:'ISOLATED-SPLIT-SUPPLIER'}]},key());
 };
 const original=await prepare('10'),originalBatchId=String(original.id);
 await decideSupplierBatch(f.manager,originalBatchId,{expectedVersion:1,reason:'Independent review of original full supplier instruction'},key(),'approve');
 await exportSupplierBatch(f.owner,originalBatchId,{expectedVersion:2},key());
 const originalItem=(await supplierBatchWorkspace(f.owner,originalBatchId)).items[0];if(!originalItem)throw Error('Original supplier instruction missing');
 const originalId=String(originalItem.id),failedKey=key(),failedBody={expectedVersion:2,outcome:'failed' as const,evidence:'Independent bank operations verified the original full instruction was not paid and made no debit'};
 const failed=await recordSupplierResult(f.manager,originalId,failedBody,failedKey);expect(await recordSupplierResult(f.manager,originalId,failedBody,failedKey)).toEqual(failed);
 const originalRow=await db.prepare('SELECT * FROM finance_supplier_instructions WHERE id=?').bind(originalId).first();
 expect(originalRow).toMatchObject({status:'failed',version:3,resolved_by:f.manager.userId,payment_id:null,bank_transaction_id:null,bank_reference:null,evidence:failedBody.evidence});
 expect(await db.prepare("SELECT count(*)::int n FROM finance_journals WHERE company_id=? AND source_type='supplier_batch' AND source_id=?").bind(f.company,originalId).first('n')).toBe(0);
 const settle=async(amount:string,reference:string)=>{
  const draft=await prepare(amount),batchId=String(draft.id);
  await decideSupplierBatch(f.manager,batchId,{expectedVersion:1,reason:'Independent review of exact partial amount and original beneficiary'},key(),'approve');
  await exportSupplierBatch(f.payer,batchId,{expectedVersion:2},key());
  const item=(await supplierBatchWorkspace(f.payer,batchId)).items[0],movement=await db.prepare('SELECT id,version FROM finance_bank_transactions WHERE statement_id=? AND company_id=? AND external_id=?').bind(statementId,f.company,reference).first<{id:string;version:number}>();
  if(!item||!movement)throw Error('Actual partial instruction or bank debit missing');
  const command=key(),body={expectedVersion:2,bankTransactionId:movement.id,bankVersion:movement.version,periodId:f.period,bankReference:reference,evidence:'Only this independently approved partial replacement debit matched the original supplier invoice'};
  const result=await confirmSupplierInstruction(f.payer,String(item.id),body,command);expect(await confirmSupplierInstruction(f.payer,String(item.id),body,command)).toEqual(result);
  expect(await db.prepare('SELECT match_status,matched_type,matched_id FROM finance_bank_transactions WHERE id=?').bind(movement.id).first()).toEqual({match_status:'matched',matched_type:'payment',matched_id:result.paymentId});
  expect(await db.prepare("SELECT count(*)::int n FROM finance_financial_events WHERE journal_id=? AND company_id=? AND source_type='supplier_batch' AND source_id=? AND legacy_snapshot=0").bind(result.journalId,f.company,item.id).first('n')).toBe(1);
  return result;
 };
 const first=await settle('6',firstReference),inactive=change('inactive',3);
 expect(await db.prepare('SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?').bind(invoiceId).first()).toEqual({status:'partially_paid',outstanding_minor:400,base_outstanding_minor:400});
 expect((await supplierBatchWork(f.manager)).find(row=>row.id===originalBatchId)).toMatchObject({queue:'todo'});
 await expect(lifecycle(inactive)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await expect(closeFinance()).rejects.toMatchObject({code:'CLOSE_NOT_READY'});
 const last=await settle('4',lastReference);
 expect(first.paymentId).not.toBe(last.paymentId);
 expect(await db.prepare('SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?').bind(invoiceId).first()).toEqual({status:'paid',outstanding_minor:0,base_outstanding_minor:0});
 expect(await db.prepare('SELECT count(*)::int count,sum(amount_minor)::int amount,sum(base_invoice_minor)::int invoice_base,sum(base_payment_minor)::int payment_base FROM finance_allocations WHERE invoice_id=? AND company_id=? AND payment_id IN (?,?)').bind(invoiceId,f.company,first.paymentId,last.paymentId).first()).toEqual({count:2,amount:1000,invoice_base:1000,payment_base:1000});
 expect((await supplierBatchWork(f.manager)).find(row=>row.id===originalBatchId)).toMatchObject({queue:'completed'});
 const period=await db.prepare('SELECT * FROM finance_periods WHERE id=?').bind(f.period).first<Record<string,string|number|null>>();if(!period)throw Error('Original close period missing');
 const reconciliation=await calculateCloseReconciliations(db,f.reviewer,period);expect(reconciliation.ready).toBe(true);
 expect(reconciliation.rows.find(row=>row.accountId===bankId)).toMatchObject({kind:'bank',ledgerMinor:-1000,sourceMinor:-1000,differenceMinor:0,source:{statementId,evidenceComplete:true}});
 await closeFinance();expect(await db.prepare('SELECT * FROM finance_supplier_instructions WHERE id=?').bind(originalId).first()).toEqual(originalRow);
 expect((await observeFinance()).outstanding).toBe(0);
 expect((await lifecycle(inactive)).after[0]).toMatchObject({state:'inactive',version:3});const applied=await lifecycle(inactive,true);expect(await lifecycle(inactive,true)).toEqual(applied);
 expect(await db.prepare('SELECT * FROM finance_supplier_instructions WHERE id=?').bind(originalId).first()).toEqual(originalRow);
});

it('Finance lifecycle retains a failed supplier source until mixed supplier and controlled replacements from two authorized bank accounts fully settle it',async()=>{
 const {importAccounts,postInvoice,importBankStatement,calculateCloseReconciliations,createPayment,approvePayment,releasePayment,confirmPayment}=await import('@erpx/server-finance/public/finance');
 const {assignFlowPrimary,resolveFlow}=await import('@erpx/server-workflow/public/flow-routing');
 const {createSupplierBatch,decideSupplierBatch,exportSupplierBatch,recordSupplierResult,confirmSupplierInstruction,supplierBatchWorkspace,supplierBatchWork}=await import('@erpx/server-procurement/public/supplier-payments');
 await commercial().apply(change('active',1));vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
 const bankId=f.accounts.cash,configured=await db.prepare('SELECT code,name FROM finance_accounts WHERE id=? AND company_id=?').bind(bankId,f.company).first<{code:string;name:string}>();
 if(!configured)throw Error('Original configured bank ledger missing');
 await importAccounts(f.reviewer,f.chart,[{code:configured.code,name:configured.name,category:'asset',currency:'THB',controlKind:'bank'}],key());
 const otherBankCode=`SPLIT-${key().slice(0,8)}`;
 await importAccounts(f.reviewer,f.chart,[{code:otherBankCode,name:'Second authorized supplier bank',category:'asset',currency:'THB',controlKind:'bank'}],key());
 const otherBankId=await db.prepare('SELECT id FROM finance_accounts WHERE company_id=? AND chart_id=? AND code=?').bind(f.company,f.chart,otherBankCode).first<string>('id');if(!otherBankId)throw Error('Second normal authorized bank ledger missing');
 const supplier=await createParty(f.reviewer,{kind:'supplier',code:key(),name:'Original supplier with split replacement settlement',currency:'THB',paymentTermsDays:30,creditLimit:'0'},key());
 const invoice=await createInvoice(f.reviewer,{partyId:String(supplier.id),kind:'payable',invoiceNumber:key(),invoiceDate:'2026-09-10',dueDate:'2026-09-30',currency:'THB',subtotal:'10',tax:'0',debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable},key()),invoiceId=String(invoice.id);
 const flow=await resolveFlow(db,f.owner,'invoice',invoiceId);
 await assignFlowPrimary(db,f.owner,'invoice',invoiceId,{stepKey:'draft',userId:f.manager.userId,reason:'Independent original supplier liability review',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
 await postInvoice(f.manager,invoiceId,{periodId:f.period,entryDate:'2026-09-10',expectedVersion:1},key());
 const firstReference=key(),lastReference=key();
 const statement=await importBankStatement(f.reviewer,{bankAccount:bankId,statementNumber:key(),statementDate:'2026-12-31',opening:'0',closing:'-6',sourceHash:key(),rows:[{externalId:firstReference,transactionDate:'2026-09-12',amount:'-6',currency:'THB',description:'First actual partial replacement debit'}]},key()),statementId=String(statement.id);
 const otherStatement=await importBankStatement(f.reviewer,{bankAccount:otherBankId,statementNumber:key(),statementDate:'2026-12-31',opening:'0',closing:'-4',sourceHash:key(),rows:[{externalId:lastReference,transactionDate:'2026-09-13',amount:'-4',currency:'THB',description:'Final partial replacement from second authorized bank'}]},key()),otherStatementId=String(otherStatement.id);
 await commercial().apply(change('draining',2));
 const prepare=async(amount:string,bankAccountId=bankId)=>{
  const version=await db.prepare('SELECT version FROM finance_invoices WHERE id=?').bind(invoiceId).first<number>('version');if(!version)throw Error('Original source version missing');
  return createSupplierBatch(f.reviewer,{bankAccountId,plannedDate:'2026-09-12',items:[{invoiceId,expectedVersion:version,amount,beneficiaryName:'Original supplier',beneficiaryAccount:'ISOLATED-SPLIT-SUPPLIER'}]},key());
 };
 const original=await prepare('10'),originalBatchId=String(original.id);
 await decideSupplierBatch(f.manager,originalBatchId,{expectedVersion:1,reason:'Independent review of original full supplier instruction'},key(),'approve');
 await exportSupplierBatch(f.owner,originalBatchId,{expectedVersion:2},key());
 const originalItem=(await supplierBatchWorkspace(f.owner,originalBatchId)).items[0];if(!originalItem)throw Error('Original supplier instruction missing');
 const originalId=String(originalItem.id),failedKey=key(),failedBody={expectedVersion:2,outcome:'failed' as const,evidence:'Independent bank operations verified the original full instruction was not paid and made no debit'};
 const failed=await recordSupplierResult(f.manager,originalId,failedBody,failedKey);expect(await recordSupplierResult(f.manager,originalId,failedBody,failedKey)).toEqual(failed);
 const originalRow=await db.prepare('SELECT * FROM finance_supplier_instructions WHERE id=?').bind(originalId).first();
 expect(originalRow).toMatchObject({status:'failed',version:3,resolved_by:f.manager.userId,payment_id:null,bank_transaction_id:null,bank_reference:null,evidence:failedBody.evidence});
 expect(await db.prepare("SELECT count(*)::int n FROM finance_journals WHERE company_id=? AND source_type='supplier_batch' AND source_id=?").bind(f.company,originalId).first('n')).toBe(0);
 const settle=async(amount:string,reference:string,bankAccountId=bankId,sourceStatementId=statementId)=>{
  const draft=await prepare(amount,bankAccountId),batchId=String(draft.id);
  await decideSupplierBatch(f.manager,batchId,{expectedVersion:1,reason:'Independent review of exact partial amount and original beneficiary'},key(),'approve');
  await exportSupplierBatch(f.payer,batchId,{expectedVersion:2},key());
  const item=(await supplierBatchWorkspace(f.payer,batchId)).items[0],movement=await db.prepare('SELECT id,version FROM finance_bank_transactions WHERE statement_id=? AND company_id=? AND external_id=?').bind(sourceStatementId,f.company,reference).first<{id:string;version:number}>();
  if(!item||!movement)throw Error('Actual partial instruction or bank debit missing');
  const command=key(),body={expectedVersion:2,bankTransactionId:movement.id,bankVersion:movement.version,periodId:f.period,bankReference:reference,evidence:'Only this independently approved partial replacement debit matched the original supplier invoice'};
  const result=await confirmSupplierInstruction(f.payer,String(item.id),body,command);expect(await confirmSupplierInstruction(f.payer,String(item.id),body,command)).toEqual(result);
  expect(await db.prepare('SELECT match_status,matched_type,matched_id FROM finance_bank_transactions WHERE id=?').bind(movement.id).first()).toEqual({match_status:'matched',matched_type:'payment',matched_id:result.paymentId});
  expect(await db.prepare("SELECT count(*)::int n FROM finance_financial_events WHERE journal_id=? AND company_id=? AND source_type='supplier_batch' AND source_id=? AND legacy_snapshot=0").bind(result.journalId,f.company,item.id).first('n')).toBe(1);
  return result;
 };
 const first=await settle('6',firstReference),inactive=change('inactive',3);
 expect(await db.prepare('SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?').bind(invoiceId).first()).toEqual({status:'partially_paid',outstanding_minor:400,base_outstanding_minor:400});
 expect((await supplierBatchWork(f.manager)).find(row=>row.id===originalBatchId)).toMatchObject({queue:'todo'});
 await expect(lifecycle(inactive)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await expect(closeFinance()).rejects.toMatchObject({code:'CLOSE_NOT_READY'});
 await expect(prepare('4',otherBankId)).rejects.toMatchObject({code:'BANK_ACCOUNT_INVALID'});
 const otherPayment=await createPayment(f.reviewer,{partyId:String(supplier.id),kind:'payment',paymentNumber:key(),paymentDate:'2026-09-13',currency:'THB',amount:'4',bankAccount:otherBankId,settlementInvoiceId:invoiceId},key()),otherPaymentId=String(otherPayment.id);
 await approvePayment(f.manager,otherPaymentId,{expectedVersion:1,proof:'Independent original source and exact remaining amount review'},key());
 await releasePayment(f.payer,otherPaymentId,{expectedVersion:2,proof:'Independent Treasury review of the second authorized bank and beneficiary'},key());
 const otherMovement=await db.prepare('SELECT id,version FROM finance_bank_transactions WHERE statement_id=? AND company_id=? AND external_id=?').bind(otherStatementId,f.company,lastReference).first<{id:string;version:number}>();if(!otherMovement)throw Error('Actual second bank debit missing');
 const lastKey=key(),lastBody={expectedVersion:3,periodId:f.period,bankTransactionId:otherMovement.id,bankVersion:otherMovement.version,bankReference:lastReference,proof:'Only the controlled replacement debit pays the exact remaining original invoice',allocations:[{invoiceId,amount:'4'}]};
 const lastResult=await confirmPayment(f.payer,otherPaymentId,lastBody,lastKey);expect(await confirmPayment(f.payer,otherPaymentId,lastBody,lastKey)).toEqual(lastResult);
 const last={paymentId:otherPaymentId};
 expect(await db.prepare('SELECT match_status,matched_type,matched_id FROM finance_bank_transactions WHERE id=?').bind(otherMovement.id).first()).toEqual({match_status:'matched',matched_type:'payment',matched_id:otherPaymentId});
 expect(await db.prepare("SELECT count(*)::int n FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_payment' AND source_id=? AND amount_kind IN ('settled','bank_settled') AND currency='THB' AND amount_units=400").bind(f.company,otherPaymentId).first('n')).toBe(2);
 expect(first.paymentId).not.toBe(last.paymentId);
 expect(await db.prepare('SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?').bind(invoiceId).first()).toEqual({status:'paid',outstanding_minor:0,base_outstanding_minor:0});
 expect(await db.prepare('SELECT count(*)::int count,sum(amount_minor)::int amount,sum(base_invoice_minor)::int invoice_base,sum(base_payment_minor)::int payment_base FROM finance_allocations WHERE invoice_id=? AND company_id=? AND payment_id IN (?,?)').bind(invoiceId,f.company,first.paymentId,last.paymentId).first()).toEqual({count:2,amount:1000,invoice_base:1000,payment_base:1000});
 expect((await supplierBatchWork(f.manager)).find(row=>row.id===originalBatchId)).toMatchObject({queue:'completed'});
 const period=await db.prepare('SELECT * FROM finance_periods WHERE id=?').bind(f.period).first<Record<string,string|number|null>>();if(!period)throw Error('Original close period missing');
 const reconciliation=await calculateCloseReconciliations(db,f.reviewer,period);expect(reconciliation.ready).toBe(true);
 expect(reconciliation.rows.find(row=>row.accountId===bankId)).toMatchObject({kind:'bank',ledgerMinor:-600,sourceMinor:-600,differenceMinor:0,source:{statementId,evidenceComplete:true}});
 expect(reconciliation.rows.find(row=>row.accountId===otherBankId)).toMatchObject({kind:'bank',ledgerMinor:-400,sourceMinor:-400,differenceMinor:0,source:{statementId:otherStatementId,evidenceComplete:true}});
 await closeFinance();expect(await db.prepare('SELECT * FROM finance_supplier_instructions WHERE id=?').bind(originalId).first()).toEqual(originalRow);
 expect((await observeFinance()).outstanding).toBe(0);
 expect((await lifecycle(inactive)).after[0]).toMatchObject({state:'inactive',version:3});const applied=await lifecycle(inactive,true);expect(await lifecycle(inactive,true)).toEqual(applied);
 expect(await db.prepare('SELECT * FROM finance_supplier_instructions WHERE id=?').bind(originalId).first()).toEqual(originalRow);
});

it('Finance lifecycle retains a failed supplier source until its originally unbound controlled payment is confirmed against the actual invoice',async()=>{
 const {importAccounts,postInvoice,importBankStatement,createPayment,approvePayment,releasePayment,confirmPayment,calculateCloseReconciliations}=await import('@erpx/server-finance/public/finance');
 const {assignFlowPrimary,resolveFlow}=await import('@erpx/server-workflow/public/flow-routing');
 const {createSupplierBatch,decideSupplierBatch,exportSupplierBatch,recordSupplierResult,supplierBatchWorkspace,supplierBatchWork}=await import('@erpx/server-procurement/public/supplier-payments');
 const foreign=await expenseFixture(),foreignSupplier=await createParty(foreign.reviewer,{kind:'supplier',code:key(),name:'Foreign Company supplier',currency:'THB',paymentTermsDays:30,creditLimit:'0'},key());
 const foreignInvoice=await createInvoice(foreign.reviewer,{partyId:String(foreignSupplier.id),kind:'payable',invoiceNumber:key(),invoiceDate:'2026-09-10',dueDate:'2026-09-30',currency:'THB',subtotal:'10',tax:'0',debitAccountId:foreign.accounts.expense,creditAccountId:foreign.accounts.payable},key());
 await commercial().apply(change('active',1));vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
 const bankId=f.accounts.cash,configured=await db.prepare('SELECT code,name FROM finance_accounts WHERE id=? AND company_id=?').bind(bankId,f.company).first<{code:string;name:string}>();if(!configured)throw Error('Original configured bank missing');
 await importAccounts(f.reviewer,f.chart,[{code:configured.code,name:configured.name,category:'asset',currency:'THB',controlKind:'bank'}],key());
 const supplier=await createParty(f.reviewer,{kind:'supplier',code:key(),name:'Original supplier paid through an initially unbound payment',currency:'THB',paymentTermsDays:30,creditLimit:'0'},key());
 const invoice=await createInvoice(f.reviewer,{partyId:String(supplier.id),kind:'payable',invoiceNumber:key(),invoiceDate:'2026-09-10',dueDate:'2026-09-30',currency:'THB',subtotal:'10',tax:'0',debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable},key()),invoiceId=String(invoice.id),flow=await resolveFlow(db,f.owner,'invoice',invoiceId);
 await assignFlowPrimary(db,f.owner,'invoice',invoiceId,{stepKey:'draft',userId:f.manager.userId,reason:'Independent original payable review',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
 await postInvoice(f.manager,invoiceId,{periodId:f.period,entryDate:'2026-09-10',expectedVersion:1},key());
 const original=await createSupplierBatch(f.reviewer,{bankAccountId:bankId,plannedDate:'2026-09-12',items:[{invoiceId,expectedVersion:2,amount:'10',beneficiaryName:'Original supplier',beneficiaryAccount:'ISOLATED-UNBOUND-SUPPLIER'}]},key()),batchId=String(original.id);
 await decideSupplierBatch(f.manager,batchId,{expectedVersion:1,reason:'Independent original supplier instruction review'},key(),'approve');await exportSupplierBatch(f.owner,batchId,{expectedVersion:2},key());
 const item=(await supplierBatchWorkspace(f.owner,batchId)).items[0];if(!item)throw Error('Original exported instruction missing');const originalId=String(item.id),failedKey=key(),failedInput={expectedVersion:2,outcome:'failed' as const,evidence:'Independent bank review verified original instruction not paid and no debit made'};
 const failed=await recordSupplierResult(f.manager,originalId,failedInput,failedKey);expect(await recordSupplierResult(f.manager,originalId,failedInput,failedKey)).toEqual(failed);
 const retained=await db.prepare('SELECT * FROM finance_supplier_instructions WHERE id=?').bind(originalId).first();expect(retained).toMatchObject({status:'failed',version:3,payment_id:null,bank_transaction_id:null,bank_reference:null,resolved_by:f.manager.userId});
 const paymentInput={partyId:String(supplier.id),kind:'payment' as const,paymentNumber:key(),paymentDate:'2026-09-13',currency:'THB',amount:'10',bankAccount:bankId};
 const foreignKey=key();await expect(createPayment(f.reviewer,{...paymentInput,settlementInvoiceId:String(foreignInvoice.id)},foreignKey)).rejects.toMatchObject({code:'PAYMENT_SETTLEMENT_SOURCE_INVALID'});
 expect(await db.prepare('SELECT command_id FROM command_receipts WHERE command_id=?').bind(foreignKey).first()).toBeNull();
 const createKey=key(),payment=await createPayment(f.reviewer,paymentInput,createKey),paymentId=String(payment.id);expect(await createPayment(f.reviewer,paymentInput,createKey)).toEqual(payment);
 const creation=await db.prepare('SELECT result_json FROM command_receipts WHERE command_id=? AND operation=?').bind(createKey,'finance.payment.create').first<{result_json:string}>();if(!creation)throw Error('Original unbound creation receipt missing');expect(JSON.parse(creation.result_json)).not.toHaveProperty('settlementInvoiceId');
 const reference=key(),statement=await importBankStatement(f.reviewer,{bankAccount:bankId,statementNumber:key(),statementDate:'2026-12-31',opening:'0',closing:'-10',sourceHash:key(),rows:[{externalId:reference,transactionDate:'2026-09-13',amount:'-10',currency:'THB',description:'Only actual independently controlled replacement debit'}]},key()),statementId=String(statement.id);
 await commercial().apply(change('draining',2));const inactive=change('inactive',3);await expect(lifecycle(inactive)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 await approvePayment(f.manager,paymentId,{expectedVersion:1,proof:'Independent exact original invoice and unbound payment amount review'},key());await releasePayment(f.payer,paymentId,{expectedVersion:2,proof:'Independent Treasury review before original bank confirmation'},key());
 const movement=await db.prepare('SELECT id,version FROM finance_bank_transactions WHERE statement_id=? AND company_id=?').bind(statementId,f.company).first<{id:string;version:number}>();if(!movement)throw Error('Actual replacement bank movement missing');
 const confirmKey=key(),confirmation={expectedVersion:3,periodId:f.period,bankTransactionId:movement.id,bankVersion:movement.version,bankReference:reference,proof:'Actual bank debit independently matched to the sole original supplier invoice',allocations:[{invoiceId,amount:'10'}]};
 const result=await confirmPayment(f.payer,paymentId,confirmation,confirmKey);expect(await confirmPayment(f.payer,paymentId,confirmation,confirmKey)).toEqual(result);
 expect(await db.prepare('SELECT id,company_id,payment_id,invoice_id,amount_minor,base_invoice_minor,base_payment_minor,created_by FROM finance_allocations WHERE payment_id=?').bind(paymentId).first()).toEqual({id:`allocation-${confirmKey}-0`,company_id:f.company,payment_id:paymentId,invoice_id:invoiceId,amount_minor:1000,base_invoice_minor:1000,base_payment_minor:1000,created_by:f.payer.userId});
 const confirmed=await db.prepare('SELECT record_id,actor_id,result_json,authority_json FROM command_receipts WHERE command_id=? AND operation=?').bind(confirmKey,'finance.payment.confirm').first<{record_id:string;actor_id:string;result_json:string;authority_json:string}>();if(!confirmed)throw Error('Original immutable confirmation receipt missing');expect(confirmed).toMatchObject({record_id:paymentId,actor_id:f.payer.userId});expect(JSON.parse(confirmed.authority_json)).toMatchObject({companyId:f.company});expect(JSON.parse(confirmed.result_json)).toMatchObject({id:paymentId,status:'settled',journalId:result.journalId,bankTransactionId:movement.id});
 expect(await db.prepare('SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?').bind(invoiceId).first()).toEqual({status:'paid',outstanding_minor:0,base_outstanding_minor:0});expect((await supplierBatchWork(f.manager)).find(row=>row.id===batchId)).toMatchObject({queue:'completed'});
 const period=await db.prepare('SELECT * FROM finance_periods WHERE id=?').bind(f.period).first<Record<string,string|number|null>>();if(!period)throw Error('Original close period missing');const reconciliation=await calculateCloseReconciliations(db,f.reviewer,period);expect(reconciliation.ready).toBe(true);expect(reconciliation.rows.find(row=>row.accountId===bankId)).toMatchObject({kind:'bank',ledgerMinor:-1000,sourceMinor:-1000,differenceMinor:0,source:{statementId,evidenceComplete:true}});await closeFinance();
 expect((await observeFinance()).outstanding).toBe(0);
 expect((await lifecycle(inactive)).after[0]).toMatchObject({state:'inactive',version:3});const applied=await lifecycle(inactive,true);expect(await lifecycle(inactive,true)).toEqual(applied);expect(await db.prepare('SELECT * FROM finance_supplier_instructions WHERE id=?').bind(originalId).first()).toEqual(retained);
});
