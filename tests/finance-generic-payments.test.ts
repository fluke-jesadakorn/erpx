import { beforeAll,beforeEach,describe,expect,it,vi } from "vitest";
import { applyMigrations,env,type TestMigration } from "./runtime-env";
import { expenseDb as db,expenseFixture,key } from "./helpers/expense-fixture";
import { approvePayment,bankWorkspace,calculateCloseReconciliations,confirmPayment,createInvoice,createParty,createPayment,importBankStatement,invoicesPage,postInvoice,releasePayment,reversePayment } from "../src/server/finance";
import { confirmManualRate } from "../src/server/company-money";
import { assignFlowPrimary,resolveFlow } from "../src/server/flow-routing";

vi.mock("server-only",()=>({}));
vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));

let f:Awaited<ReturnType<typeof expenseFixture>>;

beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS as TestMigration[]);});
beforeEach(async()=>{f=await expenseFixture();await db.batch([db.prepare("UPDATE finance_accounts SET is_control=1,control_kind='bank' WHERE id=?").bind(f.accounts.cash),db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,'asset','THB',1,'receivable',1,1)").bind(`${f.company}-receivable`,f.chart,f.company,"receivable","Trade receivables")]);});

async function payable(amount="100"){
 const supplier=await createParty(f.reviewer,{kind:"supplier",code:`SUP-${key()}`,name:"Controlled payment supplier",currency:"THB",paymentTermsDays:30,creditLimit:"0"},key());
 const invoice=await createInvoice(f.reviewer,{partyId:String(supplier.id),kind:"payable",invoiceNumber:`BILL-${key()}`,invoiceDate:"2026-09-10",dueDate:"2026-10-10",currency:"THB",subtotal:amount,tax:"0",debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable},key());
 const flow=await resolveFlow(db,f.owner,"invoice",String(invoice.id));
 await assignFlowPrimary(db,f.owner,"invoice",String(invoice.id),{stepKey:"draft",userId:f.manager.userId,reason:"Independent payment acceptance reviewer",saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
 await postInvoice(f.manager,String(invoice.id),{periodId:f.period,entryDate:"2026-09-10",expectedVersion:1},key());
 return {supplier,invoice};
}

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
