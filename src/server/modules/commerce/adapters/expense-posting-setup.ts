import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import {isDeepStrictEqual} from 'node:util';
import {hasPermission} from '@/lib/identity_access/permissions';
import type {WorkContext} from '../../identity_access/public/authorization';
import {lockQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {lockPackageSettlementSources} from '../../commercial/public/package-locks';
import {companyPurchaseReadLocks,readPurchasedAccesses,checkPurchasedCapability,purchaseCommandBoundary,purchaseCommandFailure} from '../../commercial/public/purchase-command-boundary';
import {holdingResources} from '../../runtime/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {executeCommand,replayCommand,financialConstraintError} from '../../workflow/public/commands';
import {flushFinanceDeferredEffects} from '../../finance/public/finance-transactions';
import {validateCommercePostingFinanceReferences} from '../../finance/public/commerce-posting-setup';
import {requireExpensePostingFinanceAuthority,expensePostingFinanceReadLocks,expensePostingFinanceState,validateExpensePostingFinanceState,expensePostingEffectiveService} from '../../finance/public/expense-posting-setup-sources';
import {manufacturingPostingSourceReadLocks,manufacturingPostingSourceState} from '../../manufacturing/public/posting-setup-sources';
import {manufacturingPostingInventorySourceLocks,manufacturingPostingInventoryState} from '../../inventory/public/manufacturing-posting-setup-sources';
import {expensePostingSetupSourceLocks,expensePostingSetupSources} from '../../people_payroll/public/expense-posting-setup-sources';
import {posStartupCommerceReadLocks} from './pos-posting-setup-sources';
import {manufacturingPostingDocumentState} from './sales-goods-posting-setup';
import {expensePostingSetupSchema,type ExpensePostingSetupInput,type ExpensePostingSetupView,type ExpensePostingSetupResult} from '../domain/expense-posting-setup';
import type {CommercePostingSetupConfiguration} from '../domain/posting-setup';
const operation='commerce.expense-posting-setup.initialize',capabilities=['people.directory','people.expenses'] as const;
const columns={chartId:'chart_id',currency:'currency',receivableAccountId:'receivable_account_id',payableAccountId:'payable_account_id',revenueAccountId:'revenue_account_id',inventoryAccountId:'inventory_account_id',clearingAccountId:'clearing_account_id',cogsAccountId:'cogs_account_id',inputTaxAccountId:'input_tax_account_id',outputTaxAccountId:'output_tax_account_id',cashAccountId:'cash_account_id'} as const;
const fields=Object.keys(columns) as (keyof typeof columns)[];
const identity=(actor:WorkContext)=>JSON.stringify([actor.holdingId,actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam]);
const changed=()=>new ApiError(409,'EXPENSE_POSTING_SOURCE_CHANGED','The original Company mapping, economic history, Finance references or approved Sales defaults changed.');
async function owner(actor:WorkContext){const database=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(database))throw new ApiError(503,'TRANSACTION_REQUIRED','Expense posting setup requires its owned Holding transaction.');return database;}
async function leases(database:SqlDatabase,companyId:string,write=false){
 const purchase=companyPurchaseReadLocks(database,companyId);
 if(write){await lockPackageSettlementSources(database);await database.prepare('LOCK TABLE command_receipts,commerce_settings IN SHARE ROW EXCLUSIVE MODE').run();}else await database.batch([purchase[0]]);
 await database.batch([...posStartupCommerceReadLocks(database),...manufacturingPostingInventorySourceLocks(database),...manufacturingPostingSourceReadLocks(database),...expensePostingSetupSourceLocks(database),...expensePostingFinanceReadLocks(database)]);
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);if(write)await database.batch(purchase.slice(1));
}
async function state(database:SqlDatabase,companyId:string){
 const [mapping,documents,stock,production,expense]=await Promise.all([
  database.prepare('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE s.company_id=?').bind(companyId).first<string>('image'),
  manufacturingPostingDocumentState(database,companyId),manufacturingPostingInventoryState(database,companyId),manufacturingPostingSourceState(database,companyId),expensePostingSetupSources(database,companyId),
 ]);
 return {mapping:mapping?JSON.parse(mapping) as Record<string,unknown>:null,documents,history:documents.goods||stock.economics||production.economics||expense.economics,image:JSON.stringify([documents.image,stock.image,production.image,expense.image])};
}
type State=Awaited<ReturnType<typeof state>>;
const configuration=(row:Record<string,unknown>|null):CommercePostingSetupConfiguration|null=>row?Object.fromEntries(fields.map(field=>[field,String(row[columns[field]])])) as CommercePostingSetupConfiguration:null;
const matches=(row:Record<string,unknown>|null,input:ExpensePostingSetupInput,companyId:string)=>!!row&&row.company_id===companyId&&fields.every(field=>row[columns[field]]===input[field]);
const context=(actor:WorkContext,source:State,finance:{image:string;service:unknown})=>createHash('sha256').update(JSON.stringify([identity(actor),source,finance.image,finance.service])).digest('hex');
const empty=(companyId:string,unavailableReason:ExpensePostingSetupView['unavailableReason']):ExpensePostingSetupView=>({companyId,available:false,configured:null,version:null,canConfigure:false,contextKey:null,configuration:null,charts:[],accounts:[],expectedSalesDefaults:null,approvedSalesDefaults:null,unavailableReason});
export async function expensePostingSetup(actor:WorkContext):Promise<ExpensePostingSetupView>{
 const permission=hasPermission(actor,'finance.write')?'finance.write':'finance.approve';
 if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId||!hasPermission(actor,'finance.read')||!hasPermission(actor,permission))return empty(actor.companyId,'authority-required');
 return (await owner(actor)).transaction(async database=>{
  await leases(database,actor.companyId);const current=await requireExpensePostingFinanceAuthority(database,actor,permission),access=await readPurchasedAccesses(database,'enforced',actor.companyId,capabilities);
  if(!capabilities.every(capability=>access[capability].history))return empty(actor.companyId,'purchase-required');
  const before=await state(database,actor.companyId),finance=await expensePostingFinanceState(database,actor),mapping=configuration(before.mapping),history=before.history||finance.expenseEconomics||finance.historyInvalid,defaultsRequired=finance.defaultsBlocked||before.documents.history&&!finance.defaults;
  let valid=true;if(mapping){try{await validateCommercePostingFinanceReferences(database,actor,mapping);}catch(error){if(error instanceof ApiError&&error.status===422)valid=false;else throw error;}}
  if(identity(current)!==identity(await requireExpensePostingFinanceAuthority(database,actor,permission)))throw new ApiError(409,'EXPENSE_POSTING_CONTEXT_CHANGED','The selected Company Finance authority changed.');
  const after=await state(database,actor.companyId),finalFinance=await expensePostingFinanceState(database,actor);if(!isDeepStrictEqual(before,after)||finance.image!==finalFinance.image||!isDeepStrictEqual(finance.service,finalFinance.service))throw changed();
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});
  return {companyId:actor.companyId,available:true,configured:!!mapping,version:mapping?1:0,canConfigure:!mapping&&!history&&!defaultsRequired&&hasPermission(current,'finance.write')&&capabilities.every(capability=>access[capability].newWork),contextKey:context(current,before,finance),configuration:mapping,charts:finance.references.charts.filter(row=>row.status==='active').map(row=>({id:row.id,name:row.name,baseCurrency:row.base_currency})),accounts:finance.references.accounts.filter(row=>row.status==='active'&&typeof row.currency==='string').map(row=>({id:row.id,chartId:row.chart_id,code:row.code,name:row.name,category:row.category,currency:row.currency,controlKind:row.control_kind})),expectedSalesDefaults:finance.expectedSalesDefaults,approvedSalesDefaults:finance.defaults?{id:finance.defaults.id,version:Number(finance.defaults.version),chartId:finance.defaults.chart_id,currency:finance.defaults.currency,receivableAccountId:finance.defaults.receivable_account_id,revenueAccountId:finance.defaults.revenue_account_id,taxAccountId:finance.defaults.tax_account_id}:null,unavailableReason:mapping&&!valid?'invalid-existing-mapping':!mapping&&history?'history-requires-review':!mapping&&defaultsRequired?'sales-defaults-required':null};
 });
}
export async function initializeExpensePostingSetup(actor:WorkContext,raw:unknown,key:string):Promise<ExpensePostingSetupResult>{
 const input=expensePostingSetupSchema.parse(raw);
 try{return await (await owner(actor)).transaction(async database=>{
  await leases(database,actor.companyId,true);const current=await requireExpensePostingFinanceAuthority(database,actor),receipt=await database.prepare("SELECT to_jsonb(r)::text image FROM command_receipts r WHERE r.command_id=? AND r.operation=? AND r.actor_id=? AND r.record_id=? AND r.authority_json::jsonb->>'companyId'=?").bind(key,operation,actor.userId,actor.companyId,actor.companyId).first<string>('image'),purpose=receipt?'history' as const:'new-work' as const;
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
  const before=await state(database,actor.companyId),finance=await expensePostingFinanceState(database,actor),guard=`expense-posting-setup:${randomUUID()}`,boundaries=capabilities.map((capability,index)=>({key:`${guard}:${index}`,boundary:purchaseCommandBoundary(database,'enforced',`${guard}:${index}`,{companyId:actor.companyId,capability,purpose})!}));await database.batch(boundaries.flatMap(item=>item.boundary.before));
  let result:ExpensePostingSetupResult;
  if(receipt){const replay=await replayCommand<ExpensePostingSetupResult>(database,actor,key,operation,actor.companyId,input);if(!replay||replay.id!==actor.companyId||replay.status!=='configured'||replay.version!==1||!before.mapping)throw changed();result=replay;}
  else{
   if(input.contextKey!==context(current,before,finance))throw new ApiError(409,'EXPENSE_POSTING_CONTEXT_CHANGED','Refresh the original Company mapping, Finance references and approved defaults.');
   if(before.mapping)throw new ApiError(409,'COMMERCE_POSTING_ALREADY_CONFIGURED','Retain the actual Company mapping; initial Expense setup never overwrites it.');
   if(before.history||finance.expenseEconomics||finance.historyInvalid)throw new ApiError(409,'EXPENSE_POSTING_HISTORY_REQUIRES_REVIEW','Reconcile original goods, Stock, production or Expense economic history before initial setup.');
   await validateExpensePostingFinanceState(database,actor,input,before.documents.history);
   result=await executeCommand(database,actor,key,operation,input,actor.companyId,1,{id:actor.companyId,status:'configured' as const,version:1 as const},[database.prepare(`INSERT INTO commerce_settings(company_id,${fields.map(field=>columns[field]).join(',')}) VALUES(${Array.from({length:fields.length+1},()=>'?').join(',')})`).bind(actor.companyId,...fields.map(field=>input[field]))]);
  }
  const created=await state(database,actor.companyId),createdFinance=await expensePostingFinanceState(database,actor);
  if(before.image!==created.image||finance.image!==createdFinance.image||(receipt?!isDeepStrictEqual(before.mapping,created.mapping):!matches(created.mapping,input,actor.companyId))||(!!receipt||!!finance.defaults)&&!isDeepStrictEqual(finance.service,createdFinance.service))throw changed();
  const expectedReceipt=await database.prepare('SELECT to_jsonb(r)::text image FROM command_receipts r WHERE r.command_id=?').bind(key).first<string>('image');if(!expectedReceipt||receipt&&receipt!==expectedReceipt)throw changed();
  await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)]));await flushFinanceDeferredEffects(database);
  const finalSource=await state(database,actor.companyId),finalFinance=await expensePostingFinanceState(database,actor);if(!isDeepStrictEqual(created,finalSource)||createdFinance.image!==finalFinance.image||!isDeepStrictEqual(createdFinance.service,await expensePostingEffectiveService(database,actor))||expectedReceipt!==await database.prepare('SELECT to_jsonb(r)::text image FROM command_receipts r WHERE r.command_id=?').bind(key).first<string>('image'))throw changed();
  if(identity(current)!==identity(await requireExpensePostingFinanceAuthority(database,actor)))throw new ApiError(409,'EXPENSE_POSTING_CONTEXT_CHANGED','The original Company Finance authority changed.');
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});return result;
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
