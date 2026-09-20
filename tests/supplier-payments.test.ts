import { beforeAll,beforeEach,expect,it,vi } from 'vitest';
import { applyMigrations,env } from './runtime-env';
import { expenseDb as db,expenseFixture,key } from './helpers/expense-fixture';
import { settleInvoice } from '../src/server/commerce';
import { listStaffNotifications } from '../src/server/staff-notifications';
import { createInvoice,invoiceSchema,postInvoice,importBankStatement,beginClose } from '../src/server/finance';
import { assignFlowPrimary,resolveFlow } from '../src/server/flow-routing';
import { createSupplierBatch,decideSupplierBatch,exportSupplierBatch,recoverSupplierExport,recordSupplierResult,confirmSupplierInstruction,supplierBatchWorkspace,supplierBatchWork } from '../src/server/supplier-payments';
vi.mock('../src/server/runtime',()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
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
 const {calendarData}=await import('../src/server/calendar');
 const calendar=await calendarData(db,f.payer,{date:'2026-09-03',view:'day',layers:'supplier_batches',mode:'mine'});
 expect(calendar.items.some(item=>item.sourceId===id)).toBe(true);
 expect((await calendarData(db,{...f.payer,branchId:f.branch},{date:'2026-09-03',view:'day',layers:'supplier_batches',mode:'team'})).items).toEqual([]);

 expect(await db.prepare("SELECT count(*) count FROM finance_payments WHERE company_id=?").bind(f.company).first('count')).toBe(0);
 await expect(beginClose(f.owner,f.period,key())).rejects.toMatchObject({code:'CLOSE_NOT_READY'});
 const workspace=await supplierBatchWorkspace(f.payer,id),item=workspace.items.find(i=>i.invoice_id===first)!,other=workspace.items.find(i=>i.invoice_id===second)!;
 const bankRow=await bank('60'),confirmation={expectedVersion:2,bankTransactionId:bankRow.id,bankVersion:bankRow.version,periodId:f.period,bankReference:'SIM-BANK-60',evidence:'Bank statement amount and approved beneficiary verified'},confirmKey=key();
 const confirmed=await confirmSupplierInstruction(f.payer,String(item.id),confirmation,confirmKey);
 expect(await confirmSupplierInstruction(f.payer,String(item.id),confirmation,confirmKey)).toEqual(confirmed);
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
 await expect(confirmSupplierInstruction(f.payer,String(item.id),confirmation,key())).rejects.toMatchObject({code:'CONFLICT'});
 expect(await db.prepare('SELECT outstanding_minor FROM finance_invoices WHERE id=?').bind(source).first('outstanding_minor')).toBe(10000);
 expect(await db.prepare('SELECT match_status FROM finance_bank_transactions WHERE id=?').bind(evidence.id).first('match_status')).toBe('unmatched');
});
it('prevents the legacy contractor exporter and supplier batches from reserving the same invoice',async()=>{
 const {createPaymentBatch}=await import('../src/server/people-payroll');
 const source=await invoice();
 await createSupplierBatch(f.reviewer,input([entry(source)]),key());
 await expect(createPaymentBatch(f.owner,{sourceType:'contractor',sourceId:source,formatCode:'generic-csv'},key())).rejects.toMatchObject({code:'CONFLICT'});
 const legacySource=await invoice();
 await createPaymentBatch(f.owner,{sourceType:'contractor',sourceId:legacySource,formatCode:'generic-csv'},key());
 const current=Number(await db.prepare('SELECT version FROM finance_invoices WHERE id=?').bind(legacySource).first('version'));
 await expect(createSupplierBatch(f.reviewer,input([entry(legacySource,'100',current)]),key())).rejects.toMatchObject({code:'CONFLICT'});
});
