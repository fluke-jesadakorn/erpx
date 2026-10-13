import { beforeAll,beforeEach,expect,it,vi } from "vitest";
import { applyMigrations,env } from "../../support/runtime-env";
import { expenseDb as db,expenseFixture,key } from "../../support/helpers/expense-fixture";
import { createAccount,createAccountSchema,createPeriod,createPeriodSchema,createInvoice,invoiceSchema,postInvoice,reversePayment } from "../../../src/server/modules/finance/public/finance";
import { settleInvoice } from "../../../src/server/modules/commerce/public/commerce";
import { assignFlowPrimary,resolveFlow } from "../../../src/server/modules/workflow/public/flow-routing";

vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));

let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{
  f=await expenseFixture();
  for(const [actor,role] of [[f.reviewer,"finance_preparer"],[f.manager,"finance_approver"],[f.payer,"treasury"]] as const){
    actor.role=role;actor.branchId=null;actor.departmentId=null;
    actor.assignmentId=(await db.prepare("SELECT id FROM assignments WHERE company_id=? AND user_id=? AND role=? AND revoked_at IS NULL").bind(f.company,actor.userId,role).first<{id:string}>())!.id;
  }
});

it("reverses a settled payment atomically and restores its invoice allocation",async()=>{
  const party=`customer-${key()}`;
  await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Correction customer','THB',1,1)").bind(party,f.company,party).run();
  const invoice=await createInvoice(f.reviewer,invoiceSchema.parse({partyId:party,kind:"receivable",invoiceNumber:key(),invoiceDate:"2026-09-01",dueDate:"2026-09-03",currency:"THB",subtotal:"100",tax:"0",debitAccountId:f.accounts.cash,creditAccountId:f.accounts.revenue}),key());
  const flow=await resolveFlow(db,f.owner,"invoice",invoice.id);
  await assignFlowPrimary(db,f.owner,"invoice",invoice.id,{stepKey:"draft",userId:f.manager.userId,reason:"Independent correction fixture accountant",saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
  await postInvoice(f.manager,invoice.id,{expectedVersion:1,periodId:f.period,entryDate:"2026-09-02"},key());
  const settlement=await settleInvoice(f.payer,invoice.id,{expectedVersion:2,date:"2026-09-03",amount:"100",reference:key()},key());
  const command=key(),input={expectedVersion:1,periodId:f.period,entryDate:"2026-09-04",reason:"Bank confirmed that the receipt was recorded against the wrong customer"};
  const corrected=await reversePayment(f.manager,String(settlement.paymentId),input,command);
  expect(await reversePayment(f.manager,String(settlement.paymentId),input,command)).toEqual(corrected);
  expect(await db.prepare("SELECT status,version FROM finance_payments WHERE id=?").bind(settlement.paymentId).first()).toEqual({status:"voided",version:2});
  expect(await db.prepare("SELECT outstanding_minor,status FROM finance_invoices WHERE id=?").bind(invoice.id).first()).toEqual({outstanding_minor:10000,status:"posted"});
  const journals=await db.prepare("SELECT source_type,status FROM finance_journals WHERE id=(SELECT journal_id FROM finance_payments WHERE id=?) OR source_id=? ORDER BY source_type").bind(settlement.paymentId,settlement.paymentId).all();
  expect(journals.results).toEqual(expect.arrayContaining([{source_type:"commerce_payment",status:"reversed"},{source_type:"payment_reversal",status:"posted"}]));
  const balance=await db.prepare("SELECT SUM(debit_minor) debit,SUM(credit_minor) credit FROM finance_journal_lines WHERE journal_id=?").bind(corrected.reversalJournalId).first();
  expect(balance).toEqual({debit:10000,credit:10000});
  expect(await db.prepare("SELECT reason FROM finance_payment_reversals WHERE payment_id=?").bind(settlement.paymentId).first("reason")).toContain("wrong customer");
});

it("records partial receipts in the selected monthly period and primary bank while an annual period is open",async()=>{
  const bank=await createAccount(f.reviewer,createAccountSchema.parse({chartId:f.chart,code:"1100",name:"Reviewed operating bank",category:"asset",currency:"THB",controlKind:"bank"}),key());
  const monthly=await createPeriod(f.reviewer,createPeriodSchema.parse({name:"September receipts",startDate:"2026-09-01",endDate:"2026-09-30"}),key());
  const party=`customer-${key()}`;
  await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Partial receipt customer','THB',1,1)").bind(party,f.company,party).run();
  const invoice=await createInvoice(f.reviewer,invoiceSchema.parse({partyId:party,kind:"receivable",invoiceNumber:key(),invoiceDate:"2026-09-01",dueDate:"2026-09-10",currency:"THB",subtotal:"100.01",tax:"0",debitAccountId:f.accounts.cash,creditAccountId:f.accounts.revenue}),key());
  const flow=await resolveFlow(db,f.owner,"invoice",invoice.id);
  await assignFlowPrimary(db,f.owner,"invoice",invoice.id,{stepKey:"draft",userId:f.manager.userId,reason:"Independent accountant",saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
  await postInvoice(f.manager,invoice.id,{expectedVersion:1,periodId:monthly.id,entryDate:"2026-09-01"},key());
  const input={expectedVersion:2,date:"2026-09-15",amount:"40.01",reference:"Fictional partial receipt",bankAccountId:bank.id};
  await expect(settleInvoice(f.payer,invoice.id,input,key())).rejects.toMatchObject({code:"PERIOD_UNAVAILABLE"});
  await expect(settleInvoice(f.payer,invoice.id,{...input,periodId:monthly.id,bankAccountId:f.accounts.expense},key())).rejects.toMatchObject({code:"BANK_ACCOUNT_INVALID"});
  const foreign=await expenseFixture();
  await expect(settleInvoice(f.payer,invoice.id,{...input,periodId:foreign.period},key())).rejects.toMatchObject({code:"PERIOD_UNAVAILABLE"});
  const command=key(),reviewed={...input,periodId:monthly.id};
  const first=await settleInvoice(f.payer,invoice.id,reviewed,command);
  expect(await settleInvoice(f.payer,invoice.id,reviewed,command)).toEqual(first);
  expect(first.outstandingMinor).toBe(6000);
  await settleInvoice(f.payer,invoice.id,{...reviewed,expectedVersion:3,amount:"60.00",reference:"Fictional remaining receipt"},key());
  expect(await db.prepare("SELECT status,outstanding_minor FROM finance_invoices WHERE id=?").bind(invoice.id).first()).toEqual({status:"paid",outstanding_minor:0});
  expect((await db.prepare("SELECT p.amount_minor,p.bank_account,j.period_id FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id WHERE p.company_id=? ORDER BY p.amount_minor").bind(f.company).all()).results).toEqual([{amount_minor:4001,bank_account:bank.id,period_id:monthly.id},{amount_minor:6000,bank_account:bank.id,period_id:monthly.id}]);
  expect(await db.prepare("SELECT SUM(l.debit_minor-l.credit_minor) balance FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND l.account_id=? AND j.source_type='commerce_payment'").bind(f.company,bank.id).first("balance")).toBe(10001);
});
