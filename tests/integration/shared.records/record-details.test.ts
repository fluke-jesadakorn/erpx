import {requiredFact,recordField} from "../../support/helpers/required-fact";
import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {saveMaster,commercePage} from "@erpx/server-commerce/public/commerce";
import {recordSupplementData} from "@erpx/server-records/public/record-details";
import {createJournal,createJournalSchema,postJournal,importBankStatement,trialBalance} from "@erpx/server-finance/public/finance";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:db,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();});
it('reads a product by ID beyond the register page and respects current read/write authority',async()=>{
 const ids=[];
 for(let n=0;n<35;n++)ids.push(String((await saveMaster(f.owner,'products',null,{sku:`${key()}-${n}`,name:`Drawer product ${n}`,unit:'each',price:'5'},key())).id));
 const page=await commercePage(f.owner,{view:'products'});
 const id=ids.find(id=>!page.products.some(row=>row.id===id))!;
 expect(id).toBeTruthy();
 const reader={...f.owner,iam:{permissions:['commerce.read'],grantablePermissions:[],principalId:'principal',personId:null,roleRevision:1,identityVersion:1,freshUntil:9999999999999}};
 const detail=await recordSupplementData(reader,{kind:'product',id},{});
 expect(recordField(detail.row,"id")).toBe(id);expect('canEdit' in detail&&detail.canEdit).toBe(false);
 await expect(recordSupplementData({...reader,iam:{...reader.iam,permissions:[]}},{kind:'product',id},{})).rejects.toMatchObject({status:403});
 await expect(recordSupplementData({...reader,companyId:key()},{kind:'product',id},{})).rejects.toMatchObject({status:404});
});
it('paginates stock sources by product and warehouse and refuses another branch',async()=>{
 const product=String((await saveMaster(f.owner,'products',null,{sku:key(),name:'Stock drawer product',price:'2'},key())).id);
 const warehouse=String((await saveMaster(f.owner,'warehouses',null,{code:key(),name:'Stock drawer store',branchId:f.branch},key())).id);
 await db.prepare('INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) VALUES(?,?,2000,400)').bind(warehouse,product).run();
 const detail=await recordSupplementData(f.owner,{kind:'stock_position',id:product},{warehouseId:warehouse});
 expect(recordField(detail.row,"quantity_milli")).toBe(2000);
 await expect(recordSupplementData({...f.owner,branchId:key()},{kind:'stock_position',id:product},{warehouseId:warehouse})).rejects.toMatchObject({status:404});
 await expect(recordSupplementData(f.owner,{kind:'stock_position',id:product},{warehouseId:warehouse,detailPage:'-1'})).rejects.toThrow();
});
it('refreshes a saved master in the register and detail and rejects an older drawer version',async()=>{
 const input={sku:key(),name:'Original drawer product',unit:'each',price:'5',taxBps:0};
 const id=String((await saveMaster(f.owner,'products',null,input,key())).id);
 const original=await recordSupplementData(f.owner,{kind:'product',id},{});
 const expectedVersion=Number(recordField(original.row,"version"));
 await saveMaster(f.owner,'products',id,{...input,name:'Updated drawer product',expectedVersion},key());
 const updated=await recordSupplementData(f.owner,{kind:'product',id},{});
 expect(updated.row).toMatchObject({name:'Updated drawer product',version:expectedVersion+1});
 const register=await commercePage(f.owner,{view:'products',q:'Updated drawer product'});
 expect(register.products.find(row=>row.id===id)).toMatchObject({name:'Updated drawer product',version:expectedVersion+1});
 await expect(saveMaster(f.owner,'products',id,{...input,name:'Stale overwrite',expectedVersion},key())).rejects.toMatchObject({status:409});
 expect(recordField((await recordSupplementData(f.owner,{kind:'product',id},{})).row,'name')).toBe('Updated drawer product');
});
it('keeps account drilldowns in the selected company and ledger with the owner report predicates',async()=>{
 const totals=await trialBalance(f.owner,undefined,f.chart,{startDate:'2026-01-01',endDate:'2026-12-31'});
 const detail=await recordSupplementData(f.owner,{kind:'trial_balance',id:f.accounts.expense},{chartId:f.chart,startDate:'2026-01-01',endDate:'2026-12-31'});
 expect(recordField(detail.row,"id")).toBe(f.accounts.expense);
 const expected=totals.find(row=>row.id===f.accounts.expense);
 if(expected)expect(recordField(detail.row,"balance")).toBe(expected.balance);
 await expect(recordSupplementData(f.owner,{kind:'trial_balance',id:f.accounts.expense},{chartId:key()})).rejects.toMatchObject({status:404});
 await expect(recordSupplementData({...f.owner,companyId:key()},{kind:'trial_balance',id:f.accounts.expense},{})).rejects.toMatchObject({status:404});
 await expect(recordSupplementData(f.claimant,{kind:'trial_balance',id:f.accounts.expense},{})).rejects.toMatchObject({status:403});
});

it('paginates posted journal sources without losing date filters or changing the reported total',async()=>{
 f.owner.role='finance_preparer';f.owner.assignmentId=await f.grant(f.owner,'finance_preparer');
 f.manager.role='finance_approver';f.manager.assignmentId=await f.grant(f.manager,'finance_approver');
 for(let index=0;index<32;index++){
  const prepared=await createJournal(f.owner,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:'2026-09-04',currency:'THB',lines:[{accountId:f.accounts.expense,debit:'1.01',credit:'0'},{accountId:f.accounts.payable,debit:'0',credit:'1.01'}]}),key());
  await postJournal(f.manager,String(prepared.id),key(),{expectedVersion:1});
 }
 const target={kind:'trial_balance' as const,id:f.accounts.expense},args={chartId:f.chart,startDate:'2026-09-04',endDate:'2026-09-04'};
 const first=await recordSupplementData(f.owner,target,args),second=await recordSupplementData(f.owner,target,{...args,detailPage:'2'});
 expect(recordField(first.row,"balance")).toBe('32.32');expect(recordField(second.row,"balance")).toBe('32.32');
 expect('rows' in first&&requiredFact(first.rows).slice(0,30)).toHaveLength(30);expect('hasNext' in first&&first.hasNext).toBe(true);
 expect('rows' in second&&second.rows).toHaveLength(2);
 const excluded=await recordSupplementData(f.owner,target,{...args,endDate:'2026-09-03',startDate:'2026-09-03'});
 expect('rows' in excluded&&excluded.rows).toHaveLength(0);
});
it('reads a bank movement outside the current cursor and paginates the exact statement',async()=>{
 const k=key(),created=await importBankStatement(f.owner,{bankAccount:f.accounts.cash,statementNumber:key(),statementDate:'2026-09-04',opening:'0',closing:'32',sourceHash:key(),rows:Array.from({length:32},(_,i)=>({externalId:`${k}-${i}`,transactionDate:'2026-09-04',amount:'1',currency:'THB'}))},k);
 const id=String(created.id),page=await recordSupplementData(f.owner,{kind:'bank_statement',id},{detailPage:'2'});
 expect('rows' in page&&page.rows).toHaveLength(2);
 const outside=await recordSupplementData(f.owner,{kind:'bank_transaction',id:`bankrow-${k}-31`},{});
 expect(recordField(outside.row,"id")).toBe(`bankrow-${k}-31`);
 await expect(recordSupplementData({...f.owner,companyId:key()},{kind:'bank_transaction',id:`bankrow-${k}-31`},{})).rejects.toMatchObject({status:404});
});
