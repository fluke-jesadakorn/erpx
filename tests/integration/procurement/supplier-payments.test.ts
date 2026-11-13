import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {listSupplierBatches,supplierBatchWorkspace,supplierBatchWork} from "@erpx/server-procurement/public/supplier-payments";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{
 f=await expenseFixture();
 for(const [actor,role] of [[f.reviewer,'finance_preparer'],[f.manager,'finance_approver'],[f.payer,'treasury']] as const){actor.role=role;actor.branchId=null;actor.departmentId=null;actor.assignmentId=await f.grant(actor,role);}
});
// The native Finance supplier-batch command owns write behavior. This suite
// retains the Procurement read boundary over native persisted rows only.
async function seedBatch(){
 const invoice=`supplier-invoice-${key()}`,journal=`supplier-journal-${key()}`,batchId=`supplier-batch-${key()}`,instructionId=`${batchId}-0`,now=Date.now(),party=`supplier-${key()}`;
 await db.batch([
  db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) VALUES(?,?,'supplier',?,'Seeded supplier','THB','active',1,?,?)").bind(party,f.company,key(),now,now),
  db.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,1000000,'supplier_bill',?,'Seeded payable','posted',?,?,?,?,?)").bind(journal,f.company,f.chart,f.period,`J-${key()}`,`2026-09-02`,'THB',invoice,f.reviewer.userId,f.manager.userId,now,now,now),
  db.prepare("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,base_total_minor,base_outstanding_minor,debit_account_id,credit_account_id,fx_rate_micros,journal_id,status,version,created_by,created_at,updated_at,source_type) VALUES(?,?,?,'payable',?,'2026-09-01','2026-09-10','THB',10000,0,10000,10000,10000,10000,?,?,1000000,?,'posted',1,?,?,?,'standalone')").bind(invoice,f.company,party,key(),f.accounts.expense,f.accounts.payable,journal,f.reviewer.userId,now,now),
  db.prepare("INSERT INTO finance_supplier_batches(id,company_id,currency,bank_account_id,planned_date,status,approved_by,approval_reason,exported_by,export_command_id,export_hash,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'exported',?,?,?,?,?,3,?,?,?)").bind(batchId,f.company,'THB',f.accounts.cash,'2026-09-03',f.manager.userId,'Seeded independent approval',f.payer.userId,`${batchId}-export`,'0'.repeat(64),f.reviewer.userId,now,now),
  db.prepare("INSERT INTO finance_supplier_instructions(id,batch_id,invoice_id,invoice_version,amount_minor,beneficiary_name,beneficiary_account,status,version,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'exported',2,?,?)").bind(instructionId,batchId,invoice,2,10000,'Seeded supplier','SEEDED-ACCOUNT-001',now,now),
 ]);
 return {batchId,instructionId,invoiceId:invoice};
}
it('lists and reads native exported supplier batches without a TypeScript writer',async()=>{
 const {batchId,instructionId,invoiceId}=await seedBatch();
 const listed=await listSupplierBatches(f.payer);
 expect(listed.find(row=>row.id===batchId)).toMatchObject({status:'exported',version:3,pending_count:1});
 const workspace=await supplierBatchWorkspace(f.payer,batchId);
 expect(workspace.batch).toMatchObject({id:batchId,status:'exported',currency:'THB'});
 expect(workspace.items.find(row=>row.id===instructionId)).toMatchObject({invoice_id:invoiceId,invoice_number:expect.any(String),amount_minor:10000,status:'exported'});
 const work=await supplierBatchWork(f.manager);
 expect(work.find(row=>row.id===batchId)).toMatchObject({type:'supplier_batch',status:'exported'});
});
it('rejects an unknown native batch without inventing a local row',async()=>{
 await expect(supplierBatchWorkspace(f.payer,`supplier-batch-${key()}`)).rejects.toMatchObject({code:'BATCH_NOT_FOUND'});
});
