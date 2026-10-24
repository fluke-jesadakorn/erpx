import {reviewedSecondaryBook} from "../../support/helpers/finance-book-fixture";
import {calendarData} from "@erpx/server-work_projection/public/calendar/index";
import {afterEach} from "vitest";
import {reviewedFinanceGrant} from "../../support/helpers/finance-purchase-fixture";
import { beforeAll,beforeEach,expect,it,vi } from 'vitest';
import { applyMigrations,env } from "../../support/runtime-env";
import { expenseDb as db,expenseFixture,key } from "../../support/helpers/expense-fixture";
import {settleInvoice} from '@erpx/server-commerce/public/commerce';
import { listStaffNotifications } from "@erpx/server-collaboration/public/staff-notifications";
import {createInvoice,invoiceSchema,postInvoice,importBankStatement,beginClose} from '@erpx/server-finance/public/finance';
import {assignFlowPrimary,resolveFlow} from '@erpx/server-workflow/public/flow-routing';
import { createSupplierBatch,decideSupplierBatch,exportSupplierBatch,recoverSupplierExport,recordSupplierResult,confirmSupplierInstruction,supplierBatchWorkspace,supplierBatchWork } from "@erpx/server-procurement/public/supplier-payments";
import {attachmentBindingState,enableAttachmentV2ForTest,seedAttachmentSet} from "../../support/helpers/attachment-v2";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{enableAttachmentV2ForTest();await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{
 f=await expenseFixture();
 for(const [actor,role] of [[f.reviewer,'finance_preparer'],[f.manager,'finance_approver'],[f.payer,'treasury']] as const){actor.role=role;actor.branchId=null;actor.departmentId=null;actor.assignmentId=await f.grant(actor,role);}
});
async function invoice(amount='100') {
 const party=`supplier-${key()}`;
 await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'supplier',?,'Simulation supplier','THB',1,1)").bind(party,f.company,party).run();
 const result=await createInvoice(f.reviewer,invoiceSchema.parse({partyId:party,kind:'payable',invoiceNumber:key(),invoiceDate:'2026-09-01',dueDate:'2026-09-10',currency:'THB',subtotal:amount,tax:'0',debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable}),key());
 const flow=await resolveFlow(db,f.owner,'invoice',result.id);
 await assignFlowPrimary(db,f.owner,'invoice',result.id,{stepKey:'draft',userId:f.manager.userId,reason:'Simulation independent invoice approval',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());
 await postInvoice(f.manager,result.id,{expectedVersion:1,periodId:f.period,entryDate:'2026-09-02'},key());return result.id;
}
const entry=(id:string,amount='100',expectedVersion=2)=>({invoiceId:id,expectedVersion,amount,beneficiaryName:'=Unsafe formula supplier',beneficiaryAccount:'SIMULATED-ACCOUNT-001'});
const input=(items:ReturnType<typeof entry>[])=>({bankAccountId:f.accounts.cash,plannedDate:'2026-09-03',items});
async function bank(amount='100') {
 const reference=key();
 await importBankStatement(f.reviewer,{bankAccount:f.accounts.cash,statementNumber:key(),statementDate:'2026-09-04',opening:amount,closing:'0',sourceHash:key(),rows:[{externalId:reference,transactionDate:'2026-09-04',amount:`-${amount}`,currency:'THB',description:'Simulated supplier bank confirmation'}]},key());
 return (await db.prepare('SELECT id,version FROM finance_bank_transactions WHERE company_id=? AND external_id=?').bind(f.company,reference).first<{id:string;version:number}>())!;
}
it('reserves, independently approves and confirms a partial multi-invoice batch with source accounting',async()=>{
 const first=await invoice(),second=await invoice('50'),command=key(),draft=await createSupplierBatch(f.reviewer,input([entry(first,'60'),entry(second,'50')]),command),id=String(draft.id);
 expect(await createSupplierBatch(f.reviewer,input([entry(first,'60'),entry(second,'50')]),command)).toEqual(draft);
 expect((await listStaffNotifications(db,f.manager)).some(n=>n.record_type==='supplier_batch'&&n.record_id===id)).toBe(true);
 expect((await listStaffNotifications(db,{...f.manager,branchId:f.branch})).some(n=>n.record_type==='supplier_batch')).toBe(false);
 const invoiceFlow=await resolveFlow(db,f.owner,'invoice',first);
 await assignFlowPrimary(db,f.owner,'invoice',first,{stepKey:'posted',userId:f.payer.userId,reason:'Test source settlement guard',saveDefault:false,expectedVersion:invoiceFlow.version,recordVersion:invoiceFlow.recordVersion},key());
 await expect(settleInvoice(f.payer,first,{expectedVersion:3,date:'2026-09-03',amount:'100',reference:'Concurrent manual payment',paymentSlipId:await f.attach(first,'invoice_payment')},key())).rejects.toMatchObject({code:'CONFLICT'});

 await expect(createSupplierBatch(f.reviewer,input([entry(first,'61')]),command)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
 await expect(supplierBatchWorkspace({...f.payer,branchId:f.branch},id)).rejects.toMatchObject({code:'COMPANY_TREASURY_REQUIRED'});
 await expect(supplierBatchWorkspace({...f.payer,companyId:'other'},id)).rejects.toMatchObject({code:'BATCH_NOT_FOUND'});
 await expect(decideSupplierBatch(f.owner,id,{expectedVersion:1,reason:'Owner independent approval'},key(),'approve')).resolves.toMatchObject({status:'approved'});
 // A preparer cannot become the payer by switching to an authorized role.
 await expect(exportSupplierBatch({...f.reviewer,role:'holding_owner'},id,{expectedVersion:2},key())).rejects.toMatchObject({code:'INDEPENDENT_PAYER_REQUIRED'});
 const exportKey=key(),output=await exportSupplierBatch(f.payer,id,{expectedVersion:2},exportKey);
 expect(await exportSupplierBatch(f.payer,id,{expectedVersion:2},exportKey)).toEqual(output);
 expect(output.content).toContain("'=Unsafe formula supplier");
 expect(await recoverSupplierExport(f.payer,id)).toMatchObject({content:output.content,sha256:output.sha256});
 const {calendarData}=await import("@erpx/server-work_projection/public/calendar/index");
 const calendar=await calendarData(db,f.payer,{date:'2026-09-03',view:'day',layers:'supplier_batches',mode:'mine'});
 expect(calendar.items.some(item=>item.sourceId===id)).toBe(true);
 expect((await calendarData(db,{...f.payer,branchId:f.branch},{date:'2026-09-03',view:'day',layers:'supplier_batches',mode:'team'})).items).toEqual([]);

 expect(await db.prepare("SELECT count(*) count FROM finance_payments WHERE company_id=?").bind(f.company).first('count')).toBe(0);
 await expect(beginClose(f.owner,f.period,key())).rejects.toMatchObject({code:'CLOSE_NOT_READY'});
 const workspace=await supplierBatchWorkspace(f.payer,id),item=workspace.items.find(i=>i.invoice_id===first)!,other=workspace.items.find(i=>i.invoice_id===second)!;
 const proof=await seedAttachmentSet(db,f.payer,{targetType:'finance_supplier_batch',targetId:id,componentKind:'action',componentRef:`supplier-instruction:${String(item.id)}`,purposeKey:'payment_proof',expectedTargetVersion:Number(workspace.batch!.version)});
 const bankRow=await bank('60'),confirmation={expectedVersion:2,bankTransactionId:bankRow.id,bankVersion:bankRow.version,periodId:f.period,bankReference:'SIM-BANK-60',evidence:'Bank statement amount and approved beneficiary verified',attachmentSetIds:[proof.setId]},confirmKey=key();
 const confirmed=await confirmSupplierInstruction(f.payer,String(item.id),confirmation,confirmKey);
 expect(await confirmSupplierInstruction(f.payer,String(item.id),confirmation,confirmKey)).toEqual(confirmed);
 expect(await attachmentBindingState(db,proof.setId)).toMatchObject({set_state:'bound',link_state:'bound',binding_type:'finance_supplier_payment_confirmation'});
 expect(await db.prepare('SELECT outstanding_minor,status FROM finance_invoices WHERE id=?').bind(first).first()).toEqual({outstanding_minor:4000,status:'partially_paid'});
 expect(await db.prepare('SELECT outstanding_minor,status FROM finance_invoices WHERE id=?').bind(second).first()).toEqual({outstanding_minor:5000,status:'posted'});
 expect(await db.prepare('SELECT SUM(debit_minor) debit,SUM(credit_minor) credit FROM finance_journal_lines WHERE journal_id=?').bind(confirmed.journalId).first()).toEqual({debit:6000,credit:6000});
 expect(await db.prepare('SELECT matched_id FROM finance_bank_transactions WHERE id=?').bind(bankRow.id).first('matched_id')).toBe(confirmed.paymentId);
 await recordSupplierResult(f.payer,String(other.id),{expectedVersion:2,outcome:'unknown',evidence:'Bank portal timed out; investigation open'},key());
 expect((await supplierBatchWork(f.manager)).find(x=>x.id===id)).toMatchObject({queue:'todo'});
 await expect(createSupplierBatch(f.reviewer,input([entry(second,'1',4)]),key())).rejects.toMatchObject({code:'CONFLICT'});
 await recordSupplierResult(f.manager,String(other.id),{expectedVersion:3,outcome:'failed',evidence:'Bank operations independently confirmed no debit'},key());
 expect((await supplierBatchWork(f.manager)).find(x=>x.id===id)).toMatchObject({queue:'todo',nextAction:'Review failed instructions and prepare payment for the remaining source balance'});
 const v=Number(await db.prepare('SELECT version FROM finance_invoices WHERE id=?').bind(second).first('version'));
 const replacement=await createSupplierBatch(f.reviewer,input([entry(second,'50',v)]),key()),replacementId=String(replacement.id);
 await decideSupplierBatch(f.manager,replacementId,{expectedVersion:1,reason:'Retry after independently verified bank failure'},key(),'approve');
 await exportSupplierBatch(f.payer,replacementId,{expectedVersion:2},key());
 const replacementItem=(await supplierBatchWorkspace(f.payer,replacementId)).items[0],replacementBank=await bank('50');
 await confirmSupplierInstruction(f.payer,String(replacementItem.id),{expectedVersion:2,bankTransactionId:replacementBank.id,bankVersion:replacementBank.version,periodId:f.period,bankReference:'SIM-RETRY-50',evidence:'Replacement bank debit and approved beneficiary verified'},key());
 expect(await db.prepare('SELECT outstanding_minor,status FROM finance_invoices WHERE id=?').bind(second).first()).toEqual({outstanding_minor:0,status:'paid'});
 expect((await supplierBatchWork(f.manager)).find(x=>x.id===id)).toMatchObject({queue:'completed'});
 expect((await supplierBatchWork(f.manager)).find(x=>x.id===replacementId)).toMatchObject({queue:'completed'});
 // The retained functional carrying balance must follow a real partial then
 // final payment, including the last exact remainder and immutable allocations.
 expect(await db.prepare('SELECT outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?').bind(first).first()).toEqual({outstanding_minor:4000,base_outstanding_minor:4000});
 const finalVersion=Number(await db.prepare('SELECT version FROM finance_invoices WHERE id=?').bind(first).first('version'));
 const finalBatch=await createSupplierBatch(f.reviewer,input([entry(first,'40',finalVersion)]),key()),finalId=String(finalBatch.id);
 await decideSupplierBatch(f.manager,finalId,{expectedVersion:1,reason:'Independently settle the exact final functional carrying remainder'},key(),'approve');await exportSupplierBatch(f.payer,finalId,{expectedVersion:2},key());
 const finalItem=(await supplierBatchWorkspace(f.payer,finalId)).items[0],finalBank=await bank('40'),finalKey=key(),finalInput={expectedVersion:2,bankTransactionId:finalBank.id,bankVersion:finalBank.version,periodId:f.period,bankReference:'SIM-FINAL-40',evidence:'Actual remaining functional supplier balance and bank evidence checked'};
 const final=await confirmSupplierInstruction(f.payer,String(finalItem.id),finalInput,finalKey);expect(await confirmSupplierInstruction(f.payer,String(finalItem.id),finalInput,finalKey)).toEqual(final);
 expect(await db.prepare('SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id=?').bind(first).first()).toEqual({status:'paid',outstanding_minor:0,base_outstanding_minor:0});
 expect((await db.prepare('SELECT amount_minor,base_invoice_minor,base_payment_minor,realized_fx_minor FROM finance_allocations WHERE invoice_id=? ORDER BY amount_minor').bind(first).all()).results).toEqual([{amount_minor:4000,base_invoice_minor:4000,base_payment_minor:4000,realized_fx_minor:0},{amount_minor:6000,base_invoice_minor:6000,base_payment_minor:6000,realized_fx_minor:0}]);
 expect(await db.prepare('SELECT amount_minor,base_amount_minor,fx_rate_micros FROM finance_payments WHERE id=?').bind(final.paymentId).first()).toEqual({amount_minor:4000,base_amount_minor:4000,fx_rate_micros:1000000});
});
it('prevents concurrent over-reservation and permits cancellation only before export',async()=>{
 const source=await invoice(),requests=await Promise.allSettled([createSupplierBatch(f.reviewer,input([entry(source,'70')]),key()),createSupplierBatch(f.reviewer,input([entry(source,'70')]),key())]);
 expect(requests.filter(r=>r.status==='fulfilled')).toHaveLength(1);
 const batch=String((requests.find(r=>r.status==='fulfilled') as PromiseFulfilledResult<Record<string,unknown>>).value.id);
 await expect(decideSupplierBatch({...f.reviewer,role:'holding_owner'},batch,{expectedVersion:1,reason:'Attempt self approval'},key(),'approve')).rejects.toMatchObject({code:'INDEPENDENT_APPROVER_REQUIRED'});
 await decideSupplierBatch(f.reviewer,batch,{expectedVersion:1,reason:'Incorrect beneficiary; prepare corrected batch'},key(),'cancel');
 const v=Number(await db.prepare('SELECT version FROM finance_invoices WHERE id=?').bind(source).first('version'));
 const corrected=await createSupplierBatch(f.reviewer,input([entry(source,'100',v)]),key()),id=String(corrected.id);
 await decideSupplierBatch(f.manager,id,{expectedVersion:1,reason:'Sources and beneficiary independently verified'},key(),'approve');
 await exportSupplierBatch(f.payer,id,{expectedVersion:2},key());
 await expect(decideSupplierBatch(f.manager,id,{expectedVersion:3,reason:'Cannot simply cancel exported instruction'},key(),'cancel')).rejects.toMatchObject({code:'BATCH_NOT_EDITABLE'});
 const item=(await supplierBatchWorkspace(f.payer,id)).items[0];
 await expect(recordSupplierResult({...f.payer,role:'holding_owner'},String(item.id),{expectedVersion:2,outcome:'failed',evidence:'Exporter attempts self release'},key())).rejects.toMatchObject({code:'INDEPENDENT_REVIEW_REQUIRED'});
 const mismatch=await bank('99');
 await expect(confirmSupplierInstruction(f.payer,String(item.id),{expectedVersion:2,bankTransactionId:mismatch.id,bankVersion:1,periodId:f.period,bankReference:'Mismatch',evidence:'Wrong amount'},key())).rejects.toMatchObject({code:'BANK_EVIDENCE_MISMATCH'});
 expect(await db.prepare('SELECT outstanding_minor FROM finance_invoices WHERE id=?').bind(source).first('outstanding_minor')).toBe(10000);
});

it('rejects wrong ledgers, closed periods, stale versions and revoked treasury assignments',async()=>{
 const source=await invoice();
 await expect(createSupplierBatch(f.reviewer,{...input([entry(source)]),bankAccountId:f.accounts.tax},key())).rejects.toMatchObject({code:'BANK_ACCOUNT_INVALID'});
 const draft=await createSupplierBatch(f.reviewer,input([entry(source)]),key()),id=String(draft.id);
 await expect(decideSupplierBatch(f.manager,id,{expectedVersion:2,reason:'Stale view'},key(),'approve')).rejects.toMatchObject({code:'CONFLICT'});
 await decideSupplierBatch(f.manager,id,{expectedVersion:1,reason:'Verified source and beneficiary'},key(),'approve');
 await db.prepare("UPDATE finance_periods SET state='closed' WHERE id=?").bind(f.period).run();
 await expect(exportSupplierBatch(f.payer,id,{expectedVersion:2},key())).rejects.toMatchObject({code:'CONFLICT'});
 expect((await supplierBatchWorkspace(f.payer,id)).batch?.status).toBe('approved');
 await db.prepare("UPDATE finance_periods SET state='open' WHERE id=?").bind(f.period).run();
 await exportSupplierBatch(f.payer,id,{expectedVersion:2},key());
 const item=(await supplierBatchWorkspace(f.payer,id)).items[0],evidence=await bank();
 const confirmation={expectedVersion:2,bankTransactionId:evidence.id,bankVersion:1,periodId:f.period,bankReference:'SIM-BANK',evidence:'Simulated exact bank transaction reviewed'};
 await expect(confirmSupplierInstruction(f.payer,String(item.id),{...confirmation,bankVersion:2},key())).rejects.toMatchObject({code:'CONFLICT'});
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),f.payer.assignmentId).run();
 // Current authority is denied before the legacy command's SQL assertions.
 await expect(confirmSupplierInstruction(f.payer,String(item.id),confirmation,key())).rejects.toMatchObject({status:403,code:'FINANCE_ACCESS_CHANGED'});
 expect(await db.prepare('SELECT outstanding_minor FROM finance_invoices WHERE id=?').bind(source).first('outstanding_minor')).toBe(10000);
 expect(await db.prepare('SELECT match_status FROM finance_bank_transactions WHERE id=?').bind(evidence.id).first('match_status')).toBe('unmatched');
});
it('prevents the legacy contractor exporter and supplier batches from reserving the same invoice',async()=>{
 const {createPaymentBatch}=await import("@erpx/server-people_payroll/public/people-payroll");
 const source=await invoice();
 await createSupplierBatch(f.reviewer,input([entry(source)]),key());
 await expect(createPaymentBatch(f.owner,{sourceType:'contractor',sourceId:source,formatCode:'generic-csv'},key())).rejects.toMatchObject({code:'CONFLICT'});
 const legacySource=await invoice();
 await createPaymentBatch(f.owner,{sourceType:'contractor',sourceId:legacySource,formatCode:'generic-csv'},key());
 const current=Number(await db.prepare('SELECT version FROM finance_invoices WHERE id=?').bind(legacySource).first('version'));
 await expect(createSupplierBatch(f.reviewer,input([entry(legacySource,'100',current)]),key())).rejects.toMatchObject({code:'CONFLICT'});
});
it('rolls back supplier payment accounting, bank match and File binding after the final outbox fails, then retries and replays once',async()=>{
 const source=await invoice(),draft=await createSupplierBatch(f.reviewer,input([entry(source)]),key()),batchId=String(draft.id);await decideSupplierBatch(f.manager,batchId,{expectedVersion:1,reason:'Independent supplier payment review'},key(),'approve');await exportSupplierBatch(f.payer,batchId,{expectedVersion:2},key());
 const item=(await supplierBatchWorkspace(f.payer,batchId)).items[0],proof=await seedAttachmentSet(db,f.payer,{targetType:'finance_supplier_batch',targetId:batchId,componentKind:'action',componentRef:`supplier-instruction:${String(item.id)}`,purposeKey:'payment_proof',expectedTargetVersion:3}),bankRow=await bank(),command=key(),payload={expectedVersion:2,bankTransactionId:bankRow.id,bankVersion:bankRow.version,periodId:f.period,bankReference:'SIM-ROLLBACK-100',evidence:'Approved beneficiary and exact bank debit checked',attachmentSetIds:[proof.setId]},fault=`supplier_payment_failure_${crypto.randomUUID().replaceAll('-','').slice(0,12)}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'SUPPLIER_FINAL_OUTBOX_FAILURE' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} BEFORE INSERT ON outbox_events FOR EACH ROW WHEN(NEW.command_id='${command.replaceAll("'","''")}') EXECUTE FUNCTION ${fault}();`);
 try{
  await expect(confirmSupplierInstruction(f.payer,String(item.id),payload,command)).rejects.toMatchObject({code:'23514'});
  expect(await db.prepare('SELECT outstanding_minor,status FROM finance_invoices WHERE id=?').bind(source).first()).toEqual({outstanding_minor:10000,status:'posted'});
  expect(await db.prepare('SELECT match_status,version FROM finance_bank_transactions WHERE id=?').bind(bankRow.id).first()).toEqual({match_status:'unmatched',version:bankRow.version});
  expect(await db.prepare('SELECT status,version,payment_id FROM finance_supplier_instructions WHERE id=?').bind(item.id).first()).toEqual({status:'exported',version:2,payment_id:null});
  expect(await db.prepare('SELECT version FROM finance_supplier_batches WHERE id=?').bind(batchId).first('version')).toBe(3);
  expect(await db.prepare("SELECT count(*) n FROM finance_journals WHERE source_type='supplier_batch' AND source_id=?").bind(item.id).first('n')).toBe(0);
  expect(await db.prepare('SELECT count(*) n FROM finance_allocations WHERE invoice_id=?').bind(source).first('n')).toBe(0);
  expect(await attachmentBindingState(db,proof.setId)).toMatchObject({set_state:'draft',link_state:'draft'});
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n')).toBe(0);
 }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
 const confirmed=await confirmSupplierInstruction(f.payer,String(item.id),payload,command);expect(await confirmSupplierInstruction(f.payer,String(item.id),payload,command)).toEqual(confirmed);
 expect(await db.prepare('SELECT outstanding_minor,status FROM finance_invoices WHERE id=?').bind(source).first()).toEqual({outstanding_minor:0,status:'paid'});expect(await attachmentBindingState(db,proof.setId)).toMatchObject({set_state:'bound',link_state:'bound'});
 expect(await db.prepare("SELECT count(*) n FROM finance_journals WHERE source_type='supplier_batch' AND source_id=?").bind(item.id).first('n')).toBe(1);
 expect(await db.prepare('SELECT count(*) n FROM finance_allocations WHERE invoice_id=?').bind(source).first('n')).toBe(1);
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n')).toBe(1);
});

afterEach(()=>vi.unstubAllEnvs());

it("purchased supplier batching denies an unpurchased bank workflow and Calendar projection without changing invoices",async()=>{
 const source=await invoice();vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");await expect(supplierBatchWorkspace(f.reviewer)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(createSupplierBatch(f.reviewer,input([entry(source)]),key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});expect(await supplierBatchWork(f.manager)).toEqual([]);const emptyCalendar=await calendarData(db,f.payer,{date:"2026-09-03",view:"day",layers:"supplier_batches",mode:"team"});expect(emptyCalendar.sources).toContainEqual({source:"supplier_batches",status:"ready"});expect(emptyCalendar.items).toEqual([]);expect(await db.prepare("SELECT outstanding_minor,status FROM finance_invoices WHERE id=?").bind(source).first()).toEqual({outstanding_minor:10000,status:"posted"});
});
it("Finance-only draining reserves real supplier debt and settles independently with exact bank and Files evidence",async()=>{
 await db.prepare("UPDATE finance_accounts SET control_kind='bank',is_control=1 WHERE id=?").bind(f.accounts.cash).run();
 const secondary=await reviewedSecondaryBook(db,f.reviewer,f.manager,f.chart,f.period,"2026-09-01");
 await reviewedFinanceGrant(db,f.company,"finance");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");const source=await invoice();await reviewedFinanceGrant(db,f.company,"finance","draining");const draft=await createSupplierBatch(f.reviewer,input([entry(source)]),key()),id=String(draft.id);await decideSupplierBatch(f.manager,id,{expectedVersion:1,reason:"Independent retained supplier and beneficiary review"},key(),"approve");const exportKey=key(),exported=await exportSupplierBatch(f.payer,id,{expectedVersion:2},exportKey);expect(await exportSupplierBatch(f.payer,id,{expectedVersion:2},exportKey)).toEqual(exported);expect(await recoverSupplierExport(f.payer,id)).toMatchObject({content:exported.content,sha256:exported.sha256});
 expect((await calendarData(db,f.payer,{date:"2026-09-03",view:"day",layers:"supplier_batches",mode:"team"})).items.some(item=>item.sourceId===id)).toBe(true);
 const item=(await supplierBatchWorkspace(f.payer,id)).items[0],proof=await seedAttachmentSet(db,f.payer,{targetType:"finance_supplier_batch",targetId:id,componentKind:"action",componentRef:`supplier-instruction:${item.id}`,purposeKey:"payment_proof",expectedTargetVersion:3}),bankRow=await bank(),payload={expectedVersion:2,bankTransactionId:bankRow.id,bankVersion:bankRow.version,periodId:f.period,bankReference:"ISOLATED-SUPPLIER-100",evidence:"Reviewed actual retained bank debit and counterparty",attachmentSetIds:[proof.setId]},command=key(),result=await confirmSupplierInstruction(f.payer,String(item.id),payload,command);expect(await confirmSupplierInstruction(f.payer,String(item.id),payload,command)).toEqual(result);
 expect(await attachmentBindingState(db,proof.setId)).toMatchObject({set_state:"bound",link_state:"bound"});expect(await db.prepare("SELECT outstanding_minor,status FROM finance_invoices WHERE id=?").bind(source).first()).toEqual({outstanding_minor:0,status:"paid"});expect(await db.prepare("SELECT match_status,matched_id FROM finance_bank_transactions WHERE id=?").bind(bankRow.id).first()).toEqual({match_status:"matched",matched_id:result.paymentId});
 const projection=await db.prepare("SELECT id FROM finance_journals WHERE source_type='book_projection' AND source_id=? AND chart_id=? AND status='posted'").bind(result.journalId,secondary.chartId).first<{id:string}>();expect(projection).not.toBeNull();expect(await db.prepare("SELECT SUM(base_debit_minor-base_credit_minor) balance FROM finance_journal_lines WHERE journal_id=?").bind(projection!.id).first("balance")).toBe(0);
});
it("purchased supplier confirmation rolls back invoice bank File binding and receipt after deferred payer revocation, then retries once",async()=>{
 await reviewedFinanceGrant(db,f.company,"finance");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");const source=await invoice(),draft=await createSupplierBatch(f.reviewer,input([entry(source)]),key()),id=String(draft.id);await decideSupplierBatch(f.manager,id,{expectedVersion:1,reason:"Independent retained batch review"},key(),"approve");await exportSupplierBatch(f.payer,id,{expectedVersion:2},key());
 const item=(await supplierBatchWorkspace(f.payer,id)).items[0],proof=await seedAttachmentSet(db,f.payer,{targetType:"finance_supplier_batch",targetId:id,componentKind:"action",componentRef:`supplier-instruction:${item.id}`,purposeKey:"payment_proof",expectedTargetVersion:3}),bankRow=await bank(),payload={expectedVersion:2,bankTransactionId:bankRow.id,bankVersion:bankRow.version,periodId:f.period,bankReference:"ISOLATED-LATE-REVOKE",evidence:"Reviewed retained source and original bank debit",attachmentSetIds:[proof.setId]},command=key(),fault=`supplier_purchase_${key().replaceAll("-","")}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.payer.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
 try{await expect(confirmSupplierInstruction(f.payer,String(item.id),payload,command)).rejects.toMatchObject({code:"FINANCE_ACCESS_CHANGED"});expect(await db.prepare("SELECT outstanding_minor,status FROM finance_invoices WHERE id=?").bind(source).first()).toEqual({outstanding_minor:10000,status:"posted"});expect(await db.prepare("SELECT match_status,version FROM finance_bank_transactions WHERE id=?").bind(bankRow.id).first()).toEqual({match_status:"unmatched",version:bankRow.version});expect(await attachmentBindingState(db,proof.setId)).toMatchObject({set_state:"draft",link_state:"draft"});for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first("n")).toBe(0);}finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}()`);}
 const result=await confirmSupplierInstruction(f.payer,String(item.id),payload,command);expect(await confirmSupplierInstruction(f.payer,String(item.id),payload,command)).toEqual(result);expect(await attachmentBindingState(db,proof.setId)).toMatchObject({set_state:"bound",link_state:"bound"});expect(await db.prepare("SELECT count(*) n FROM command_assertions").first("n")).toBe(0);
});

it('retains ambiguous historical functional carrying for reconciliation instead of inventing a new supplier payment or changing old money',async()=>{
 const source=await invoice(),draft=await createSupplierBatch(f.reviewer,input([entry(source)]),key()),id=String(draft.id);await decideSupplierBatch(f.manager,id,{expectedVersion:1,reason:'Independent original payable source review'},key(),'approve');await exportSupplierBatch(f.payer,id,{expectedVersion:2},key());
 const item=(await supplierBatchWorkspace(f.payer,id)).items[0],evidence=await bank(),command=key(),payload={expectedVersion:2,bankTransactionId:evidence.id,bankVersion:evidence.version,periodId:f.period,bankReference:'LEGACY-CARRYING',evidence:'Original bank observation retained while legacy carrying needs reconciliation'};
 await db.prepare('UPDATE finance_invoices SET base_outstanding_minor=base_outstanding_minor-1 WHERE id=?').bind(source).run();
 const tables=['finance_invoices','finance_payments','finance_allocations','finance_journals','finance_journal_lines','finance_financial_events','finance_bank_transactions','command_receipts','audit_events','outbox_events'],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(row)::text image FROM ${table} row ORDER BY row_to_json(row)::text`).all()).results)),before=await snapshot();
 await expect(confirmSupplierInstruction(f.payer,String(item.id),payload,command)).rejects.toMatchObject({code:'SUPPLIER_CARRYING_MISMATCH'});expect(await snapshot()).toEqual(before);
});
