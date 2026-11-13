import {beforeAll,beforeEach,afterAll,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {createInvoice,invoiceSchema,postInvoice,importBankStatement} from '@erpx/server-finance/public/finance';
import {settleInvoice} from '@erpx/server-commerce/public/commerce';
import {supplierBatchWorkspace} from "@erpx/server-procurement/public/supplier-payments";
import {prepareTaxIdentity,decideTaxIdentity,taxIdentitySchema} from "@erpx/server-finance/public/finance-tax-identities";
import {thaiFinanceTaxPack} from "@erpx/feature-finance/lib/thai-finance-tax-pack";
import {assignFlowPrimary,resolveFlow} from '@erpx/server-workflow/public/flow-routing';
import {loadIamAuthority} from "@erpx/server-identity_access/public/iam-store";
import {createAttachmentSet,reserveAttachmentFile,completeAttachmentUpload,drainAttachmentScans,bindRecordAttachmentSet} from "@erpx/server-files/public/attachments/service";

vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
vi.mock("@erpx/server-files/public/attachments/scanner",()=>({attachmentScannerReadiness:async()=>({enabled:true,ready:true,engine:'Declared clean scanner fixture',definitionUpdatedAt:Date.now(),error:null}),scanAttachmentBytes:async()=>({clean:true,signature:null,engine:'Declared clean scanner fixture',definitionUpdatedAt:Date.now()})}));
const png=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=','base64');
const reason='Independent review of fictional Company statutory payer evidence and scanned signature.',oldGate=process.env.ERPX_ATTACHMENTS_V2;
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);process.env.ERPX_ATTACHMENTS_V2='true';});
afterAll(()=>{if(oldGate===undefined)delete process.env.ERPX_ATTACHMENTS_V2;else process.env.ERPX_ATTACHMENTS_V2=oldGate;});
beforeEach(async()=>{
 f=await expenseFixture();
 for(const actor of [f.reviewer,f.manager,f.payer,f.owner]){actor.branchId=null;actor.departmentId=null;actor.modules=['finance'];await db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind('["finance"]',actor.assignmentId).run();actor.iam=await loadIamAuthority(db,f.company,actor.userId,actor.role);}
});
async function bill(){
 const party=`supplier-${key()}`;await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'supplier',?,'Declared fictional supplier','THB',1,1)").bind(party,f.company,party).run();
 const result=await createInvoice(f.reviewer,invoiceSchema.parse({partyId:party,kind:'payable',invoiceNumber:key(),invoiceDate:'2026-09-01',dueDate:'2026-09-10',currency:'THB',subtotal:'100',tax:'0',debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable}),key()),flow=await resolveFlow(db,f.owner,'invoice',result.id);
 await assignFlowPrimary(db,f.owner,'invoice',result.id,{stepKey:'draft',userId:f.manager.userId,reason,saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await postInvoice(f.manager,result.id,{expectedVersion:1,periodId:f.period,entryDate:'2026-09-02'},key());return result.id;
}
async function payerIdentity(effectiveFrom='2026-09-01',effectiveTo='2026-09-30'){
 const prefix='099999999991',sum=[...prefix].reduce((total,digit,index)=>total+Number(digit)*(13-index),0),taxId=prefix+String((11-sum%11)%10);
 const payer=await prepareTaxIdentity(f.reviewer,taxIdentitySchema.parse({partyId:null,legalName:'บริษัทผู้จ่ายเงินสมมติ จำกัด',taxId,address:'99 ถนนทดสอบ แขวงสมมติ เขตสมมติ กรุงเทพมหานคร 10000',branchCode:'00000',entityKind:'domestic_company',effectiveFrom,effectiveTo,signerName:'ผู้ลงนามสมมติ',signerAuthority:reason,evidenceRef:reason,rulePackVersion:thaiFinanceTaxPack.version}),key()),id=String(payer.id);
 const set=await createAttachmentSet(db,f.reviewer,{targetType:'finance_tax_identity',targetId:id,componentKind:'record',purposeKey:'signer_signature',expectedTargetVersion:1},key()),file=await reserveAttachmentFile(db,f.reviewer,set.id,{filename:'fictional-statutory-signer.png',mediaType:'image/png',bytes:png.byteLength,caption:reason},key());
 const uploaded=await completeAttachmentUpload(db,env.H001_FILES,f.reviewer,file.uploadIntentId,png.buffer.slice(png.byteOffset,png.byteOffset+png.byteLength));expect(await drainAttachmentScans(db,env.H001_FILES,uploaded.sourceId)).toBe(true);await bindRecordAttachmentSet(db,f.reviewer,set.id,1,key());
 return {id,signatureLinkId:file.attachmentLinkId};
}
async function approvePayer(payer:Awaited<ReturnType<typeof payerIdentity>>){await decideTaxIdentity(f.manager,payer.id,{action:'approve',expectedVersion:2,signatureLinkId:payer.signatureLinkId,reason},key());expect(await db.prepare('SELECT status,created_by,reviewed_by,effective_from,effective_to FROM finance_tax_identities WHERE id=?').bind(payer.id).first()).toMatchObject({status:'approved',created_by:f.reviewer.userId,reviewed_by:f.manager.userId});}
async function seedDraftSupplierBatch(invoiceId:string){
 const batchId=`supplier-batch-${key()}`,instructionId=`${batchId}-0`,now=Date.now();
 await db.batch([
  db.prepare("INSERT INTO finance_supplier_batches(id,company_id,currency,bank_account_id,planned_date,status,version,created_by,created_at,updated_at) VALUES(?,?,'THB',?,?,'draft',1,?,?,?)").bind(batchId,f.company,f.accounts.cash,'2026-09-03',f.reviewer.userId,now,now),
  db.prepare("INSERT INTO finance_supplier_instructions(id,batch_id,invoice_id,invoice_version,amount_minor,beneficiary_name,beneficiary_account,status,version,created_at,updated_at) VALUES(?,?,?,2,10000,?,?,'reserved',1,?,?)").bind(instructionId,batchId,invoiceId,'Declared fictional supplier','SYNTHETIC-SUPPLIER-ACCOUNT',now,now),
 ]);
 return {id:batchId,status:"draft"};
}
// Native Finance owns supplier-batch writes. These statutory oracles assert the
// ` persisted native row shapes and the remaining Finance settlement guard.
async function createSupplierBatchFixture(fixture:Awaited<ReturnType<typeof expenseFixture>>,invoiceId:string){
 const command=key(),batchId=`supplier-batch-${command}`,instructionId=`${batchId}-0`,now=Date.now();
 await db.batch([
  db.prepare("INSERT INTO finance_supplier_batches(id,company_id,currency,bank_account_id,planned_date,status,approved_by,approval_reason,exported_by,export_command_id,export_hash,version,created_by,created_at,updated_at) VALUES(?,?,'THB',?,?,'exported',?,'Independent statutory review',?,?,?,3,?,?,?)").bind(batchId,fixture.company,fixture.accounts.cash,'2026-09-03',fixture.manager.userId,fixture.payer.userId,`${command}-export`,'0'.repeat(64),fixture.reviewer.userId,now,now),
  db.prepare("INSERT INTO finance_supplier_instructions(id,batch_id,invoice_id,invoice_version,amount_minor,beneficiary_name,beneficiary_account,status,version,created_at,updated_at) VALUES(?,?,?,2,10000,?,?,'exported',2,?,?)").bind(instructionId,batchId,invoiceId,'Declared fictional supplier','SYNTHETIC-SUPPLIER-ACCOUNT',now,now),
  db.prepare("UPDATE finance_invoices SET version=version+1,updated_at=? WHERE id=?").bind(now,invoiceId),
 ]);
 return {batchId,instructionId,command};
}
async function exported(invoiceId:string){const batch=await createSupplierBatchFixture(f,invoiceId);return {id:batch.batchId,item:(await supplierBatchWorkspace(f.payer,batch.batchId)).items[0]};}
async function seedConfirmedSupplierBatch(invoiceId:string,bankTransactionId:string){
 const command=key(),batchId=`supplier-batch-${command}`,instructionId=`${batchId}-0`,paymentId=`supplier-payment-${command}`,journalId=`journal-${command}`,now=Date.now();
 const invoice=await db.prepare("SELECT currency,party_id,credit_account_id FROM finance_invoices WHERE id=?").bind(invoiceId).first<{currency:string;party_id:string;credit_account_id:string}>();if(!invoice)throw Error('Statutory supplier fixture invoice missing');
 await db.batch([
  db.prepare("INSERT INTO finance_supplier_batches(id,company_id,currency,bank_account_id,planned_date,status,approved_by,approval_reason,exported_by,export_command_id,export_hash,version,created_by,created_at,updated_at) VALUES(?,?,'THB',?,?,'exported',?,'Independent statutory review',?,?,?,4,?,?,?)").bind(batchId,f.company,f.accounts.cash,'2026-09-03',f.manager.userId,f.payer.userId,`${command}-export`,'0'.repeat(64),f.reviewer.userId,now,now),
  db.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,i.company_id,j.chart_id,?,'SB-'||?,?, ?,1000000,'supplier_batch',?,?,'posted',?,?,?,?,? FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id WHERE i.id=?").bind(journalId,f.period,command,'2026-09-04',invoice.currency,instructionId,'Statutory supplier settlement',f.payer.userId,f.payer.userId,now,now,now,invoiceId),
  db.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) VALUES(?,?,?,'',10000,0,10000,0,'{}',0)").bind(`${journalId}-0`,journalId,invoice.credit_account_id),
  db.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) VALUES(?,?,?,'',0,10000,0,10000,'{}',1)").bind(`${journalId}-1`,journalId,f.accounts.cash),
  db.prepare("INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,fx_rate_micros,base_amount_minor,created_by,approved_by,released_by,created_at,updated_at) VALUES(?,?,?,'payment',?,'2026-09-04','THB',10000,0,?,'settled',?,1000000,10000,?,?,?,?,?)").bind(paymentId,f.company,invoice.party_id,`SB-${command}`,f.accounts.cash,journalId,f.reviewer.userId,f.manager.userId,f.payer.userId,now,now),
  db.prepare("INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,base_invoice_minor,base_payment_minor,created_by,created_at) VALUES(?,?,?,?,10000,10000,10000,?,?)").bind(`allocation-${command}`,f.company,paymentId,invoiceId,f.payer.userId,now),
  db.prepare("UPDATE finance_invoices SET outstanding_minor=0,base_outstanding_minor=0,status='paid',version=version+1,updated_at=? WHERE id=?").bind(now,invoiceId),
  db.prepare("UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=?").bind(paymentId,bankTransactionId),
  db.prepare("INSERT INTO finance_supplier_instructions(id,batch_id,invoice_id,invoice_version,amount_minor,beneficiary_name,beneficiary_account,status,evidence,resolved_by,bank_reference,payment_id,bank_transaction_id,version,created_at,updated_at) VALUES(?,?,?,2,10000,?,?,'confirmed',?,?,?,?,?,3,?,?)").bind(instructionId,batchId,invoiceId,'Declared fictional supplier','SYNTHETIC-SUPPLIER-ACCOUNT','Declared statutory bank evidence',f.payer.userId,'SYNTHETIC-100-TRANSFER',paymentId,bankTransactionId,now,now),
  db.prepare("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES(?,'finance.supplier_batch.confirm',?,?,?,?,?,?)").bind(`${command}-confirm`,f.payer.userId,'statutory-confirm',instructionId,JSON.stringify({id:instructionId,status:'confirmed',version:3,paymentId,journalId}),now,JSON.stringify({companyId:f.company})),
 ]);
 return {paymentId,journalId,instructionId};
}
async function bank(){const command=key();await importBankStatement(f.reviewer,{bankAccount:f.accounts.cash,statementNumber:key(),statementDate:'2026-09-04',opening:'100',closing:'0',sourceHash:key(),rows:[{externalId:key(),transactionDate:'2026-09-04',amount:'-100',currency:'THB',description:'Declared synthetic supplier bank confirmation'}]},command);return String(await db.prepare('SELECT id FROM finance_bank_transactions WHERE statement_id=?').bind(`statement-${command}`).first('id'));}

it('blocks inline payable settlement only after effective independent statutory payer review',async()=>{
 const payer=await payerIdentity(),beforeReview=await bill(),generic=await seedDraftSupplierBatch(beforeReview);expect(generic.status).toBe('draft');
 await approvePayer(payer);const source=await bill(),settleKey=key(),journals=await db.prepare('SELECT COUNT(*) count FROM finance_journals WHERE company_id=?').bind(f.company).first('count');
 await expect(settleInvoice(f.payer,source,{expectedVersion:2,date:'2026-09-03',amount:'100',reference:'Unreviewed gross payment must use the controlled Finance owner.',attachmentSetIds:[]},settleKey)).rejects.toMatchObject({status:409,code:'WITHHOLDING_OWNER_REQUIRED'});
 expect(await db.prepare('SELECT status,version,outstanding_minor FROM finance_invoices WHERE id=?').bind(source).first()).toEqual({status:'posted',version:2,outstanding_minor:10000});
 expect(await db.prepare('SELECT COUNT(*) count FROM finance_supplier_instructions WHERE invoice_id=?').bind(source).first('count')).toBe(0);expect(await db.prepare('SELECT COUNT(*) count FROM finance_payments WHERE company_id=?').bind(f.company).first('count')).toBe(0);expect(await db.prepare('SELECT COUNT(*) count FROM finance_journals WHERE company_id=?').bind(f.company).first('count')).toBe(journals);
 expect(await db.prepare('SELECT COUNT(*) count FROM command_receipts WHERE command_id=?').bind(settleKey).first('count')).toBe(0);
});
it('keeps an exported supplier instruction readable without a bank payment after statutory review changes',async()=>{
 const source=await bill(),batch=await exported(source),payer=await payerIdentity();await approvePayer(payer);const bankId=await bank(),journals=await db.prepare('SELECT COUNT(*) count FROM finance_journals WHERE company_id=?').bind(f.company).first('count');
 const workspace=await supplierBatchWorkspace(f.payer,batch.id);expect(workspace.batch).toMatchObject({status:'exported',version:3});expect(workspace.items[0]).toMatchObject({status:'exported',version:2,payment_id:null,bank_transaction_id:null});
 expect(await db.prepare('SELECT status,outstanding_minor FROM finance_invoices WHERE id=?').bind(source).first()).toEqual({status:'posted',outstanding_minor:10000});expect(await db.prepare('SELECT match_status,version FROM finance_bank_transactions WHERE id=?').bind(bankId).first()).toEqual({match_status:'unmatched',version:1});
 expect(await db.prepare('SELECT COUNT(*) count FROM finance_payments WHERE company_id=?').bind(f.company).first('count')).toBe(0);expect(await db.prepare('SELECT COUNT(*) count FROM finance_journals WHERE company_id=?').bind(f.company).first('count')).toBe(journals);
});
it('preserves an actual gross supplier settlement row outside the reviewed payer effective interval',async()=>{
 const payer=await payerIdentity('2026-09-10','2026-09-20');await approvePayer(payer);const source=await bill(),bankId=await bank(),result=await seedConfirmedSupplierBatch(source,bankId);
 expect(await db.prepare('SELECT status,outstanding_minor FROM finance_invoices WHERE id=?').bind(source).first()).toEqual({status:'paid',outstanding_minor:0});expect(await db.prepare('SELECT status,amount_minor,payment_date FROM finance_payments WHERE id=?').bind(result.paymentId).first()).toEqual({status:'settled',amount_minor:10000,payment_date:'2026-09-04'});expect(await db.prepare('SELECT matched_id FROM finance_bank_transactions WHERE id=?').bind(bankId).first('matched_id')).toBe(result.paymentId);
 expect(await db.prepare('SELECT SUM(base_debit_minor) debit,SUM(base_credit_minor) credit FROM finance_journal_lines WHERE journal_id=?').bind(result.journalId).first()).toEqual({debit:10000,credit:10000});
});
