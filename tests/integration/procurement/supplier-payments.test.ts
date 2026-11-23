import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {listSupplierBatches,supplierBatchWorkspace,supplierBatchWork} from "@erpx/server-procurement/public/supplier-payments";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
// The owner plans these exact statements; this double executes them on the same
// open unit so the Procurement read boundary stays exercised against real rows.
vi.mock("../../../packages/server-finance/src/adapters/native-finance-read",()=>{
 const owned="(NOT ? OR (EXISTS(SELECT 1 FROM finance_supplier_instructions owned_item WHERE owned_item.batch_id=b.id) AND NOT EXISTS(SELECT 1 FROM finance_supplier_instructions owned_item WHERE owned_item.batch_id=b.id AND NOT EXISTS(SELECT 1 FROM (SELECT o.company_id,f.invoice_id id FROM commerce_orders o JOIN commerce_fulfillments f ON f.order_id=o.id JOIN finance_invoices i ON i.id=f.invoice_id AND i.company_id=o.company_id AND i.kind='payable' WHERE o.kind='purchase') owned_source WHERE owned_source.id=owned_item.invoice_id AND owned_source.company_id=b.company_id))))";
 const counts="SELECT b.*,(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status IN ('reserved','exported','unknown')) pending_count,(SELECT count(*) FROM finance_supplier_instructions i JOIN finance_invoices f ON f.id=i.invoice_id WHERE i.batch_id=b.id AND i.status='failed' AND f.outstanding_minor>0) failed_count,(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status='unknown') unknown_count FROM finance_supplier_batches b WHERE b.company_id=? AND "+owned+" ORDER BY b.updated_at DESC";
 const picker="SELECT i.*,p.name party_name,i.outstanding_minor-COALESCE((SELECT SUM(s.amount_minor) FROM finance_supplier_instructions s WHERE s.invoice_id=i.id AND s.status IN ('reserved','exported','unknown')),0) available_minor FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id WHERE i.company_id=? AND i.kind='payable' AND i.source_type IN ('standalone','commerce_fulfillment') AND i.status IN ('posted','partially_paid') AND (NOT ? OR EXISTS(SELECT 1 FROM (SELECT o.company_id,f.invoice_id id FROM commerce_orders o JOIN commerce_fulfillments f ON f.order_id=o.id JOIN finance_invoices i ON i.id=f.invoice_id AND i.company_id=o.company_id AND i.kind='payable' WHERE o.kind='purchase') owner_invoice WHERE owner_invoice.id=i.id AND owner_invoice.company_id=i.company_id))";
 const sql:Record<string,string>={
  finance_rd_supplier_batch_row:'SELECT "id","company_id","currency","bank_account_id","planned_date","status","created_by","approved_by","approval_reason","exported_by","export_command_id","export_hash","version","created_at","updated_at" FROM finance_supplier_batches WHERE id=? AND company_id=?',
  finance_rd_supplier_instruction_row:"SELECT i.* FROM finance_supplier_instructions i JOIN finance_supplier_batches b ON b.id=i.batch_id WHERE i.id=? AND b.company_id=?",
  finance_rd_supplier_batch_list:counts+" LIMIT 200",
  finance_rd_supplier_batch_work_list:counts,
  finance_rd_supplier_batch_items:"SELECT s.*,i.invoice_number,i.currency FROM finance_supplier_instructions s JOIN finance_invoices i ON i.id=s.invoice_id WHERE s.batch_id=? ORDER BY s.id",
  finance_rd_supplier_batch_invoices:picker+" ORDER BY i.due_date,i.id LIMIT 200",
  finance_rd_supplier_batch_invoice:picker+" AND i.id=? ORDER BY i.due_date,i.id LIMIT 200",
  finance_rd_supplier_batch_accounts:"SELECT a.id,a.code,a.name,a.currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.category='asset' AND a.status='active' AND a.id=(SELECT cash_account_id FROM commerce_settings WHERE company_id=a.company_id) AND c.status='active' AND a.currency=c.base_currency ORDER BY a.code",
  finance_rd_supplier_batch_periods:"SELECT id,name FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date",
  finance_rd_supplier_batch_bank:"SELECT t.*,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.company_id=? AND s.bank_account=? AND t.match_status IN ('unmatched','proposed') AND (NOT ? OR (t.currency=? AND t.transaction_date>=? AND EXISTS(SELECT 1 FROM finance_supplier_instructions instruction WHERE instruction.batch_id=? AND instruction.status IN ('exported','unknown') AND t.amount_minor=-instruction.amount_minor))) ORDER BY t.transaction_date DESC LIMIT 200",
 };
 const all=async(kind:string,values:readonly unknown[],db:typeof env.H001_DB)=>({results:(await db.prepare(sql[kind]).bind(...values).all()).results,success:true,meta:{changes:0,duration:0}});
 return {
  FINANCE_RD_SUPPLIER_BATCH_ROW:"finance_rd_supplier_batch_row",FINANCE_RD_SUPPLIER_INSTRUCTION_ROW:"finance_rd_supplier_instruction_row",
  FINANCE_RD_SUPPLIER_BATCH_LIST:"finance_rd_supplier_batch_list",FINANCE_RD_SUPPLIER_BATCH_WORK_LIST:"finance_rd_supplier_batch_work_list",
  FINANCE_RD_SUPPLIER_BATCH_ITEMS:"finance_rd_supplier_batch_items",FINANCE_RD_SUPPLIER_BATCH_INVOICES:"finance_rd_supplier_batch_invoices",
  FINANCE_RD_SUPPLIER_BATCH_INVOICE:"finance_rd_supplier_batch_invoice",FINANCE_RD_SUPPLIER_BATCH_ACCOUNTS:"finance_rd_supplier_batch_accounts",
  FINANCE_RD_SUPPLIER_BATCH_PERIODS:"finance_rd_supplier_batch_periods",FINANCE_RD_SUPPLIER_BATCH_BANK:"finance_rd_supplier_batch_bank",
  nativeFinanceReadAll:all,nativeFinanceReadFirst:async(kind:string,values:readonly unknown[],db:typeof env.H001_DB)=>(await db.prepare(sql[kind]).bind(...values).first())??null,
 };
});
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
