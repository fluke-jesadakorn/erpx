import {beforeAll,beforeEach,afterAll,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {createInvoice,invoiceSchema,postInvoice,importBankStatement} from "../../../src/server/modules/finance/public/finance";
import {settleInvoice} from "../../../src/server/modules/commerce/public/commerce";
import {createSupplierBatch,decideSupplierBatch,exportSupplierBatch,confirmSupplierInstruction,supplierBatchWorkspace} from "../../../src/server/modules/procurement/public/supplier-payments";
import {prepareTaxIdentity,decideTaxIdentity,taxIdentitySchema} from "../../../src/server/modules/finance/public/finance-tax-identities";
import {thaiFinanceTaxPack} from "../../../src/lib/finance/thai-finance-tax-pack";
import {assignFlowPrimary,resolveFlow} from "../../../src/server/modules/workflow/public/flow-routing";
import {loadIamAuthority} from "../../../src/server/modules/identity_access/public/iam-store";
import {createAttachmentSet,reserveAttachmentFile,completeAttachmentUpload,drainAttachmentScans,bindRecordAttachmentSet} from "../../../src/server/modules/files/public/attachments/service";

vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
vi.mock("../../../src/server/modules/files/public/attachments/scanner",()=>({attachmentScannerReadiness:async()=>({enabled:true,ready:true,engine:'Declared clean scanner fixture',definitionUpdatedAt:Date.now(),error:null}),scanAttachmentBytes:async()=>({clean:true,signature:null,engine:'Declared clean scanner fixture',definitionUpdatedAt:Date.now()})}));
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
function batchInput(invoiceId:string){return {bankAccountId:f.accounts.cash,plannedDate:'2026-09-03',items:[{invoiceId,expectedVersion:2,amount:'100',beneficiaryName:'Declared fictional supplier',beneficiaryAccount:'SYNTHETIC-SUPPLIER-ACCOUNT'}]};}
async function payerIdentity(effectiveFrom='2026-09-01',effectiveTo='2026-09-30'){
 const prefix='099999999991',sum=[...prefix].reduce((total,digit,index)=>total+Number(digit)*(13-index),0),taxId=prefix+String((11-sum%11)%10);
 const payer=await prepareTaxIdentity(f.reviewer,taxIdentitySchema.parse({partyId:null,legalName:'บริษัทผู้จ่ายเงินสมมติ จำกัด',taxId,address:'99 ถนนทดสอบ แขวงสมมติ เขตสมมติ กรุงเทพมหานคร 10000',branchCode:'00000',entityKind:'domestic_company',effectiveFrom,effectiveTo,signerName:'ผู้ลงนามสมมติ',signerAuthority:reason,evidenceRef:reason,rulePackVersion:thaiFinanceTaxPack.version}),key()),id=String(payer.id);
 const set=await createAttachmentSet(db,f.reviewer,{targetType:'finance_tax_identity',targetId:id,componentKind:'record',purposeKey:'signer_signature',expectedTargetVersion:1},key()),file=await reserveAttachmentFile(db,f.reviewer,set.id,{filename:'fictional-statutory-signer.png',mediaType:'image/png',bytes:png.byteLength,caption:reason},key());
 const uploaded=await completeAttachmentUpload(db,env.H001_FILES,f.reviewer,file.uploadIntentId,png.buffer.slice(png.byteOffset,png.byteOffset+png.byteLength));expect(await drainAttachmentScans(db,env.H001_FILES,uploaded.sourceId)).toBe(true);await bindRecordAttachmentSet(db,f.reviewer,set.id,1,key());
 return {id,signatureLinkId:file.attachmentLinkId};
}
async function approvePayer(payer:Awaited<ReturnType<typeof payerIdentity>>){await decideTaxIdentity(f.manager,payer.id,{action:'approve',expectedVersion:2,signatureLinkId:payer.signatureLinkId,reason},key());expect(await db.prepare('SELECT status,created_by,reviewed_by,effective_from,effective_to FROM finance_tax_identities WHERE id=?').bind(payer.id).first()).toMatchObject({status:'approved',created_by:f.reviewer.userId,reviewed_by:f.manager.userId});}
async function exported(invoiceId:string){const batch=await createSupplierBatch(f.reviewer,batchInput(invoiceId),key()),id=String(batch.id);await decideSupplierBatch(f.manager,id,{expectedVersion:1,reason},key(),'approve');await exportSupplierBatch(f.payer,id,{expectedVersion:2},key());return {id,item:(await supplierBatchWorkspace(f.payer,id)).items[0]};}
async function bank(){const command=key();await importBankStatement(f.reviewer,{bankAccount:f.accounts.cash,statementNumber:key(),statementDate:'2026-09-04',opening:'100',closing:'0',sourceHash:key(),rows:[{externalId:key(),transactionDate:'2026-09-04',amount:'-100',currency:'THB',description:'Declared synthetic supplier bank confirmation'}]},command);return String(await db.prepare('SELECT id FROM finance_bank_transactions WHERE statement_id=?').bind(`statement-${command}`).first('id'));}
function confirmation(bankTransactionId:string){return {expectedVersion:2,bankTransactionId,bankVersion:1,periodId:f.period,bankReference:'SYNTHETIC-100-TRANSFER',evidence:reason,attachmentSetIds:[]};}

it('blocks new supplier batches and inline payable settlement only after effective independent statutory payer review',async()=>{
 const payer=await payerIdentity(),beforeReview=await bill(),generic=await createSupplierBatch(f.reviewer,batchInput(beforeReview),key());expect(generic.status).toBe('draft');
 await approvePayer(payer);const source=await bill(),createKey=key(),settleKey=key(),journals=await db.prepare('SELECT COUNT(*) count FROM finance_journals WHERE company_id=?').bind(f.company).first('count');
 await expect(createSupplierBatch(f.reviewer,batchInput(source),createKey)).rejects.toMatchObject({status:409,code:'WITHHOLDING_OWNER_REQUIRED'});
 await expect(settleInvoice(f.payer,source,{expectedVersion:2,date:'2026-09-03',amount:'100',reference:'Unreviewed gross payment must use the controlled Finance owner.',attachmentSetIds:[]},settleKey)).rejects.toMatchObject({status:409,code:'WITHHOLDING_OWNER_REQUIRED'});
 expect(await db.prepare('SELECT status,version,outstanding_minor FROM finance_invoices WHERE id=?').bind(source).first()).toEqual({status:'posted',version:2,outstanding_minor:10000});
 expect(await db.prepare('SELECT COUNT(*) count FROM finance_supplier_instructions WHERE invoice_id=?').bind(source).first('count')).toBe(0);expect(await db.prepare('SELECT COUNT(*) count FROM finance_payments WHERE company_id=?').bind(f.company).first('count')).toBe(0);expect(await db.prepare('SELECT COUNT(*) count FROM finance_journals WHERE company_id=?').bind(f.company).first('count')).toBe(journals);
 expect(await db.prepare('SELECT COUNT(*) count FROM command_receipts WHERE command_id IN (?,?)').bind(createKey,settleKey).first('count')).toBe(0);
});
it('blocks an exported supplier instruction when effective statutory review is installed before its actual bank confirmation',async()=>{
 const source=await bill(),batch=await exported(source),payer=await payerIdentity();await approvePayer(payer);const bankId=await bank(),journals=await db.prepare('SELECT COUNT(*) count FROM finance_journals WHERE company_id=?').bind(f.company).first('count');
 await expect(confirmSupplierInstruction(f.payer,String(batch.item.id),confirmation(bankId),key())).rejects.toMatchObject({status:409,code:'WITHHOLDING_OWNER_REQUIRED'});
 expect(await db.prepare('SELECT status,version,payment_id FROM finance_supplier_instructions WHERE id=?').bind(batch.item.id).first()).toEqual({status:'exported',version:2,payment_id:null});expect(await db.prepare('SELECT status,outstanding_minor FROM finance_invoices WHERE id=?').bind(source).first()).toEqual({status:'posted',outstanding_minor:10000});expect(await db.prepare('SELECT match_status,version FROM finance_bank_transactions WHERE id=?').bind(bankId).first()).toEqual({match_status:'unmatched',version:1});
 expect(await db.prepare('SELECT COUNT(*) count FROM finance_payments WHERE company_id=?').bind(f.company).first('count')).toBe(0);expect(await db.prepare('SELECT COUNT(*) count FROM finance_journals WHERE company_id=?').bind(f.company).first('count')).toBe(journals);
});
it('preserves actual gross supplier settlement outside the reviewed payer effective interval',async()=>{
 const payer=await payerIdentity('2026-09-10','2026-09-20');await approvePayer(payer);const source=await bill(),batch=await exported(source),bankId=await bank(),input=confirmation(bankId),command=key(),result=await confirmSupplierInstruction(f.payer,String(batch.item.id),input,command);expect(await confirmSupplierInstruction(f.payer,String(batch.item.id),input,command)).toEqual(result);
 expect(await db.prepare('SELECT status,outstanding_minor FROM finance_invoices WHERE id=?').bind(source).first()).toEqual({status:'paid',outstanding_minor:0});expect(await db.prepare('SELECT status,amount_minor,payment_date FROM finance_payments WHERE id=?').bind(result.paymentId).first()).toEqual({status:'settled',amount_minor:10000,payment_date:'2026-09-04'});expect(await db.prepare('SELECT matched_id FROM finance_bank_transactions WHERE id=?').bind(bankId).first('matched_id')).toBe(result.paymentId);
 expect(await db.prepare('SELECT SUM(base_debit_minor) debit,SUM(base_credit_minor) credit FROM finance_journal_lines WHERE journal_id=?').bind(result.journalId).first()).toEqual({debit:10000,credit:10000});expect(await db.prepare('SELECT COUNT(*) count FROM command_receipts WHERE command_id=?').bind(command).first('count')).toBe(1);
});
