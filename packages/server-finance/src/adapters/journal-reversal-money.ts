import 'server-only';
import {isDeepStrictEqual} from 'node:util';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase,type SqlStatement} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {financePackageEnforcementMode,checkPurchasedCapability,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import {financialConstraintError} from '@erpx/server-workflow/public/commands';
import {snapshotMoney,snapshotMoneyCarry,snapshotMoneyReversal} from '@erpx/server-finance/adapters/company-money';
import {requireFinanceMoneyScale} from '@erpx/server-finance/adapters/finance-money-policy';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/adapters/finance-transactions';
import {createFinancePolicy} from '@erpx/server-finance/domain/finance-policy';

type Row=Record<string,unknown>;
type Chain={id:string;depth:number;cycle:boolean;source_type:string;source_id:string|null;currency:string;total:string;source:string};
type Money=Row&{source_id:string;currency:string;currency_scale:number;amount_units:string;usd_units:string;fx_rate_id:string|null;used_latest_fallback:boolean};
type Image={chain:Chain[];journals:Row[];lines:Row[];money:Money[];openingInvoices:Row[];openingAllocations:Row[];period:Row|null;receipt:Row|null};
type Plan={at:number;periodId:string;entryDate:string;memo:string;money:Partial<Money>};
type Unit={actor:WorkContext;actorImage:string;journalId:string;key:string;before:Image;plan?:Plan};
const units=new WeakMap<SqlDatabase,Unit>();
const changed=()=>new ApiError(409,'JOURNAL_MONEY_SOURCE_CHANGED','The original journal, reversal lineage or immutable Money evidence changed.');
const parse=(value:{source:string})=>JSON.parse(value.source) as Row;
async function authority(database:SqlDatabase,actor:WorkContext){try{const current=await requireQueuedCommandAuthority(database,actor,'finance.write');createFinancePolicy((...args)=>new ApiError(...args)).requireFinanceRole(current,true);return current;}catch(error){if(error instanceof ApiError&&error.code==='QUEUED_AUTHORITY_CHANGED')throw new ApiError(403,'FINANCE_ACCESS_CHANGED','The original Finance assignment or Work context changed.');throw error;}}
async function image(database:SqlDatabase,actor:WorkContext,journalId:string,key:string,periodId:string):Promise<Image>{
 const chain=(await database.prepare(`WITH RECURSIVE ancestry AS (
  SELECT j.id,j.source_type,j.source_id,j.currency,to_jsonb(j)::text source,ARRAY[j.id] path,0 depth,false cycle FROM finance_journals j WHERE j.id=? AND j.company_id=?
  UNION ALL SELECT p.id,p.source_type,p.source_id,p.currency,to_jsonb(p)::text,a.path||p.id,a.depth+1,p.id=ANY(a.path) FROM ancestry a JOIN finance_journals p ON p.id=a.source_id AND p.company_id=? WHERE a.source_type='reversal' AND NOT a.cycle AND a.depth<100
 ) SELECT a.id,a.source_type,a.source_id,a.currency,a.source,a.depth,a.cycle,COALESCE((SELECT SUM(l.debit_minor) FROM finance_journal_lines l WHERE l.journal_id=a.id),0)::text total FROM ancestry a ORDER BY depth`).bind(journalId,actor.companyId,actor.companyId).all<Chain>()).results;
 const ids=[...new Set([...chain.map(row=>row.id),journalId,`reversal-${key}`])],slots=ids.map(()=>'?').join(',');
 const [journals,lines,money,openingInvoices,openingAllocations,period,receipt]=await Promise.all([
  database.prepare(`SELECT to_jsonb(j)::text source FROM finance_journals j WHERE j.company_id=? AND j.id IN (${slots}) ORDER BY j.id`).bind(actor.companyId,...ids).all<{source:string}>(),
  database.prepare(`SELECT to_jsonb(l)::text source FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.id IN (${slots}) ORDER BY l.journal_id,l.ordinal,l.id`).bind(actor.companyId,...ids).all<{source:string}>(),
  database.prepare(`SELECT (to_jsonb(m)||jsonb_build_object('amount_units',m.amount_units::text,'usd_units',m.usd_units::text))::text source FROM money_transaction_snapshots m WHERE m.company_id=? AND m.source_type='finance_journal' AND m.source_id IN (${slots}) ORDER BY m.source_id,m.amount_kind`).bind(actor.companyId,...ids).all<{source:string}>(),
  database.prepare(`SELECT to_jsonb(i)::text source FROM finance_invoices i WHERE i.company_id=? AND i.source_type='opening_import' AND i.source_id IN (${slots}) ORDER BY i.id`).bind(actor.companyId,...ids).all<{source:string}>(),
  database.prepare(`SELECT to_jsonb(a)::text source FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id WHERE i.company_id=? AND i.source_type='opening_import' AND i.source_id IN (${slots}) ORDER BY a.id`).bind(actor.companyId,...ids).all<{source:string}>(),
  database.prepare('SELECT to_jsonb(p)::text source FROM finance_periods p WHERE p.id=? AND p.company_id=?').bind(periodId,actor.companyId).first<{source:string}>(),
  database.prepare('SELECT to_jsonb(r)::text source FROM command_receipts r WHERE r.command_id=?').bind(key).first<{source:string}>(),
 ]);
 return {chain,journals:journals.results.map(parse),lines:lines.results.map(parse),money:money.results.map(row=>parse(row) as Money),openingInvoices:openingInvoices.results.map(parse),openingAllocations:openingAllocations.results.map(parse),period:period?parse(period):null,receipt:receipt?parse(receipt):null};
}
function ancestry(unit:Unit){
 const rows=unit.before.chain,last=rows.at(-1);
 if(!rows.length||!last||last.cycle||rows.some(row=>row.cycle)||!['manual','opening_import'].includes(last.source_type)||last.depth!==rows.length-1)throw changed();
 const total=BigInt(rows[0].total),currency=rows[0].currency;
 if(total<=BigInt(0)||rows.some((row,index)=>row.depth!==index||row.currency!==currency||BigInt(row.total)!==total||index<rows.length-1&&(row.source_type!=='reversal'||row.source_id!==rows[index+1].id)))throw changed();
 return {rows,total,currency};
}
function expectedSign(rows:Chain[],index:number){return (rows.length-1-index)%2===0?BigInt(1):-BigInt(1);}
function sourceMoney(unit:Unit,rows:Chain[],total:bigint,currency:string){
 for(let index=0;index<rows.length;index++){
  const source=unit.before.money.find(row=>row.source_id===rows[index].id&&row.amount_kind==='debit_total');
  if(!source)continue;
  const sign=expectedSign(rows,index);
  if(source.currency!==currency||source.currency_scale!==2||source.transaction_date!==parse(rows[index]).entry_date||BigInt(source.amount_units)!==sign*total||sign*BigInt(source.usd_units)<BigInt(0)||currency==='USD'&&(source.fx_rate_id!==null||source.usd_units!==source.amount_units)||currency!=='USD'&&typeof source.fx_rate_id!=='string')throw changed();
  // Every captured ancestry row is actual same-Company native journal evidence.
  // A negative return origin, when present, must be on this true parent chain.
  if(source.original_source_type!==null){
   const originIndex=rows.findIndex((row,at)=>at>index&&row.id===source.original_source_id),origin=unit.before.money.find(row=>row.source_id===source.original_source_id&&row.amount_kind==='debit_total');
   if(source.original_source_type!=='finance_journal'||source.original_amount_kind!=='debit_total'||originIndex<0||!origin||BigInt(origin.amount_units)!==total||BigInt(source.amount_units)!==-BigInt(origin.amount_units)||BigInt(source.usd_units)!==-BigInt(origin.usd_units)||source.fx_rate_id!==origin.fx_rate_id||source.currency!==origin.currency||source.currency_scale!==origin.currency_scale||source.used_latest_fallback!==origin.used_latest_fallback)throw changed();
  }
  return {source,index};
 }
 return null;
}
/** Fixed private bridge for the original manual/opening reversal application.
 * Positive sources retain the original native allocation guard. Negative
 * sources are inverted from their actual immutable row, never sent to that API. */
export async function prepareJournalReversalMoney(database:SqlDatabase,actor:WorkContext,facts:{journalId:string;reversalId:string;key:string;periodId:string;entryDate:string;memo:string;at:number}):Promise<SqlStatement>{
 const unit=units.get(database);
 if(!unit||unit.actorImage!==JSON.stringify(actor)||unit.journalId!==facts.journalId||unit.key!==facts.key||facts.reversalId!==`reversal-${facts.key}`)throw new ApiError(503,'JOURNAL_REVERSAL_TRANSACTION_REQUIRED','Use the original owned Finance reversal command.');
 const {rows,total,currency}=ancestry(unit),captured=sourceMoney(unit,rows,total,currency),identity={sourceType:'finance_journal',sourceId:facts.reversalId,amountKind:'debit_total',transactionDate:facts.entryDate};
 let statement:SqlStatement,money:Partial<Money>;
 if(captured){
  const {source,index}=captured,factor=(index+1)%2===0?BigInt(1):-BigInt(1),amount=BigInt(source.amount_units)*factor,usd=BigInt(source.usd_units)*factor,original={sourceType:'finance_journal',sourceId:source.source_id,amountKind:'debit_total'};
  money={currency:source.currency,currency_scale:source.currency_scale,amount_units:String(amount),usd_units:String(usd),fx_rate_id:source.fx_rate_id,used_latest_fallback:source.used_latest_fallback};
  if(BigInt(source.amount_units)>BigInt(0)){
   if(amount<BigInt(0)){statement=(await snapshotMoneyReversal(database,actor,{...identity,amountUnits:String(total),original})).statement;money={...money,original_source_type:original.sourceType,original_source_id:original.sourceId,original_amount_kind:original.amountKind};}
   else{statement=(await snapshotMoneyCarry(database,actor,{...identity,amountUnits:String(total),original})).statement;money={...money,carried_source_type:original.sourceType,carried_source_id:original.sourceId,carried_amount_kind:original.amountKind};}
  }else{
   // The original signed allocation API rejects negative origins. Copy only
   // this exact immutable source with its computed sign; native journal.source_id
   // retains the real parent. No allocation guard is disabled or old row changed.
   statement=database.prepare(`INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at)
    SELECT company_id,'finance_journal',?,'debit_total',?,currency,currency_scale,amount_units*?::numeric,usd_units*?::numeric,fx_rate_id,used_latest_fallback,? FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_journal' AND source_id=? AND amount_kind='debit_total'`).bind(facts.reversalId,facts.entryDate,String(factor),String(factor),facts.at,actor.companyId,source.source_id);
  }
 }else{
  // Old journals without any captured ancestor receive evidence ONLY for the
  // genuinely new reversal, dated now in business time. Never backfill history.
  await requireFinanceMoneyScale(database,actor.companyId,currency);
  const amount=-expectedSign(rows,0)*total,prepared=await snapshotMoney(database,actor,{...identity,currency,currencyScale:2,amountUnits:String(amount)});
  statement=prepared.statement;money={currency,currency_scale:2,amount_units:String(amount),usd_units:prepared.usdUnits,fx_rate_id:prepared.rateId,used_latest_fallback:prepared.usedLatestFallback};
 }
 unit.plan={at:facts.at,periodId:facts.periodId,entryDate:facts.entryDate,memo:facts.memo,money};return statement;
}
function validateNew(unit:Unit,after:Image){
 const plan=unit.plan;if(!plan)throw changed();
 const old=unit.before.journals.find(row=>row.id===unit.journalId),source=after.journals.find(row=>row.id===unit.journalId),id=`reversal-${unit.key}`,created=after.journals.find(row=>row.id===id);
 if(!old||!source||!created||unit.before.journals.some(row=>row.id===id)||!isDeepStrictEqual(source,{...old,status:'reversed',version:Number(old.version)+1,reversed_by:unit.actor.userId,reversed_at:plan.at,updated_at:plan.at}))throw changed();
 for(const row of unit.before.journals)if(row.id!==unit.journalId&&!isDeepStrictEqual(row,after.journals.find(current=>current.id===row.id)))throw changed();
 const expected={company_id:unit.actor.companyId,chart_id:old.chart_id,period_id:plan.periodId,entry_date:plan.entryDate,currency:old.currency,fx_rate_micros:old.fx_rate_micros,source_type:'reversal',source_id:unit.journalId,memo:plan.memo,status:'posted',version:1,created_by:unit.actor.userId,posted_by:unit.actor.userId,posted_at:plan.at,created_at:plan.at,updated_at:plan.at};
 if(Object.entries(expected).some(([key,value])=>!isDeepStrictEqual(created[key],value)))throw changed();
 const originalLines=unit.before.lines.filter(row=>row.journal_id===unit.journalId),reversedLines=after.lines.filter(row=>row.journal_id===id),fields=['id','journal_id','account_id','description','debit_minor','credit_minor','base_debit_minor','base_credit_minor','dimension_json','ordinal'];
 if(reversedLines.length!==originalLines.length||!isDeepStrictEqual(unit.before.lines,after.lines.filter(row=>row.journal_id!==id)))throw changed();
 for(const row of originalLines){const inverse:Row={...row,id:`${id}-${row.ordinal}`,journal_id:id,debit_minor:row.credit_minor,credit_minor:row.debit_minor,base_debit_minor:row.base_credit_minor,base_credit_minor:row.base_debit_minor},actual=reversedLines.find(line=>line.ordinal===row.ordinal);if(!actual||fields.some(key=>!isDeepStrictEqual(actual[key],inverse[key])))throw changed();}
 const money=after.money.filter(row=>row.source_id===id);if(money.length!==1||!isDeepStrictEqual(unit.before.money,after.money.filter(row=>row.source_id!==id)))throw changed();
 if(money[0].amount_kind!=='debit_total'||money[0].transaction_date!==plan.entryDate||Object.entries(plan.money).some(([key,value])=>!isDeepStrictEqual(money[0][key],value)))throw changed();
 const invoices=unit.before.openingInvoices.map(row=>old.source_type==='opening_import'&&row.source_id===unit.journalId?{...row,status:'voided',outstanding_minor:0,base_outstanding_minor:0,version:Number(row.version)+1,updated_at:plan.at}:row);
 if(!isDeepStrictEqual(invoices,after.openingInvoices)||!isDeepStrictEqual(unit.before.openingAllocations,after.openingAllocations)||!isDeepStrictEqual(unit.before.period,after.period))throw changed();
 const receipt=after.receipt;
 if(!receipt||receipt.operation!=='finance.journal.reverse'||receipt.actor_id!==unit.actor.userId||receipt.record_id!==unit.journalId||!isDeepStrictEqual(JSON.parse(String(receipt.result_json)),{reversalJournalId:id,reversedJournalId:unit.journalId}))throw changed();
}
/** Rare manual/opening reversals hold a complete short unit. The strong
 * package/receipt fences precede sources/advisories, including disabled legacy
 * commands. This serializes unrelated Commercial writers; no perf claim follows. */
export async function withJournalReversalMoneyUnit<Result>(actor:WorkContext,journalId:string,key:string,periodId:string,run:(database:SqlDatabase)=>Promise<Result>):Promise<Result>{
 const owner=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'JOURNAL_REVERSAL_TRANSACTION_REQUIRED','Finance reversal requires its original Holding transaction.');
 const mode=await financePackageEnforcementMode();
 try{return await owner.transaction(async database=>{
  await lockPackageSettlementSources(database);
  // Legacy disabled commands can lock their selected Work row BEFORE receipt.
  // Exclude that RowShare phase before taking receipt SRE, without an early
  // SELECT/snapshot or any change to generic Workflow/legacy command policy.
  await database.prepare('LOCK TABLE active_work_contexts IN EXCLUSIVE MODE').run();
  await database.prepare('LOCK TABLE command_receipts,finance_journals,finance_journal_lines,money_transaction_snapshots,finance_invoices,finance_allocations IN SHARE ROW EXCLUSIVE MODE').run();
  await database.prepare('LOCK TABLE finance_periods IN SHARE MODE').run();
  await lockQueuedCommandAuthority(database);
  const current=await authority(database,actor),before=await image(database,actor,journalId,key,periodId),unit:Unit={actor,actorImage:JSON.stringify(actor),journalId,key,before};units.set(database,unit);
  try{
   const result=await run(database),after=await image(database,actor,journalId,key,periodId);
   if(unit.plan)validateNew(unit,after);else if(!isDeepStrictEqual(before,after))throw changed();
   // Original command + purchase-boundary assertion cleanup is complete. Flush
   // all deferred work before the final READS, with no subsequent owner writes.
   await flushFinanceDeferredEffects(database);
   if(unit.actorImage!==JSON.stringify(actor)||!isDeepStrictEqual(after,await image(database,actor,journalId,key,periodId)))throw changed();
   const final=await authority(database,actor);if(!isDeepStrictEqual(current,final))throw new ApiError(403,'FINANCE_ACCESS_CHANGED','The original Finance assignment or Work context changed.');
   await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'finance.accounting',purpose:before.receipt?'history':'settle-existing'});
   return result;
  }finally{units.delete(database);}
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
