import {beforeAll,beforeEach,describe,expect,it,vi} from "vitest";
import {applyMigrations,env} from "./runtime-env";
import {expenseDb as db,expenseFixture,key} from "./helpers/expense-fixture";
import {approvePayment,confirmPayment,createInvoice,createJournal,createParty,createPayment,importBankStatement,postInvoice,postJournal,releasePayment} from "../src/server/finance";
import {confirmManualRate} from "../src/server/company-money";
import {assignFlowPrimary,resolveFlow} from "../src/server/flow-routing";
import {approveRefund,confirmRefundInstruction,exportRefundInstructions,prepareRefundInstruction,recoverRefundExport,refundConfirmationSchema,refundInstructionSchema,refundWorkspace} from "../src/server/refunds";
import {invoiceHistoryCte} from "../src/server/finance-history";

vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();f.head.role="finance_approver";f.head.branchId=null;f.head.departmentId=null;f.head.assignmentId=await f.grant(f.head,"finance_approver");});

async function assign(kind:"invoice"|"journal",id:string){const flow=await resolveFlow(db,f.owner,kind,id);await assignFlowPrimary(db,f.owner,kind,id,{stepKey:"draft",userId:f.manager.userId,reason:"Independent FX source review",saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());}
async function source(direction:"incoming"|"outgoing",carryingRate="35") {
 const ids={control:`refund-control-${key()}`,trade:`refund-trade-${key()}`,bank:`refund-bank-${key()}`,gain:`refund-gain-${key()}`,loss:`refund-loss-${key()}`},incoming=direction==="incoming";
 for(const [name,id] of Object.entries(ids))await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,1,1)").bind(id,f.chart,f.company,id,name,name==="control"?(incoming?"liability":"asset"):name==="trade"?(incoming?"expense":"revenue"):name==="bank"?"asset":name==="gain"?"other_income":"other_expense",name==="gain"||name==="loss"?"THB":"USD",name==="control"||name==="bank"?1:0,name==="control"?(incoming?"payable":"receivable"):name==="bank"?"bank":"none").run();
 await db.prepare("UPDATE commerce_settings SET cash_account_id=? WHERE company_id=?").bind(ids.bank,f.company).run();
 await confirmManualRate(f.manager,{currency:"THB",effectiveDate:"2026-09-01",currencyPerUsd:carryingRate,note:"Synthetic source-date manual quote"},key());
 const party=await createParty(f.reviewer,{kind:incoming?"supplier":"customer",code:key(),name:"Synthetic foreign refund counterparty",currency:"USD",paymentTermsDays:0,creditLimit:"1000"},key());
 const invoice=await createInvoice(f.reviewer,{partyId:String(party.id),kind:incoming?"payable":"receivable",invoiceNumber:key(),invoiceDate:"2026-09-01",dueDate:"2026-09-02",currency:"USD",subtotal:"100",tax:"0",debitAccountId:incoming?ids.trade:ids.control,creditAccountId:incoming?ids.control:ids.trade},key());
 await assign("invoice",String(invoice.id));await postInvoice(f.manager,String(invoice.id),{expectedVersion:1,periodId:f.period,entryDate:"2026-09-01"},key());
 const payment=await createPayment(f.reviewer,{partyId:String(party.id),kind:incoming?"payment":"receipt",paymentNumber:key(),paymentDate:"2026-09-02",currency:"USD",amount:"100",bankAccount:ids.bank},key());
 await approvePayment(f.manager,String(payment.id),{expectedVersion:1,proof:"Independent source liability review"},key());await releasePayment(f.payer,String(payment.id),{expectedVersion:2,proof:"Treasury source cash release"},key());
 const statement=await importBankStatement(f.reviewer,{bankAccount:ids.bank,statementNumber:key(),statementDate:"2026-09-02",opening:"0",closing:incoming?"-100":"100",sourceHash:key(),rows:[{externalId:key(),transactionDate:"2026-09-02",amount:incoming?"-100":"100",currency:"USD"}]},key()),bankId=String(await db.prepare("SELECT id FROM finance_bank_transactions WHERE statement_id=?").bind(statement.id).first("id"));
 await confirmPayment(f.payer,String(payment.id),{expectedVersion:3,periodId:f.period,bankTransactionId:bankId,bankVersion:1,bankReference:key(),proof:"Exact synthetic original settlement",allocations:[{invoiceId:String(invoice.id),amount:"100"}]},key());
 const returnId=`refund-fixture-return-${key()}`,credit=await createJournal(f.reviewer,{chartId:f.chart,periodId:f.period,entryDate:"2026-09-03",currency:"USD",fxRate:carryingRate,memo:"Synthetic Commerce source adapter retained credit",sourceType:"commerce_return",sourceId:returnId,isOpening:false,lines:[{accountId:ids.control,debit:incoming?"100":"0",credit:incoming?"0":"100"},{accountId:ids.trade,debit:incoming?"0":"100",credit:incoming?"100":"0"}]},key());
 await assign("journal",credit.id);await postJournal(f.manager,credit.id,key(),{expectedVersion:1});
 // This fixture models a retained Commerce handoff. Commerce's owner tests
 // separately exercise the goods, return approval and source posting commands.
 const warehouse=`fixture-warehouse-${key()}`,order=`fixture-order-${key()}`,fulfillment=`fixture-fulfillment-${key()}`,obligation=`fixture-refund-${key()}`,base=Math.round(Number(carryingRate)*10_000);
 await db.batch([
  db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES(?,?,?,?, 'Synthetic refund handoff warehouse')").bind(warehouse,f.company,f.branch,warehouse),
  db.prepare("INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'USD','2026-09-01','2026-09-02','fulfilled',10000,0,?,1,1)").bind(order,f.company,f.branch,incoming?"purchase":"sales",order,party.id,warehouse,f.owner.userId),
  db.prepare("INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,invoice_id) VALUES(?,?,?,'2026-09-01',?,1,?)").bind(fulfillment,order,fulfillment,f.owner.userId,invoice.id),
  db.prepare("INSERT INTO commerce_returns(id,company_id,branch_id,order_id,fulfillment_id,invoice_id,kind,number,return_date,reason,status,subtotal_minor,total_minor,credited_minor,refund_minor,base_credit_minor,base_credited_minor,base_refund_minor,credit_journal_id,requested_by,approved_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?, '2026-09-03','Synthetic retained owner handoff','posted',10000,10000,0,10000,?,0,?,?,?,?,1,1)").bind(returnId,f.company,f.branch,order,fulfillment,invoice.id,incoming?"supplier_return":"customer_return",returnId,base,base,credit.id,f.owner.userId,f.manager.userId),
  db.prepare("INSERT INTO finance_refund_obligations(id,company_id,branch_id,return_id,invoice_id,party_id,direction,currency,chart_id,balance_account_id,amount_minor,outstanding_minor,base_amount_minor,base_outstanding_minor,source_credit_journal_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'USD',?,?,10000,10000,?,?,?, ?,1,1)").bind(obligation,f.company,f.branch,returnId,invoice.id,party.id,direction,f.chart,ids.control,base,base,credit.id,f.manager.userId),
 ]);
 await approveRefund(f.head,obligation,{expectedVersion:1,reason:"Independently reviewed retained credit and counterparty",beneficiaryName:"Synthetic counterparty",beneficiaryAccount:"FIXTURE-USD-001"},key());
 return {ids,obligation,invoiceId:String(invoice.id),base};
}
async function confirm(obligation:string,ids:Awaited<ReturnType<typeof source>>["ids"],amount:string,date="2026-09-05") {
 const owner=(await refundWorkspace(f.payer,obligation)).owner;
 const prepared=await prepareRefundInstruction(f.payer,obligation,refundInstructionSchema.parse({expectedVersion:Number(owner.version),amount,plannedDate:date,bankAccount:ids.bank,cashAccountId:ids.bank,fxGainAccountId:ids.gain,fxLossAccountId:ids.loss}),key());
 const exportKey=key(),exported=await exportRefundInstructions(f.payer,{instructions:[{id:String(prepared.id),expectedVersion:1}]},exportKey);
 expect(await recoverRefundExport(f.payer,String(prepared.id))).toMatchObject({sha256:exported.sha256,content:exported.content});
 const outgoing=owner.direction==="outgoing",statement=await importBankStatement(f.reviewer,{bankAccount:ids.bank,statementNumber:key(),statementDate:date,opening:"0",closing:outgoing?`-${amount}`:amount,sourceHash:key(),rows:[{externalId:key(),transactionDate:date,amount:outgoing?`-${amount}`:amount,currency:"USD"}]},key()),bankId=String(await db.prepare("SELECT id FROM finance_bank_transactions WHERE statement_id=?").bind(statement.id).first("id"));
 const input=refundConfirmationSchema.parse({expectedVersion:2,bankTransactionId:bankId,bankVersion:1,periodId:f.period,bankReference:key(),evidence:"Treasury checked exact dated synthetic bank evidence"}),command=key(),result=await confirmRefundInstruction(f.payer,String(prepared.id),input,command);
 expect(await confirmRefundInstruction(f.payer,String(prepared.id),input,command)).toEqual(result);
 return {...result,instructionId:String(prepared.id),exported};
}

describe("foreign refund cash and carrying evidence",()=>{
 it.each(["incoming","outgoing"] as const)("settles partial and final %s refunds with exact source carrying, bank-date cash and immutable FX",async(direction)=>{
  const s=await source(direction);await confirmManualRate(f.manager,{currency:"THB",effectiveDate:"2026-09-05",currencyPerUsd:"36",note:"Synthetic bank-date quote"},key());
  const first=await confirm(s.obligation,s.ids,"40.01"),last=await confirm(s.obligation,s.ids,"59.99");
  expect(first).toMatchObject({baseCarryingMinor:140035,cashBaseMinor:144036,realizedFxMinor:direction==="incoming"?4001:-4001});
  expect(last).toMatchObject({baseCarryingMinor:209965,cashBaseMinor:215964,realizedFxMinor:direction==="incoming"?5999:-5999});
  expect((await refundWorkspace(f.payer,s.obligation)).owner).toMatchObject({status:"settled",outstanding_minor:0,base_outstanding_minor:0});
  const history=await db.prepare(`${invoiceHistoryCte} SELECT historical_outstanding_minor,historical_base_outstanding_minor FROM historical_invoice_balances WHERE id=?`).bind(f.company,"2026-09-05",s.invoiceId).first();expect(history).toEqual({historical_outstanding_minor:0,historical_base_outstanding_minor:0});
  const control=await db.prepare("SELECT SUM(l.base_debit_minor-l.base_credit_minor) balance FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.status IN ('posted','reversed') AND l.account_id=?").bind(f.company,s.ids.control).first("balance");expect(control).toBe(0);
  const gains=await db.prepare("SELECT SUM(base_credit_minor-base_debit_minor) balance FROM finance_journal_lines WHERE account_id=?").bind(direction==="incoming"?s.ids.gain:s.ids.loss).first("balance");expect(gains).toBe(direction==="incoming"?10000:-10000);
  expect(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE company_id=? AND source_type='return_refund_fx'").bind(f.company).first("n")).toBe(2);
  expect(await db.prepare("SELECT COUNT(*) n FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_payment' AND source_id IN (?,?)").bind(f.company,first.paymentId,last.paymentId).first("n")).toBe(2);
  await expect(db.prepare("UPDATE finance_refund_instructions SET base_carrying_minor=0 WHERE id=?").bind(first.instructionId).run()).rejects.toThrow("REFUND_CONFIRMATION_EVIDENCE_IMMUTABLE");
  await expect(db.prepare("UPDATE finance_refund_instructions SET export_hash='tampered' WHERE id=?").bind(first.instructionId).run()).rejects.toThrow("REFUND_EXPORT_EVIDENCE_IMMUTABLE");
  await expect(refundWorkspace({...f.payer,companyId:"other-company"},s.obligation)).rejects.toMatchObject({status:404});
 });

 it("allocates cumulative carrying rounding exactly and rejects unreviewed FX mapping without moving cash",async()=>{
  const s=await source("incoming","35.123456");await confirmManualRate(f.manager,{currency:"THB",effectiveDate:"2026-09-05",currencyPerUsd:"36",note:"Synthetic bank-date quote"},key());
  const first=await confirm(s.obligation,s.ids,"0.01"),last=await confirm(s.obligation,s.ids,"99.99");
  expect(Number(first.baseCarryingMinor)+Number(last.baseCarryingMinor)).toBe(s.base);
  const other=await source("incoming"),owner=(await refundWorkspace(f.payer,other.obligation)).owner;
  await expect(prepareRefundInstruction(f.payer,other.obligation,refundInstructionSchema.parse({expectedVersion:Number(owner.version),amount:"100",plannedDate:"2026-09-05",bankAccount:other.ids.bank,cashAccountId:other.ids.bank,fxGainAccountId:other.ids.loss}),key())).rejects.toMatchObject({code:"FX_ACCOUNT_INVALID"});
  expect((await refundWorkspace(f.payer,other.obligation)).owner.outstanding_minor).toBe(10000);
 });

 it("rolls back refund cash FX carrying snapshots and notification delivery if final outbox fails then retries exactly once",async()=>{
  const sourceFacts=await source("incoming");await confirmManualRate(f.manager,{currency:"THB",effectiveDate:"2026-09-05",currencyPerUsd:"36",note:"Synthetic refund bank-date quote for rollback proof"},key());
  const owner=(await refundWorkspace(f.payer,sourceFacts.obligation)).owner,instruction=await prepareRefundInstruction(f.payer,sourceFacts.obligation,refundInstructionSchema.parse({expectedVersion:Number(owner.version),amount:"40.01",plannedDate:"2026-09-05",bankAccount:sourceFacts.ids.bank,cashAccountId:sourceFacts.ids.bank,fxGainAccountId:sourceFacts.ids.gain,fxLossAccountId:sourceFacts.ids.loss}),key()),instructionId=String(instruction.id),exported=await exportRefundInstructions(f.payer,{instructions:[{id:instructionId,expectedVersion:1}]},key());
  const statement=await importBankStatement(f.reviewer,{bankAccount:sourceFacts.ids.bank,statementNumber:key(),statementDate:"2026-09-05",opening:"0",closing:"40.01",sourceHash:key(),rows:[{externalId:key(),transactionDate:"2026-09-05",amount:"40.01",currency:"USD"}]},key()),bankId=String(await db.prepare("SELECT id FROM finance_bank_transactions WHERE statement_id=?").bind(statement.id).first("id")),command=key(),input=refundConfirmationSchema.parse({expectedVersion:2,bankTransactionId:bankId,bankVersion:1,periodId:f.period,bankReference:key(),evidence:"Reviewed exact bank amount for isolated rollback proof"}),before=(await refundWorkspace(f.payer,sourceFacts.obligation)).owner,fault=`refund_confirm_fault_${crypto.randomUUID().replaceAll("-","")}`;
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'REFUND_CONFIRM_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='finance.refund.confirm') EXECUTE FUNCTION ${fault}();`);
  try{
   await expect(confirmRefundInstruction(f.payer,instructionId,input,command)).rejects.toThrow();
   expect((await refundWorkspace(f.payer,sourceFacts.obligation)).owner).toEqual(before);
   expect(await db.prepare("SELECT status,version,base_carrying_minor,cash_base_minor FROM finance_refund_instructions WHERE id=?").bind(instructionId).first()).toEqual({status:"exported",version:2,base_carrying_minor:null,cash_base_minor:null});
   expect(await db.prepare("SELECT match_status,version,matched_id FROM finance_bank_transactions WHERE id=?").bind(bankId).first()).toEqual({match_status:"unmatched",version:1,matched_id:null});
   expect(Number(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE source_type IN ('return_refund','return_refund_fx') AND source_id=?").bind(instructionId).first("n"))).toBe(0);
   expect(Number(await db.prepare("SELECT COUNT(*) n FROM finance_payments WHERE id=?").bind(`refund-payment-${command}`).first("n"))).toBe(0);
   expect(Number(await db.prepare("SELECT COUNT(*) n FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_payment' AND source_id=?").bind(f.company,`refund-payment-${command}`).first("n"))).toBe(0);
   for(const table of ["command_receipts","audit_events","outbox_events"])expect(Number(await db.prepare(`SELECT COUNT(*) n FROM ${table} WHERE command_id=?`).bind(command).first("n"))).toBe(0);
   expect(Number(await db.prepare("SELECT COUNT(*) n FROM staff_notifications WHERE id LIKE ?").bind(`${command}-%`).first("n"))).toBe(0);
  }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
  const result=await confirmRefundInstruction(f.payer,instructionId,input,command);expect(await confirmRefundInstruction(f.payer,instructionId,input,command)).toEqual(result);expect(result).toMatchObject({status:"confirmed",version:3,baseCarryingMinor:140035,cashBaseMinor:144036,realizedFxMinor:4001});
  for(const table of ["command_receipts","audit_events","outbox_events"])expect(Number(await db.prepare(`SELECT COUNT(*) n FROM ${table} WHERE command_id=?`).bind(command).first("n"))).toBe(1);
  expect(Number(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE source_type IN ('return_refund','return_refund_fx') AND source_id=?").bind(instructionId).first("n"))).toBe(2);
  expect(Number(await db.prepare("SELECT COUNT(*) n FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_payment' AND source_id=?").bind(f.company,`refund-payment-${command}`).first("n"))).toBe(1);
  expect(await recoverRefundExport(f.payer,instructionId)).toMatchObject({content:exported.content,sha256:exported.sha256});
 });
});
