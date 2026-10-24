import {getRecord,listRecords,recordsPage,recordContext,searchReferences} from "@erpx/server-records/public/records";
import {findModule} from "@erpx/catalogue/module-registry";
import {requireFlowAccess} from "@erpx/server-workflow/public/flow-access";
import {enrichFlowWork} from "@erpx/server-workflow/public/flow-work";
import {invoiceRecordWorkspace} from "@erpx/server-finance/public/record-presentation";
import {publishInboxEvent,inboxPage,inboxSummary,sendNotice} from "@erpx/server-collaboration/public/inbox";
import {listStaffNotifications,notificationsPage,visibleNotifications} from "@erpx/server-collaboration/public/staff-notifications";
import type {WorkItem} from "@erpx/feature-work_projection/lib/work-item";
import { financePurchaseControls } from "@erpx/server-finance/public/purchases";
import { financeWorkbench, paymentRecordsWorkspace, paymentObligationsWorkspace, financeSetupWorkspace } from "@erpx/server-finance/public/finance-workspace";
import { ledgerWorkspace, journalDetail, workJournalRows } from "@erpx/server-finance/public/ledger";
import { beforeAll,beforeEach,afterEach,describe,expect,it,vi } from "vitest";
import { applyMigrations,env,type TestMigration } from "../../support/runtime-env";
import { expenseDb as db,expenseFixture,key } from "../../support/helpers/expense-fixture";
import {approvePayment,bankWorkspace,calculateCloseReconciliations,confirmPayment,createInvoice,createParty,createPayment,importBankStatement,invoicesPage,postInvoice,releasePayment,reversePayment,workInvoiceRows,createJournal,createJournalSchema,listCharts,listAccounts,getPayment} from '@erpx/server-finance/public/finance';
import {getInvoice} from '@erpx/server-finance/public/finance-invoice-read';
import { packageGrantStore,readCompanyPackageGrants } from "@erpx/server-commercial/public/package-grants";
import { confirmManualRate } from "@erpx/server-finance/public/company-money";
import {assignFlowPrimary,resolveFlow} from '@erpx/server-workflow/public/flow-routing';

vi.mock("server-only",()=>({}));
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));

let f:Awaited<ReturnType<typeof expenseFixture>>;

beforeAll(async()=>{await Promise.all([applyMigrations(db,env.STAFF_TEST_MIGRATIONS as TestMigration[]),applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS as TestMigration[])]);});
afterEach(()=>vi.unstubAllEnvs());
beforeEach(async()=>{f=await expenseFixture();await db.batch([db.prepare("UPDATE finance_accounts SET is_control=1,control_kind='bank' WHERE id=?").bind(f.accounts.cash),db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,'asset','THB',1,'receivable',1,1)").bind(`${f.company}-receivable`,f.chart,f.company,"receivable","Trade receivables")]);});

async function payable(amount="100"){
 const supplier=await createParty(f.reviewer,{kind:"supplier",code:`SUP-${key()}`,name:"Controlled payment supplier",currency:"THB",paymentTermsDays:30,creditLimit:"0"},key());
 const invoice=await createInvoice(f.reviewer,{partyId:String(supplier.id),kind:"payable",invoiceNumber:`BILL-${key()}`,invoiceDate:"2026-09-10",dueDate:"2026-10-10",currency:"THB",subtotal:amount,tax:"0",debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable},key());
 const flow=await resolveFlow(db,f.owner,"invoice",String(invoice.id));
 await assignFlowPrimary(db,f.owner,"invoice",String(invoice.id),{stepKey:"draft",userId:f.manager.userId,reason:"Independent payment acceptance reviewer",saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
 await postInvoice(f.manager,String(invoice.id),{periodId:f.period,entryDate:"2026-09-10",expectedVersion:1},key());
 return {supplier,invoice};
}
async function financePackage(state:"active"|"draining"="active",endsAt:number|null=null,packageId:"finance"|"pos"="finance"){
 const current=(await readCompanyPackageGrants(db,f.company)).find(grant=>grant.packageId===packageId);
 return packageGrantStore(db,{authorize:async()=>({userId:"isolated-finance-purchase-fixture"}),obligations:async()=>{throw Error("No test Finance withdrawal witness");}}).apply({expectedVersion:current?.version??0,idempotencyKey:key(),grant:{companyId:f.company,packageId,packageVersion:1,state,startsAt:0,endsAt,version:(current?.version??0)+1,reason:"Native Finance purchase review",evidence:"test:finance-purchase-core"}});
}
const financialRows=()=>Promise.all(["finance_invoices","finance_payments","finance_journals","finance_journal_lines","finance_allocations","finance_bank_transactions","finance_bank_statements","money_transaction_snapshots","command_receipts","audit_events","outbox_events"].map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results));
it("package Finance denies unpurchased money and reads, while shadow retains original authority",async()=>{
 const {supplier,invoice}=await payable(),input={partyId:String(supplier.id),kind:"payment" as const,paymentNumber:key(),paymentDate:"2026-09-12",currency:"THB",amount:"100",bankAccount:f.accounts.cash},before=await financialRows();vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 await expect(createPayment(f.reviewer,input,key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(createJournal(f.reviewer,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:"2026-09-12",currency:"THB",memo:"Unpurchased manual GL must fail",lines:[{accountId:f.accounts.expense,debit:"10",credit:"0"},{accountId:f.accounts.cash,debit:"0",credit:"10"}]}),key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(createInvoice(f.reviewer,{partyId:String(supplier.id),kind:"payable",invoiceNumber:key(),invoiceDate:"2026-09-10",dueDate:"2026-09-12",currency:"THB",subtotal:"100",tax:"0",debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable},key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 for(const read of [()=>getInvoice(f.owner,String(invoice.id)),()=>bankWorkspace(f.owner,{}),()=>invoicesPage(f.owner,{}),()=>financeWorkbench(f.owner),()=>paymentRecordsWorkspace(f.owner,{}),()=>paymentObligationsWorkspace(f.owner,{}),()=>financeSetupWorkspace(f.owner),()=>ledgerWorkspace(f.owner),()=>journalDetail(f.owner,"unavailable")])await expect(read()).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});expect(await workInvoiceRows(f.owner,db)).toEqual([]);expect(await workJournalRows(f.owner,db)).toEqual([]);expect(await financialRows()).toEqual(before);
 vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","shadow");const log=vi.spyOn(console,"info").mockImplementation(()=>{});
 try{expect(await getInvoice(f.owner,String(invoice.id))).toMatchObject({invoice:{id:invoice.id}});await createPayment(f.reviewer,input,key());expect(log.mock.calls.some(args=>String(args[0]).includes("commercial.purchase.shadow")&&String(args[0]).includes("PACKAGE_REQUIRED"))).toBe(true);await expect(createPayment({...f.payer,role:"auditor"},input,key())).rejects.toMatchObject({status:403});}finally{log.mockRestore();}
});
it("package Finance preserves POS accounting references without granting general Finance work",async()=>{
 await financePackage("active",null,"pos");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 expect((await listCharts(f.owner)).map(row=>row.id)).toContain(f.chart);expect((await listAccounts(f.owner,f.chart)).map(row=>row.id)).toContain(f.accounts.cash);
 for(const read of [()=>financeSetupWorkspace(f.owner),()=>ledgerWorkspace(f.owner)])await expect(read()).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 const before=await financialRows();await expect(createJournal(f.reviewer,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:"2026-09-12",currency:"THB",sourceType:"pos_sale",memo:"A user supplied source label cannot select an included foundation",lines:[{accountId:f.accounts.expense,debit:"10",credit:"0"},{accountId:f.accounts.cash,debit:"0",credit:"10"}]}),key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});expect(await financialRows()).toEqual(before);
});
it("package Finance-only draining prepares an invoice-bound payment and completes independent bank settlement and replay",async()=>{
 await financePackage();vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");const {supplier,invoice}=await payable();await financePackage("draining");
 const controls=await financePurchaseControls(f.owner);expect(Object.keys(controls??{}).sort()).toEqual(["finance.accounting","finance.advanced","finance.banking","finance.close","finance.receivables","finance.tax"]);for(const capability of ["finance.accounting","finance.banking","finance.close","finance.receivables","finance.tax"])expect(controls?.[capability]).toEqual({newWork:false,settleExisting:true,history:true,revision:expect.any(String)});expect(controls?.["finance.advanced"]).toEqual({newWork:false,settleExisting:false,history:false,revision:expect.any(String)});
 const input={partyId:String(supplier.id),kind:"payment" as const,paymentNumber:key(),paymentDate:"2026-09-12",currency:"THB",amount:"100",bankAccount:f.accounts.cash};
 await expect(createPayment(f.reviewer,input,key())).rejects.toMatchObject({code:"PACKAGE_DRAINING"});const createKey=key(),bound={...input,settlementInvoiceId:String(invoice.id)},payment=await createPayment(f.reviewer,bound,createKey);expect(payment.settlementInvoiceId).toBe(invoice.id);expect((await getPayment(f.payer,String(payment.id))).payment.settlement_invoice_id).toBe(invoice.id);expect((await getPayment(f.payer,String(payment.id))).invoices.map(row=>row.id)).toEqual([invoice.id]);expect(await createPayment(f.reviewer,bound,createKey)).toEqual(payment);
 await approvePayment(f.manager,String(payment.id),{expectedVersion:1,proof:"Independent invoice and payment review"},key());await releasePayment(f.payer,String(payment.id),{expectedVersion:2,proof:"Treasury reviewed original invoice and beneficiary"},key());
 const statement=await importBankStatement(f.reviewer,{bankAccount:f.accounts.cash,statementNumber:key(),statementDate:"2026-09-30",opening:"1000",closing:"900",sourceHash:key(),rows:[{externalId:key(),transactionDate:"2026-09-12",description:"Existing supplier liability settlement",amount:"-100",currency:"THB"}]},key()),bankId=String(await db.prepare("SELECT id FROM finance_bank_transactions WHERE statement_id=?").bind(statement.id).first("id"));
 const confirmation={expectedVersion:3,periodId:f.period,bankTransactionId:bankId,bankVersion:1,bankReference:"NATIVE-PURCHASE-SETTLEMENT",proof:"Treasury verified exact bank evidence",allocations:[{invoiceId:String(invoice.id),amount:"100"}]},receipt=key(),result=await confirmPayment(f.payer,String(payment.id),confirmation,receipt);
 expect(result).toMatchObject({id:payment.id,status:"settled",version:4});expect(await confirmPayment(f.payer,String(payment.id),confirmation,receipt)).toEqual(result);expect(await db.prepare("SELECT status,outstanding_minor FROM finance_invoices WHERE id=?").bind(invoice.id).first()).toEqual({status:"paid",outstanding_minor:0});
 expect((await readCompanyPackageGrants(db,f.company)).map(grant=>grant.packageId)).toEqual(["finance"]);expect(await getInvoice(f.owner,String(invoice.id))).toMatchObject({invoice:{id:invoice.id,status:"paid"}});expect(await db.prepare("SELECT count(*) n FROM command_assertions").first()).toEqual({n:0});
 await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(f.owner.assignmentId).run();await expect(getInvoice(f.owner,String(invoice.id))).rejects.toMatchObject({code:"FINANCE_ACCESS_CHANGED"});
});
it("package Finance draining rejects invented, oversized and retargeted settlement bindings",async()=>{
 await financePackage();const{supplier,invoice}=await payable();await financePackage("draining");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");const input={partyId:String(supplier.id),kind:"payment" as const,paymentNumber:key(),paymentDate:"2026-09-12",currency:"THB",amount:"100",bankAccount:f.accounts.cash,settlementInvoiceId:String(invoice.id)},before=await financialRows();
 for(const change of [{settlementInvoiceId:"missing-invoice"},{amount:"100.01"},{partyId:"foreign-party"},{currency:"USD"}])await expect(createPayment(f.reviewer,{...input,...change},key())).rejects.toMatchObject({code:"PAYMENT_SETTLEMENT_SOURCE_INVALID"});expect(await financialRows()).toEqual(before);
 const payment=await createPayment(f.reviewer,input,key());await approvePayment(f.manager,String(payment.id),{expectedVersion:1,proof:"Original invoice review"},key());await releasePayment(f.payer,String(payment.id),{expectedVersion:2,proof:"Original beneficiary review"},key());
 await expect(confirmPayment(f.payer,String(payment.id),{expectedVersion:3,periodId:f.period,bankTransactionId:"not-used",bankVersion:1,bankReference:"not-used",proof:"Retarget must fail before bank work",allocations:[{invoiceId:"another-invoice",amount:"100"}]},key())).rejects.toMatchObject({code:"PAYMENT_SETTLEMENT_SOURCE_CHANGED"});expect(await db.prepare("SELECT status,version FROM finance_payments WHERE id=?").bind(payment.id).first()).toEqual({status:"released",version:3});
});
it("package Finance expires after deferred invoice effects and rolls back money, record and receipt together",async()=>{
 const supplier=await createParty(f.reviewer,{kind:"supplier",code:key(),name:"Expiry supplier",currency:"THB",paymentTermsDays:30,creditLimit:"0"},key());await financePackage("active",Date.now()+1500);vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");const before=await financialRows(),command=key(),trigger=`finance_purchase_${key().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(1.6);RETURN NEW;END $$;
  CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.action='finance.invoice.create' AND NEW.authority_json::jsonb->>'companyId'='${f.company}') EXECUTE FUNCTION ${trigger}()`);
 try{await expect(createInvoice(f.reviewer,{partyId:String(supplier.id),kind:"payable",invoiceNumber:key(),invoiceDate:"2026-09-10",dueDate:"2026-09-12",currency:"THB",subtotal:"100",tax:"0",debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable},command)).rejects.toMatchObject({code:"PACKAGE_CAPABILITY_CHANGED"});expect(await financialRows()).toEqual(before);expect(await db.prepare("SELECT command_id FROM command_receipts WHERE command_id=?").bind(command).first()).toBeNull();}finally{await db.exec(`DROP TRIGGER ${trigger} ON audit_events;DROP FUNCTION ${trigger}()`);}
});

describe("generic controlled payments",()=>{
 it("reads each statement's bank currency and chart currency in paged and print views",async()=>{
  const statement=await importBankStatement(f.reviewer,{bankAccount:f.accounts.cash,statementNumber:`DISPLAY-${key()}`,statementDate:"2026-09-30",opening:"0",closing:"50",sourceHash:key(),rows:[{externalId:`DISPLAY-ROW-${key()}`,transactionDate:"2026-09-30",amount:"50",currency:"THB"}]},key());
  for(const query of [{},{print:"1"}]) {
   const view=await bankWorkspace(f.owner,query);
   expect(view.statements.find(row=>row.id===statement.id)).toMatchObject({currency:"THB",base_currency:"THB",opening_minor:0,closing_minor:5000});
   expect(view.statements.every(row=>typeof row.currency==="string"&&row.currency!=="undefined")).toBe(true);
  }
  const other=await expenseFixture();
  expect((await bankWorkspace(other.owner,{})).statements.some(row=>row.id===statement.id)).toBe(false);
  expect((await bankWorkspace(f.owner,{chart:f.chart,view:"unmatched",q:"DISPLAY"})).transactions).toHaveLength(1);
  expect((await bankWorkspace(f.owner,{chart:other.chart})).statements).toEqual([]);
  expect((await bankWorkspace(f.owner,{chart:other.chart})).transactions).toEqual([]);
  expect((await bankWorkspace(f.owner,{q:"does not exist"})).transactions).toEqual([]);
 });
 it("filters payable documents by their owning chart without leaking another Company",async()=>{
  const supplier=await createParty(f.reviewer,{kind:"supplier",code:`CHART-${key()}`,name:"Chart selection supplier",currency:"THB",paymentTermsDays:30,creditLimit:"0"},key());
  const invoice=await createInvoice(f.reviewer,{partyId:String(supplier.id),kind:"payable",invoiceNumber:`CHART-${key()}`,invoiceDate:"2026-09-10",dueDate:"2026-10-10",currency:"THB",subtotal:"10",tax:"0",debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable},key());
  const other=await expenseFixture();
  expect((await invoicesPage(f.owner,{chartId:f.chart,kind:"payable"})).items.map(row=>row.id)).toContain(invoice.id);
  expect((await invoicesPage(f.owner,{chartId:f.chart,kind:"receivable"})).items).toEqual([]);
  expect((await invoicesPage(f.owner,{chartId:other.chart})).items).toEqual([]);
  expect((await invoicesPage(other.owner,{chartId:f.chart})).items).toEqual([]);
 });
 it("freezes the confirmed USD snapshot when an invoice is posted and a dated rate is revised",async()=>{
  const original=await confirmManualRate(f.manager,{currency:"THB",effectiveDate:"2026-09-10",currencyPerUsd:"36",note:"Company manual quote"},key());
  const {invoice}=await payable("100");
  const before=await db.prepare("SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_invoice' AND source_id=? AND amount_kind='total'").bind(f.company,invoice.id).first();
  expect(before).toEqual({amount_units:"10000",usd_units:"278",fx_rate_id:original.id});
  await confirmManualRate(f.manager,{currency:"THB",effectiveDate:"2026-09-10",currencyPerUsd:"37",note:"Revised after posting"},key());
  expect(await db.prepare("SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_invoice' AND source_id=? AND amount_kind='total'").bind(f.company,invoice.id).first()).toEqual(before);
 });
 it("uses three people and atomically posts, allocates, and reconciles exact bank evidence",async()=>{
  const {supplier,invoice}=await payable();
  const payment=await createPayment(f.reviewer,{partyId:String(supplier.id),kind:"payment",paymentNumber:`PAY-${key()}`,paymentDate:"2026-09-12",currency:"THB",amount:"100",bankAccount:f.accounts.cash},key());
  await approvePayment(f.manager,String(payment.id),{expectedVersion:1,proof:"Controller reviewed supplier, invoice and amount"},key());
  await expect(releasePayment(f.manager,String(payment.id),{expectedVersion:2,proof:"Approver cannot also pay"},key())).rejects.toMatchObject({code:"INDEPENDENT_PAYER_REQUIRED"});
  await releasePayment(f.payer,String(payment.id),{expectedVersion:2,proof:"Treasury checked approved beneficiary and bank details"},key());
  const statement=await importBankStatement(f.reviewer,{bankAccount:f.accounts.cash,statementNumber:`BANK-${key()}`,statementDate:"2026-09-30",opening:"1000",closing:"900",sourceHash:key(),rows:[{externalId:`TX-${key()}`,transactionDate:"2026-09-12",description:"Supplier transfer",amount:"-100",currency:"THB"}]},key());
  const bankId=String(await db.prepare("SELECT id FROM finance_bank_transactions WHERE statement_id=?").bind(statement.id).first("id")),command=key(),input={expectedVersion:3,periodId:f.period,bankTransactionId:bankId,bankVersion:1,bankReference:"BANK-CONFIRMED-001",proof:"Treasury matched the posted bank row to the approved payment",allocations:[{invoiceId:String(invoice.id),amount:"100"}]};
  const result=await confirmPayment(f.payer,String(payment.id),input,command);
  expect(result).toMatchObject({id:payment.id,status:"settled",version:4,bankTransactionId:bankId});
  expect(await confirmPayment(f.payer,String(payment.id),input,command)).toEqual(result);
  const stored=await db.prepare("SELECT status,outstanding_minor,journal_id FROM finance_payments WHERE id=?").bind(payment.id).first<Record<string,string|number>>(),bill=await db.prepare("SELECT status,outstanding_minor FROM finance_invoices WHERE id=?").bind(invoice.id).first<Record<string,string|number>>(),bank=await db.prepare("SELECT match_status,matched_type,matched_id FROM finance_bank_transactions WHERE id=?").bind(bankId).first<Record<string,string|number>>();
  expect(stored).toMatchObject({status:"settled",outstanding_minor:0});expect(stored?.journal_id).toBeTruthy();expect(bill).toMatchObject({status:"paid",outstanding_minor:0});expect(bank).toMatchObject({match_status:"matched",matched_type:"payment",matched_id:payment.id});
  const balances=await db.prepare("SELECT account_id,SUM(debit_minor-credit_minor) balance FROM finance_journal_lines WHERE journal_id=? GROUP BY account_id").bind(stored!.journal_id).all<{account_id:string;balance:number}>();
  expect(Object.fromEntries(balances.results.map(row=>[row.account_id,Number(row.balance)]))).toEqual({[f.accounts.payable]:10000,[f.accounts.cash]:-10000});
  expect(await db.prepare("SELECT count(*) n FROM decision_evidence WHERE record_id=?").bind(payment.id).first("n")).toBe(3);
 });

 it("rejects mismatched bank amounts and stale concurrent confirmation",async()=>{
  const {supplier,invoice}=await payable("50"),payment=await createPayment(f.reviewer,{partyId:String(supplier.id),kind:"payment",paymentNumber:`PAY-${key()}`,paymentDate:"2026-09-15",currency:"THB",amount:"50",bankAccount:f.accounts.cash},key());
  await approvePayment(f.manager,String(payment.id),{expectedVersion:1,proof:"Independent amount review"},key());await releasePayment(f.payer,String(payment.id),{expectedVersion:2,proof:"Independent treasury release"},key());
  const statement=await importBankStatement(f.reviewer,{bankAccount:f.accounts.cash,statementNumber:`BANK-${key()}`,statementDate:"2026-09-30",opening:"100",closing:"40",sourceHash:key(),rows:[{externalId:`TX-${key()}`,transactionDate:"2026-09-15",amount:"-60",currency:"THB"}]},key()),bankId=String(await db.prepare("SELECT id FROM finance_bank_transactions WHERE statement_id=?").bind(statement.id).first("id"));
  await expect(confirmPayment(f.payer,String(payment.id),{expectedVersion:3,periodId:f.period,bankTransactionId:bankId,bankVersion:1,bankReference:"WRONG-AMOUNT",proof:"This evidence must not be accepted",allocations:[{invoiceId:String(invoice.id),amount:"50"}]},key())).rejects.toMatchObject({code:"BANK_EVIDENCE_MISMATCH"});
  expect(await db.prepare("SELECT status FROM finance_payments WHERE id=?").bind(payment.id).first("status")).toBe("released");expect(await db.prepare("SELECT outstanding_minor FROM finance_invoices WHERE id=?").bind(invoice.id).first("outstanding_minor")).toBe(5000);
 });

 it("allocates one customer receipt across multiple invoices",async()=>{
  const customer=await createParty(f.reviewer,{kind:"customer",code:`CUS-${key()}`,name:"Controlled receipt customer",currency:"THB",paymentTermsDays:30,creditLimit:"1000"},key()),invoiceIds:string[]=[];
  for(const amount of ["100","50"]){const invoice=await createInvoice(f.reviewer,{partyId:String(customer.id),kind:"receivable",invoiceNumber:`INV-${key()}`,invoiceDate:"2026-09-10",dueDate:"2026-10-10",currency:"THB",subtotal:amount,tax:"0",debitAccountId:`${f.company}-receivable`,creditAccountId:f.accounts.revenue},key()),flow=await resolveFlow(db,f.owner,"invoice",String(invoice.id));await assignFlowPrimary(db,f.owner,"invoice",String(invoice.id),{stepKey:"draft",userId:f.manager.userId,reason:"Independent customer invoice reviewer",saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await postInvoice(f.manager,String(invoice.id),{periodId:f.period,entryDate:"2026-09-10",expectedVersion:1},key());invoiceIds.push(String(invoice.id));}
  const payment=await createPayment(f.reviewer,{partyId:String(customer.id),kind:"receipt",paymentNumber:`REC-${key()}`,paymentDate:"2026-09-20",currency:"THB",amount:"150",bankAccount:f.accounts.cash},key());await approvePayment(f.manager,String(payment.id),{expectedVersion:1,proof:"Independent receipt approval"},key());await releasePayment(f.payer,String(payment.id),{expectedVersion:2,proof:"Treasury prepared receipt matching"},key());
  const statement=await importBankStatement(f.reviewer,{bankAccount:f.accounts.cash,statementNumber:`BANK-${key()}`,statementDate:"2026-09-30",opening:"0",closing:"150",sourceHash:key(),rows:[{externalId:`TX-${key()}`,transactionDate:"2026-09-20",amount:"150",currency:"THB"}]},key()),bankId=String(await db.prepare("SELECT id FROM finance_bank_transactions WHERE statement_id=?").bind(statement.id).first("id")),result=await confirmPayment(f.payer,String(payment.id),{expectedVersion:3,periodId:f.period,bankTransactionId:bankId,bankVersion:1,bankReference:"CUSTOMER-RECEIPT",proof:"Treasury matched customer remittance",allocations:[{invoiceId:invoiceIds[0],amount:"100"},{invoiceId:invoiceIds[1],amount:"50"}]},key());
  expect(result.status).toBe("settled");expect(await db.prepare(`SELECT count(*) n FROM finance_invoices WHERE id IN (?,?) AND status='paid' AND outstanding_minor=0`).bind(...invoiceIds).first("n")).toBe(2);expect(await db.prepare("SELECT count(*) n FROM finance_allocations WHERE payment_id=?").bind(payment.id).first("n")).toBe(2);
 });

 it("posts a foreign supplier invoice, settles it at the bank-date rate, and clears AP through realized FX",async()=>{
  f=await expenseFixture(true,false);
  const ids={expense:`usd-expense-${key()}`,payable:`usd-payable-${key()}`,bank:`usd-bank-${key()}`,gain:`fx-gain-${key()}`,loss:`fx-loss-${key()}`};
  await db.batch([
   db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,'expense','USD',0,'none',1,1)").bind(ids.expense,f.chart,f.company,`USD-EXP-${key()}`,"USD purchasing expense"),
   db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,'liability','USD',1,'payable',1,1)").bind(ids.payable,f.chart,f.company,`USD-AP-${key()}`,"USD trade payables"),
   db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,'asset','USD',1,'bank',1,1)").bind(ids.bank,f.chart,f.company,`USD-BANK-${key()}`,"USD bank"),
   db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,'other_income','THB',0,'none',1,1)").bind(ids.gain,f.chart,f.company,`FX-GAIN-${key()}`,"Realized FX gain"),
   db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,'other_expense','THB',0,'none',1,1)").bind(ids.loss,f.chart,f.company,`FX-LOSS-${key()}`,"Realized FX loss"),
  ]);
  const supplier=await createParty(f.reviewer,{kind:"supplier",code:`USD-SUP-${key()}`,name:"USD supplier",currency:"USD",paymentTermsDays:30,creditLimit:"0"},key()),invoice=await createInvoice(f.reviewer,{partyId:String(supplier.id),kind:"payable",invoiceNumber:`USD-BILL-${key()}`,invoiceDate:"2026-09-10",dueDate:"2026-10-10",currency:"USD",subtotal:"100",tax:"0",debitAccountId:ids.expense,creditAccountId:ids.payable},key()),flow=await resolveFlow(db,f.owner,"invoice",String(invoice.id));
  await assignFlowPrimary(db,f.owner,"invoice",String(invoice.id),{stepKey:"draft",userId:f.manager.userId,reason:"Independent USD invoice reviewer",saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
  await expect(postInvoice(f.manager,String(invoice.id),{periodId:f.period,entryDate:"2026-09-10",expectedVersion:1},key())).rejects.toMatchObject({code:"MANUAL_RATE_REQUIRED"});
  await confirmManualRate(f.manager,{currency:"THB",effectiveDate:"2026-09-10",currencyPerUsd:"35",note:"Invoice-date manual rate"},key());
  await postInvoice(f.manager,String(invoice.id),{periodId:f.period,entryDate:"2026-09-10",expectedVersion:1},key());
  const payment=await createPayment(f.reviewer,{partyId:String(supplier.id),kind:"payment",paymentNumber:`USD-PAY-${key()}`,paymentDate:"2026-09-20",currency:"USD",amount:"100",bankAccount:ids.bank},key());
  await approvePayment(f.manager,String(payment.id),{expectedVersion:1,proof:"Independent USD obligation review"},key());await releasePayment(f.payer,String(payment.id),{expectedVersion:2,proof:"Treasury released the USD transfer"},key());
  await confirmManualRate(f.manager,{currency:"THB",effectiveDate:"2026-09-20",currencyPerUsd:"36",note:"Bank-date manual rate"},key());
  const statement=await importBankStatement(f.reviewer,{bankAccount:ids.bank,statementNumber:`USD-STMT-${key()}`,statementDate:"2026-12-31",opening:"0",closing:"-100",sourceHash:key(),rows:[{externalId:`USD-TX-${key()}`,transactionDate:"2026-09-20",amount:"-100",currency:"USD"}]},key()),bankId=String(await db.prepare("SELECT id FROM finance_bank_transactions WHERE statement_id=?").bind(statement.id).first("id"));
  const result=await confirmPayment(f.payer,String(payment.id),{expectedVersion:3,periodId:f.period,bankTransactionId:bankId,bankVersion:1,bankReference:"USD-WIRE",proof:"Treasury matched the exact USD bank debit",fxGainAccountId:ids.gain,fxLossAccountId:ids.loss,allocations:[{invoiceId:String(invoice.id),amount:"100"}]},key());
  expect(result).toMatchObject({status:"settled",realizedFx:"-100.00"});expect(result.fxJournalId).toBeTruthy();
  expect(await db.prepare("SELECT fx_rate_micros,base_total_minor,base_outstanding_minor,status FROM finance_invoices WHERE id=?").bind(invoice.id).first()).toMatchObject({fx_rate_micros:35_000_000,base_total_minor:350_000,base_outstanding_minor:0,status:"paid"});
  expect(await db.prepare("SELECT fx_rate_micros,base_amount_minor,fx_journal_id FROM finance_payments WHERE id=?").bind(payment.id).first()).toMatchObject({fx_rate_micros:36_000_000,base_amount_minor:360_000,fx_journal_id:result.fxJournalId});
  expect(await db.prepare("SELECT base_invoice_minor,base_payment_minor,realized_fx_minor FROM finance_allocations WHERE payment_id=?").bind(payment.id).first()).toMatchObject({base_invoice_minor:350_000,base_payment_minor:360_000,realized_fx_minor:10_000});
  const balance=Number(await db.prepare("SELECT COALESCE(SUM(l.base_debit_minor-l.base_credit_minor),0) balance FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE l.account_id=? AND j.status='posted'").bind(ids.payable).first("balance"));
  expect(balance).toBe(0);
  const fx=await db.prepare("SELECT account_id,SUM(base_debit_minor-base_credit_minor) balance FROM finance_journal_lines WHERE journal_id=? GROUP BY account_id").bind(result.fxJournalId).all<{account_id:string;balance:number}>();
  expect(Object.fromEntries(fx.results.map(row=>[row.account_id,Number(row.balance)]))).toEqual({[ids.payable]:-10_000,[ids.loss]:10_000});
  const period=await db.prepare("SELECT * FROM finance_periods WHERE id=?").bind(f.period).first<Record<string,string|number|null>>(),close=await calculateCloseReconciliations(db,f.owner,period!);
  expect(close.rows.find(row=>row.accountId===ids.payable)).toMatchObject({currency:"USD",ledgerMinor:0,sourceMinor:0,differenceMinor:0});
  expect(close.rows.find(row=>row.accountId===ids.bank)).toMatchObject({currency:"USD",ledgerMinor:-360_000,sourceMinor:-360_000,differenceMinor:0,source:expect.objectContaining({closingFxRateMicros:36_000_000,evidenceComplete:true})});
  const corrected=await reversePayment(f.manager,String(payment.id),{expectedVersion:4,periodId:f.period,entryDate:"2026-09-21",reason:"Supplier bank beneficiary was incorrect"},key());
  expect(corrected).toMatchObject({status:"voided",version:5});
  expect(await db.prepare("SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?").bind(invoice.id).first()).toMatchObject({status:"posted",outstanding_minor:10_000,base_outstanding_minor:350_000});
  expect(await db.prepare("SELECT match_status,matched_type,matched_id,version FROM finance_bank_transactions WHERE id=?").bind(bankId).first()).toMatchObject({match_status:"unmatched",matched_type:null,matched_id:null,version:3});
  expect(await db.prepare("SELECT status FROM finance_journals WHERE id=?").bind(result.fxJournalId).first("status")).toBe("reversed");
  expect(await db.prepare("SELECT fx_reversal_journal_id FROM finance_payment_reversals WHERE payment_id=?").bind(payment.id).first("fx_reversal_journal_id")).toBeTruthy();
 });
 it("rolls back journal, allocations, bank match, receipt and audit when final confirmation outbox fails, then retries and replays once",async()=>{
  const {supplier,invoice}=await payable("100"),payment=await createPayment(f.reviewer,{partyId:String(supplier.id),kind:"payment",paymentNumber:`ROLLBACK-${key()}`,paymentDate:"2026-09-12",currency:"THB",amount:"100",bankAccount:f.accounts.cash},key());
  await approvePayment(f.manager,String(payment.id),{expectedVersion:1,proof:"Independent rollback fixture review"},key());await releasePayment(f.payer,String(payment.id),{expectedVersion:2,proof:"Independent rollback fixture release"},key());
  const statement=await importBankStatement(f.reviewer,{bankAccount:f.accounts.cash,statementNumber:`ROLLBACK-BANK-${key()}`,statementDate:"2026-09-30",opening:"1000",closing:"900",sourceHash:key(),rows:[{externalId:`ROLLBACK-TX-${key()}`,transactionDate:"2026-09-12",amount:"-100",currency:"THB"}]},key()),bankId=String(await db.prepare("SELECT id FROM finance_bank_transactions WHERE statement_id=?").bind(statement.id).first("id")),command=key(),input={expectedVersion:3,periodId:f.period,bankTransactionId:bankId,bankVersion:1,bankReference:"ROLLBACK-CONFIRM",proof:"Treasury matched bank evidence before confirmation",allocations:[{invoiceId:String(invoice.id),amount:"100"}]},fault=`finance_confirmation_fault_${crypto.randomUUID().replaceAll("-","")}`;
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'FINANCE_CONFIRM_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='finance.payment.confirm') EXECUTE FUNCTION ${fault}();`);
  try {
   await expect(confirmPayment(f.payer,String(payment.id),input,command)).rejects.toThrow();
   expect(await db.prepare("SELECT status,version,outstanding_minor,journal_id FROM finance_payments WHERE id=?").bind(payment.id).first()).toMatchObject({status:"released",version:3,outstanding_minor:10000,journal_id:null});
   expect(await db.prepare("SELECT status,outstanding_minor FROM finance_invoices WHERE id=?").bind(invoice.id).first()).toMatchObject({status:"posted",outstanding_minor:10000});
   expect(await db.prepare("SELECT match_status,version,matched_id FROM finance_bank_transactions WHERE id=?").bind(bankId).first()).toEqual({match_status:"unmatched",version:1,matched_id:null});
   expect(Number(await db.prepare("SELECT count(*) n FROM finance_allocations WHERE payment_id=?").bind(payment.id).first("n"))).toBe(0);
   for(const table of ["command_receipts","audit_events","decision_evidence","outbox_events"])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first("n"))).toBe(0);
   expect(Number(await db.prepare("SELECT count(*) n FROM finance_journals WHERE id=?").bind(`journal-${command}`).first("n"))).toBe(0);
   expect(Number(await db.prepare("SELECT count(*) n FROM money_transaction_snapshots WHERE source_type='finance_payment' AND source_id=? AND amount_kind='settled'").bind(payment.id).first("n"))).toBe(0);
  } finally {await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
  const result=await confirmPayment(f.payer,String(payment.id),input,command);expect(result).toMatchObject({status:"settled",version:4});expect(await confirmPayment(f.payer,String(payment.id),input,command)).toEqual(result);
  for(const table of ["command_receipts","audit_events","decision_evidence","outbox_events"])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first("n"))).toBe(1);
  expect(Number(await db.prepare("SELECT count(*) n FROM finance_allocations WHERE payment_id=?").bind(payment.id).first("n"))).toBe(1);
 });

});

it("supplied Finance record reads and flow access cannot bypass purchase and revoked cached authority",async()=>{
 const {invoice}=await payable();vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");const id=String(invoice.id),item:WorkItem={id,type:"invoice",title:"Retained source",detail:"Source review",href:`/finance/invoices/${id}`,status:"posted",updatedAt:1,queue:"todo",canAct:true};
 await expect(getInvoice(f.owner,id,db)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(requireFlowAccess(f.owner,"invoice",id,db)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});expect(await enrichFlowWork(f.owner,[item])).toEqual([]);
 await financePackage();const detail=await invoiceRecordWorkspace(f.owner,id);expect(detail.invoice.id).toBe(id);expect(detail.options.invoices).toEqual([]);expect(detail.options.charts).toHaveLength(1);expect(detail.options.accounts.every(account=>account.chart_id===f.chart)).toBe(true);await requireFlowAccess(f.owner,"invoice",id,db);
 const stale=await enrichFlowWork(f.owner,[item]);await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(f.owner.assignmentId).run();await expect(getInvoice(f.owner,id,db)).rejects.toMatchObject({code:"FINANCE_ACCESS_CHANGED"});expect(await enrichFlowWork(f.owner,stale)).toEqual([]);
});
it("generic Finance history and cross-module reference search intersect purchase before the shared limit",async()=>{
 const actor={...f.owner,modules:["finance","projects"]},needle=`purchase-${key()}`,financeSection=findModule("finance")!.sections[0].id,projectSection=findModule("projects")!.sections[0].id,project=`record-${key()}`,ids:string[]=[];
 for(let index=0;index<16;index++){const id=`record-${key()}`;ids.push(id);await db.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,version,created_by,updated_by,created_at,updated_at) VALUES('finance',?,?,?,'Legacy retained preview','draft','[]',?,1,?,?,1,?)").bind(id,financeSection,`${needle} financial ${index}`,f.company,f.owner.userId,f.owner.userId,100+index).run();}
 await db.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,version,created_by,updated_by,created_at,updated_at) VALUES('projects',?,?,?,'Other authorized reference','draft','[]',?,1,?,?,1,1)").bind(project,projectSection,`${needle} project`,f.company,f.owner.userId,f.owner.userId).run();vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 for(const read of [()=>getRecord(actor,"finance",ids[0],db),()=>listRecords(actor,"finance",financeSection),()=>recordsPage(actor,"finance",financeSection,{}),()=>recordContext(actor,"finance",ids[0])])await expect(read()).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 const visible=await searchReferences(actor,needle);expect(visible).toEqual([expect.objectContaining({kind:"record",module_id:"projects",id:project})]);
 await financePackage();expect((await searchReferences(actor,needle)).filter(row=>row.kind==="record")).toHaveLength(12);expect((await getRecord(actor,"finance",ids[0],db)).id).toBe(ids[0]);await financePackage("draining");expect((await getRecord(actor,"finance",ids[0],db)).id).toBe(ids[0]);
 await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(actor.assignmentId).run();expect((await searchReferences(actor,needle)).some(row=>"module_id" in row&&row.module_id==="finance")).toBe(false);
});
it("financial Inbox and legacy notifications filter source access before limits and retain delivery identity through grant and revocation",async()=>{
 const {invoice}=await payable(),journal=await createJournal(f.reviewer,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:"2026-09-12",currency:"THB",memo:"Notification original journal",lines:[{accountId:f.accounts.expense,debit:"1",credit:"0"},{accountId:f.accounts.cash,debit:"0",credit:"1"}]}),key());
 await sendNotice(db,f.owner,{title:"Retained nonfinancial handoff",body:"The earlier visible notice stays available.",recipients:[f.manager.userId]},key());
 for(let index=0;index<105;index++)await publishInboxEvent(db,{key:key(),companyId:f.company,sourceType:index%2?"journal":"invoice",sourceId:String(index%2?journal.id:invoice.id),sourceVersion:1,category:"work",preview:"Financial source review",recipientIds:[f.manager.userId],createdAt:Date.now()+index});
 const delivery=`financial-notice-${key()}`;await db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'invoice',?,1,'Retained invoice review',?)").bind(delivery,f.company,f.manager.userId,invoice.id,Date.now()).run();
 vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");expect((await listStaffNotifications(db,f.manager)).filter(row=>row.record_type==="invoice"||row.record_type==="journal")).toEqual([]);const page=await inboxPage(db,f.manager,{view:"all",limit:10});expect(page.items.some(item=>item.message.includes("Retained nonfinancial handoff"))).toBe(true);expect(page.items.some(item=>item.sourceType==="invoice"||item.sourceType==="journal")).toBe(false);expect((await inboxSummary(db,f.manager)).categories.work).toBe(0);
 await financePackage();const visible=await listStaffNotifications(db,f.manager);expect(visible.find(row=>row.id===delivery)).toMatchObject({record_version:1,source:{kind:"invoice",id:invoice.id,version:2}});expect((await notificationsPage(db,f.manager,{view:"all"})).items.some(row=>row.id===delivery)).toBe(true);expect((await inboxPage(db,f.manager,{view:"all",limit:10})).items.some(item=>item.sourceType==="invoice"||item.sourceType==="journal")).toBe(true);
 await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(f.manager.assignmentId).run();expect((await visibleNotifications(db,f.manager,visible)).filter(row=>row.record_type==="invoice"||row.record_type==="journal")).toEqual([]);expect((await inboxPage(db,f.manager,{view:"all"})).items.some(item=>item.sourceType==="invoice"||item.sourceType==="journal")).toBe(false);expect(await db.prepare("SELECT record_id,record_version,read_at FROM staff_notifications WHERE id=?").bind(delivery).first()).toEqual({record_id:invoice.id,record_version:1,read_at:null});
});
