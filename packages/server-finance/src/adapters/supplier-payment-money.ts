import 'server-only';
import {isDeepStrictEqual} from 'node:util';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase,type SqlStatement} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {financePackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import {commerceFinanceRecordSources} from '@erpx/server-commerce/public/finance-record-sources';
import {financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {replayCommand} from '@erpx/server-workflow/public/commands';
import {supplierPaymentOriginalSources} from '@erpx/server-finance/adapters/supplier-payment-correction-sources';
import {financeSourceDocumentReadLeases} from '@erpx/server-finance/adapters/document-purchases';
import {snapshotMoney,snapshotMoneyReversal} from '@erpx/server-finance/adapters/company-money';
import {requireFinanceMoneyScale} from '@erpx/server-finance/adapters/finance-money-policy';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/adapters/finance-transactions';

type Row=Record<string,unknown>;
type ConfirmInput={expectedVersion:number;bankTransactionId:string;bankVersion:number;periodId:string;bankReference:string;evidence:string;attachmentSetIds:string[]};
type ReverseInput={expectedVersion:number;periodId:string;entryDate:string;reason:string};
type Command={kind:'confirm';instructionId:string;key:string;input:ConfirmInput}|{kind:'reverse';paymentId:string;key:string;input:ReverseInput};
type Money=Row&{source_type:string;source_id:string;amount_kind:string;currency:string;currency_scale:number;amount_units:string;usd_units:string;fx_rate_id:string|null;used_latest_fallback:boolean};
type Image={batch:Row|null;instructions:Row[];invoice:Row|null;payment:Row|null;bank:Row|null;statement:Row|null;journals:Row[];lines:Row[];events:Row[];books:Row[];allocations:Row[];reversals:Row[];money:Money[];sourceReceipts:Row[];receipt:Row|null;purchaseOwned:boolean;native:boolean};
type Plan={kind:Command['kind'];at:number;paymentId:string;journalId:string;money:Partial<Money>};
type Unit={actorImage:string;command:Command;before:Image;plan?:Plan};
const units=new WeakMap<SqlDatabase,Unit>();
const parse=(row:{source:string})=>JSON.parse(row.source) as Row;
const changed=()=>new ApiError(409,'SUPPLIER_MONEY_SOURCE_CHANGED','The original Supplier instruction, bank, accounting source or immutable Money evidence changed.');
function equal(a:unknown,b:unknown){if(!isDeepStrictEqual(a,b))throw changed();}
function matches(row:Row|undefined|null,fields:Row){if(!row||Object.entries(fields).some(([key,value])=>!isDeepStrictEqual(row[key],value)))throw changed();}
function requireReceiptCompany(receipt:Row|null,companyId:string){
 if(!receipt||typeof receipt.authority_json!=='string')throw changed();
 let authority:unknown;try{authority=JSON.parse(receipt.authority_json);}catch{throw changed();}
 if(!authority||typeof authority!=='object'||Array.isArray(authority)||!('companyId' in authority)||authority.companyId!==companyId)throw changed();
}
async function rows(database:SqlDatabase,sql:string,args:(string|number|null)[]=[]){return (await database.prepare(sql).bind(...args).all<{source:string}>()).results.map(parse);}
async function one(database:SqlDatabase,sql:string,args:(string|number|null)[]=[]){const row=await database.prepare(sql).bind(...args).first<{source:string}>();return row?parse(row):null;}
async function authority(database:SqlDatabase,actor:WorkContext,kind:Command['kind']){
 try{
  const current=await requireQueuedCommandAuthority(database,actor,kind==='confirm'?'finance.pay':'finance.write');
  requirePermission(current,'finance.read');if(kind==='reverse')requirePermission(current,'finance.approve');
  return current;
 }catch(error){if(error instanceof ApiError&&error.code==='QUEUED_AUTHORITY_CHANGED')throw new ApiError(403,'FINANCE_ACCESS_CHANGED','The original Finance assignment or Work context changed.');throw error;}
}
async function references(database:SqlDatabase,companyId:string){
 // Keep exact NUMERIC text: parsing a 38-digit quote into a JS number would
 // lose low-order decimal changes in the final source comparison.
 const definitions=[...['company_money_settings','company_currency_scales','company_fx_rates','finance_charts','finance_accounts','finance_periods','finance_book_rules'].map(table=>`SELECT to_jsonb(r)::text source FROM ${table} r WHERE r.company_id=? ORDER BY to_jsonb(r)::text`),'SELECT to_jsonb(m)::text source FROM finance_book_account_mappings m JOIN finance_book_rules r ON r.id=m.rule_id WHERE r.company_id=? ORDER BY m.rule_id,m.primary_account_id'];
 return Promise.all(definitions.map(async sql=>(await database.prepare(sql).bind(companyId).all<{source:string}>()).results.map(row=>row.source)));
}
async function image(database:SqlDatabase,actor:WorkContext,command:Command):Promise<Image>{
 const instruction=await one(database,`SELECT to_jsonb(i)::text source FROM finance_supplier_instructions i JOIN finance_supplier_batches b ON b.id=i.batch_id WHERE ${command.kind==='confirm'?'i.id=?':'i.payment_id=?'} AND b.company_id=?`,[command.kind==='confirm'?command.instructionId:command.paymentId,actor.companyId]);
 const batchId=typeof instruction?.batch_id==='string'?instruction.batch_id:'',invoiceId=typeof instruction?.invoice_id==='string'?instruction.invoice_id:'',paymentId=command.kind==='reverse'?command.paymentId:typeof instruction?.payment_id==='string'?instruction.payment_id:`supplier-payment-${command.key}`,bankId=command.kind==='confirm'?command.input.bankTransactionId:typeof instruction?.bank_transaction_id==='string'?instruction.bank_transaction_id:'';
 const [batch,instructions,invoice,payment,bank]=await Promise.all([
  one(database,'SELECT to_jsonb(b)::text source FROM finance_supplier_batches b WHERE b.id=? AND b.company_id=?',[batchId,actor.companyId]),
  rows(database,'SELECT to_jsonb(i)::text source FROM finance_supplier_instructions i WHERE i.batch_id=? ORDER BY i.id',[batchId]),
  one(database,'SELECT to_jsonb(i)::text source FROM finance_invoices i WHERE i.id=? AND i.company_id=?',[invoiceId,actor.companyId]),
  one(database,'SELECT to_jsonb(p)::text source FROM finance_payments p WHERE p.id=? AND p.company_id=?',[paymentId,actor.companyId]),
  one(database,'SELECT to_jsonb(b)::text source FROM finance_bank_transactions b WHERE b.id=? AND b.company_id=?',[bankId,actor.companyId]),
 ]);
 const journalIds=[...new Set([invoice?.journal_id,payment?.journal_id,command.kind==='confirm'?`journal-${command.key}`:`payment-reversal-${command.key}`].filter((id):id is string=>typeof id==='string'))],slots=journalIds.map(()=>'?').join(','),recordIds=[...new Set([batchId,invoiceId,paymentId,...instructions.map(row=>String(row.id))])],recordSlots=recordIds.map(()=>'?').join(',');
 const selectedJournals=`SELECT j.id FROM finance_journals j WHERE j.company_id=? AND (j.id IN (${slots}) OR EXISTS(SELECT 1 FROM finance_book_postings b WHERE b.secondary_journal_id=j.id AND b.primary_journal_id IN (${slots})))`,journalArgs=[actor.companyId,...journalIds,...journalIds];
 const [statement,journals,lines,events,books,allocations,reversals,money,sourceReceipts,receipt,purchaseOwned,native]=await Promise.all([
  one(database,'SELECT to_jsonb(s)::text source FROM finance_bank_statements s WHERE s.id=? AND s.company_id=?',[typeof bank?.statement_id==='string'?bank.statement_id:'',actor.companyId]),
  rows(database,`SELECT to_jsonb(j)::text source FROM finance_journals j WHERE j.id IN (${selectedJournals}) ORDER BY j.id`,journalArgs),
  rows(database,`SELECT to_jsonb(l)::text source FROM finance_journal_lines l WHERE l.journal_id IN (${selectedJournals}) ORDER BY l.journal_id,l.ordinal,l.id`,journalArgs),
  rows(database,`SELECT to_jsonb(e)::text source FROM finance_financial_events e WHERE e.journal_id IN (${selectedJournals}) ORDER BY e.journal_id`,journalArgs),
  rows(database,`SELECT to_jsonb(b)::text source FROM finance_book_postings b WHERE b.company_id=? AND b.primary_journal_id IN (${slots}) ORDER BY b.primary_journal_id,b.rule_id`,[actor.companyId,...journalIds]),
  rows(database,'SELECT to_jsonb(a)::text source FROM finance_allocations a WHERE a.company_id=? AND a.payment_id=? ORDER BY a.id',[actor.companyId,paymentId]),
  rows(database,'SELECT to_jsonb(r)::text source FROM finance_payment_reversals r WHERE r.payment_id=? ORDER BY r.id',[paymentId]),
  rows(database,`SELECT (to_jsonb(m)||jsonb_build_object('amount_units',m.amount_units::text,'usd_units',m.usd_units::text,'transaction_date',m.transaction_date::text))::text source FROM money_transaction_snapshots m WHERE m.company_id=? AND ((m.source_type='finance_supplier_instruction' AND m.source_id=?) OR (m.source_type='finance_payment' AND m.source_id=?) OR (m.source_type='finance_payment_reversal' AND m.source_id=?)) ORDER BY m.source_type,m.source_id,m.amount_kind`,[actor.companyId,typeof instruction?.id==='string'?instruction.id:command.kind==='confirm'?command.instructionId:'',paymentId,`payment-reversal-${command.key}`]),
  rows(database,`SELECT to_jsonb(r)::text source FROM command_receipts r WHERE r.command_id<>? AND r.record_id IN (${recordSlots}) AND r.authority_json::jsonb->>'companyId'=? ORDER BY r.command_id`,[command.key,...recordIds,actor.companyId]),
  one(database,'SELECT to_jsonb(r)::text source FROM command_receipts r WHERE r.command_id=?',[command.key]),
  database.prepare(`SELECT EXISTS(SELECT 1 FROM finance_supplier_instructions i WHERE i.batch_id=?) AND NOT EXISTS(SELECT 1 FROM finance_supplier_instructions i WHERE i.batch_id=? AND NOT EXISTS(SELECT 1 FROM (${commerceFinanceRecordSources.purchase.invoice}) p WHERE p.id=i.invoice_id AND p.company_id=?)) owned`).bind(batchId,batchId,actor.companyId).first<boolean>('owned'),
  database.prepare(`SELECT EXISTS(SELECT 1 FROM (${supplierPaymentOriginalSources}) s WHERE s.id=? AND s.company_id=?) native`).bind(paymentId,actor.companyId).first<boolean>('native'),
 ]);
 return {batch,instructions,invoice,payment,bank,statement,journals,lines,events,books,allocations,reversals,money:money as Money[],sourceReceipts,receipt,purchaseOwned:!!purchaseOwned,native:!!native};
}
function unitFor(database:SqlDatabase,actor:WorkContext,kind:Command['kind'],key:string){
 const unit=units.get(database);if(!unit||unit.actorImage!==JSON.stringify(actor)||unit.command.kind!==kind||unit.command.key!==key)throw new ApiError(503,'SUPPLIER_MONEY_TRANSACTION_REQUIRED','Use the original complete Supplier confirmation or reversal transaction.');return unit;
}
/** Source-specific evidence for a NEW normal Supplier instruction. It is not
 * a generic finance.payment.confirm receipt or a rewrite of old Supplier GL. */
export async function prepareSupplierConfirmationMoney(database:SqlDatabase,actor:WorkContext,facts:{key:string;instructionId:string;paymentId:string;journalId:string;amount:number;currency:string;transactionDate:string;at:number}):Promise<SqlStatement>{
 const unit=unitFor(database,actor,'confirm',facts.key),command=unit.command;if(command.kind!=='confirm'||command.instructionId!==facts.instructionId||facts.paymentId!==`supplier-payment-${facts.key}`||facts.journalId!==`journal-${facts.key}`)throw changed();
 const item=unit.before.instructions.find(row=>row.id===facts.instructionId);
 matches(item,{amount_minor:facts.amount});matches(unit.before.batch,{currency:facts.currency});matches(unit.before.bank,{transaction_date:facts.transactionDate,amount_minor:-facts.amount});
 await requireFinanceMoneyScale(database,actor.companyId,facts.currency);
 const prepared=await snapshotMoney(database,actor,{sourceType:'finance_supplier_instruction',sourceId:facts.instructionId,amountKind:'settled',transactionDate:facts.transactionDate,currency:facts.currency,currencyScale:2,amountUnits:String(facts.amount)});
 unit.plan={kind:'confirm',at:facts.at,paymentId:facts.paymentId,journalId:facts.journalId,money:{source_type:'finance_supplier_instruction',source_id:facts.instructionId,amount_kind:'settled',transaction_date:facts.transactionDate,currency:facts.currency,currency_scale:prepared.currencyScale,amount_units:prepared.amountUnits,usd_units:prepared.usdUnits,fx_rate_id:prepared.rateId,used_latest_fallback:prepared.usedLatestFallback,original_source_type:null,original_source_id:null,original_amount_kind:null,carried_source_type:null,carried_source_id:null,carried_amount_kind:null,carried_allocation_offset_units:null}};
 return prepared.statement;
}
/** Only an actual independently confirmed native Supplier source can enter.
 * Captured sources keep their exact quote; an uncaptured historical source
 * receives Money ONLY for the genuinely new reversal, at its own business date. */
export async function prepareSupplierReversalMoney(database:SqlDatabase,actor:WorkContext,facts:{key:string;paymentId:string;journalId:string;amount:number;currency:string;entryDate:string;at:number}):Promise<SqlStatement|null>{
 if(!units.has(database))return null;
 const unit=unitFor(database,actor,'reverse',facts.key),command=unit.command;if(command.kind!=='reverse'||command.paymentId!==facts.paymentId||facts.journalId!==`payment-reversal-${facts.key}`||facts.entryDate!==command.input.entryDate)throw changed();
 const before=unit.before,item=before.instructions.find(row=>row.payment_id===facts.paymentId);if(!item||!before.native)throw changed();matches(before.payment,{id:facts.paymentId,currency:facts.currency,amount_minor:facts.amount});
 const captured=before.money.find(row=>row.source_type==='finance_supplier_instruction'&&row.source_id===item.id&&row.amount_kind==='settled')??before.money.find(row=>row.source_type==='finance_payment'&&row.source_id===facts.paymentId&&row.amount_kind==='settled');
 const identity={sourceType:'finance_payment_reversal',sourceId:facts.journalId,amountKind:'gross',transactionDate:facts.entryDate};
 let statement:SqlStatement,money:Partial<Money>;
 if(captured){
  matches(captured,{currency:facts.currency,currency_scale:2,amount_units:String(facts.amount),transaction_date:before.payment!.payment_date,original_source_type:null});
  if(BigInt(captured.usd_units)<BigInt(0)||facts.currency==='USD'&&(captured.usd_units!==captured.amount_units||captured.fx_rate_id!==null)||facts.currency!=='USD'&&typeof captured.fx_rate_id!=='string')throw changed();
  const original={sourceType:captured.source_type,sourceId:captured.source_id,amountKind:captured.amount_kind};
  const prepared=await snapshotMoneyReversal(database,actor,{...identity,amountUnits:String(facts.amount),original});statement=prepared.statement;
  money={currency:captured.currency,currency_scale:captured.currency_scale,amount_units:String(-BigInt(captured.amount_units)),usd_units:String(-BigInt(captured.usd_units)),fx_rate_id:captured.fx_rate_id,used_latest_fallback:captured.used_latest_fallback,original_source_type:original.sourceType,original_source_id:original.sourceId,original_amount_kind:original.amountKind};
 }else{
  await requireFinanceMoneyScale(database,actor.companyId,facts.currency);
  const prepared=await snapshotMoney(database,actor,{...identity,currency:facts.currency,currencyScale:2,amountUnits:String(-facts.amount)});statement=prepared.statement;
  money={currency:facts.currency,currency_scale:prepared.currencyScale,amount_units:prepared.amountUnits,usd_units:prepared.usdUnits,fx_rate_id:prepared.rateId,used_latest_fallback:prepared.usedLatestFallback,original_source_type:null,original_source_id:null,original_amount_kind:null};
 }
 unit.plan={kind:'reverse',at:facts.at,paymentId:facts.paymentId,journalId:facts.journalId,money:{...money,carried_source_type:null,carried_source_id:null,carried_amount_kind:null,carried_allocation_offset_units:null,source_type:identity.sourceType,source_id:identity.sourceId,amount_kind:identity.amountKind,transaction_date:identity.transactionDate}};return statement;
}
function validateExisting(unit:Unit){
 const {command,before}=unit,actor=JSON.parse(unit.actorImage) as WorkContext;
 if(!before.receipt||!before.native||!before.payment)throw changed();
 requireReceiptCompany(before.receipt,actor.companyId);
 matches(before.receipt,{operation:command.kind==='confirm'?'finance.supplier_batch.confirm':'finance.payment.reverse',actor_id:actor.userId,record_id:command.kind==='confirm'?command.instructionId:command.paymentId});
 const result=JSON.parse(String(before.receipt.result_json)) as Row;
 if(command.kind==='confirm'){
  equal(result,{id:command.instructionId,status:'confirmed',version:command.input.expectedVersion+1,paymentId:`supplier-payment-${command.key}`,journalId:`journal-${command.key}`});
  matches(before.payment,{id:result.paymentId,journal_id:result.journalId});return;
 }
 equal(result,{id:command.paymentId,status:'voided',version:command.input.expectedVersion+1,reversalJournalId:`payment-reversal-${command.key}`});
 matches(before.payment,{status:'voided'});
 const id=String(result.reversalJournalId),reversal=before.reversals.find(row=>row.id===`payment-correction-${command.key}`),original=before.journals.find(row=>row.id===before.payment!.journal_id),journal=before.journals.find(row=>row.id===id);
 matches(reversal,{payment_id:command.paymentId,reversal_journal_id:id,fx_reversal_journal_id:null,reason:command.input.reason,reversed_by:actor.userId});matches(original,{status:'reversed',reversed_by:actor.userId,reversed_at:reversal!.reversed_at});
 matches(journal,{company_id:actor.companyId,chart_id:original!.chart_id,currency:original!.currency,period_id:command.input.periodId,entry_date:command.input.entryDate,source_type:'payment_reversal',source_id:command.paymentId,status:'posted',posted_by:actor.userId});
 const sourceLines=before.lines.filter(row=>row.journal_id===original!.id),lines=before.lines.filter(row=>row.journal_id===id);if(sourceLines.length<2||lines.length!==sourceLines.length)throw changed();
 for(const row of sourceLines)equal({...row,id:`${id}-${row.ordinal}`,journal_id:id,debit_minor:row.credit_minor,credit_minor:row.debit_minor,base_debit_minor:row.base_credit_minor,base_credit_minor:row.base_debit_minor},lines.find(line=>line.ordinal===row.ordinal));
 for(const selected of [original!,journal!]){const event=before.events.find(row=>row.journal_id===selected.id);if(!event)throw changed();equal(JSON.parse(String(event.evidence_json)).lines,before.lines.filter(row=>row.journal_id===selected.id));}
}
function validate(unit:Unit,after:Image,flushed:boolean){
 const before=unit.before,plan=unit.plan,command=unit.command;
 if(!plan){validateExisting(unit);equal(before,after);return;}
 const originalItem=before.instructions.find(row=>row.id===(command.kind==='confirm'?command.instructionId:before.instructions.find(row=>row.payment_id===command.paymentId)?.id));
 if(!originalItem||!before.batch||!before.invoice||!before.bank||!after.native)throw changed();
 const invoice=before.invoice,amount=Number(originalItem.amount_minor),at=plan.at;
 equal(before.statement,after.statement);equal(before.sourceReceipts,after.sourceReceipts);equal(before.purchaseOwned,after.purchaseOwned);
 equal(before.instructions.map(row=>command.kind==='confirm'&&row.id===command.instructionId?{...row,status:'confirmed',bank_reference:command.input.bankReference,evidence:command.input.evidence,payment_id:plan.paymentId,bank_transaction_id:before.bank!.id,resolved_by:JSON.parse(unit.actorImage).userId,version:Number(row.version)+1,updated_at:at}:row),after.instructions);
 equal(command.kind==='confirm'?{...before.batch,version:Number(before.batch.version)+1,updated_at:at}:before.batch,after.batch);
 const invoiceAmount=command.kind==='confirm'?-amount:amount,remaining=Number(invoice.outstanding_minor)+invoiceAmount;
 equal({...invoice,outstanding_minor:remaining,base_outstanding_minor:Number(invoice.base_outstanding_minor)+invoiceAmount,status:command.kind==='confirm'?(remaining===0?'paid':'partially_paid'):(remaining===Number(invoice.total_minor)?'posted':'partially_paid'),version:Number(invoice.version)+1,updated_at:at},after.invoice);
 equal(command.kind==='confirm'?{...before.bank,match_status:'matched',matched_type:'payment',matched_id:plan.paymentId,version:Number(before.bank.version)+1}:{...before.bank,match_status:'unmatched',matched_type:null,matched_id:null,version:Number(before.bank.version)+1},after.bank);
 if(command.kind==='confirm'){
  if(before.payment||before.allocations.length||before.reversals.length)throw changed();
  matches(after.payment,{id:plan.paymentId,company_id:invoice.company_id,party_id:invoice.party_id,kind:'payment',payment_number:`SB-${command.key}`,payment_date:before.bank.transaction_date,currency:before.batch.currency,amount_minor:amount,outstanding_minor:0,bank_account:before.batch.bank_account_id,status:'settled',journal_id:plan.journalId,fx_rate_micros:1000000,base_amount_minor:amount,created_by:before.batch.created_by,approved_by:before.batch.approved_by,released_by:JSON.parse(unit.actorImage).userId,created_at:at,updated_at:at});
  equal(after.allocations,[{id:`allocation-${command.key}`,company_id:invoice.company_id,payment_id:plan.paymentId,invoice_id:invoice.id,amount_minor:amount,base_invoice_minor:amount,base_payment_minor:amount,created_by:JSON.parse(unit.actorImage).userId,created_at:at,realized_fx_minor:0}]);equal(before.reversals,after.reversals);
 }else{
  if(!before.payment)throw changed();equal({...before.payment,status:'voided',version:Number(before.payment.version)+1,updated_at:at},after.payment);equal(before.allocations,after.allocations);
  equal(after.reversals,[{id:`payment-correction-${command.key}`,payment_id:plan.paymentId,reversal_journal_id:plan.journalId,fx_reversal_journal_id:null,reason:command.input.reason,reversed_by:JSON.parse(unit.actorImage).userId,reversed_at:at}]);
 }
 for(const row of before.journals){const primary=command.kind==='reverse'&&row.id===before.payment?.journal_id,secondary=command.kind==='reverse'&&before.books.some(book=>book.primary_journal_id===before.payment?.journal_id&&book.secondary_journal_id===row.id),expected=primary||secondary?{...row,status:'reversed',version:Number(row.version)+1,reversed_by:JSON.parse(unit.actorImage).userId,reversed_at:at,updated_at:at}:row,actual=after.journals.find(value=>value.id===row.id);if(!flushed&&secondary&&isDeepStrictEqual(row,actual))continue;equal(expected,actual);}
 for(const row of before.lines)equal(row,after.lines.find(value=>value.id===row.id));for(const row of before.events)equal(row,after.events.find(value=>value.journal_id===row.journal_id));for(const row of before.books)equal(row,after.books.find(value=>value.primary_journal_id===row.primary_journal_id&&value.rule_id===row.rule_id));
 const journal=after.journals.find(row=>row.id===plan.journalId),source=before.journals.find(row=>row.id===invoice.journal_id),actor=JSON.parse(unit.actorImage) as WorkContext;
 matches(journal,{company_id:actor.companyId,chart_id:source?.chart_id,period_id:command.input.periodId,entry_date:command.kind==='confirm'?before.bank.transaction_date:command.input.entryDate,currency:before.batch.currency,fx_rate_micros:1000000,memo:command.kind==='confirm'?`Supplier payment ${invoice.invoice_number}: ${command.input.bankReference}`:command.input.reason,source_type:command.kind==='confirm'?'supplier_batch':'payment_reversal',source_id:command.kind==='confirm'?command.instructionId:command.paymentId,status:'posted',created_by:actor.userId,posted_by:actor.userId,version:1});
 const lines=after.lines.filter(row=>row.journal_id===plan.journalId);
 if(command.kind==='confirm'){
  if(lines.length!==2)throw changed();
  for(const [ordinal,accountId,debit,credit] of [[0,invoice.credit_account_id,amount,0],[1,before.batch.bank_account_id,0,amount]] as const)matches(lines[ordinal],{id:`${plan.journalId}-${ordinal}`,journal_id:plan.journalId,ordinal,account_id:accountId,debit_minor:debit,credit_minor:credit,base_debit_minor:debit,base_credit_minor:credit,description:'',dimension_json:'{}'});
 }else{
  const original=before.lines.filter(row=>row.journal_id===before.payment!.journal_id);if(lines.length!==original.length)throw changed();for(const row of original)equal({...row,id:`${plan.journalId}-${row.ordinal}`,journal_id:plan.journalId,debit_minor:row.credit_minor,credit_minor:row.debit_minor,base_debit_minor:row.base_credit_minor,base_credit_minor:row.base_debit_minor},lines.find(value=>value.ordinal===row.ordinal));
 }
 const money=after.money.filter(row=>row.source_type===plan.money.source_type&&row.source_id===plan.money.source_id&&row.amount_kind===plan.money.amount_kind);if(money.length!==1)throw changed();matches(money[0],plan.money);equal(before.money,after.money.filter(row=>row!==money[0]));
 requireReceiptCompany(after.receipt,actor.companyId);
 matches(after.receipt,{operation:command.kind==='confirm'?'finance.supplier_batch.confirm':'finance.payment.reverse',actor_id:actor.userId,record_id:command.kind==='confirm'?command.instructionId:command.paymentId});
 equal(JSON.parse(String(after.receipt!.result_json)),command.kind==='confirm'?{id:command.instructionId,status:'confirmed',version:command.input.expectedVersion+1,paymentId:plan.paymentId,journalId:plan.journalId}:{id:command.paymentId,status:'voided',version:command.input.expectedVersion+1,reversalJournalId:plan.journalId});
 if(command.kind==='reverse')matches(journal,{journal_number:`PAY-REV-${command.key}`,posted_at:at,created_at:at,updated_at:at});
 if(flushed){const event=after.events.find(row=>row.journal_id===plan.journalId);matches(event,{company_id:actor.companyId,journal_id:plan.journalId,source_type:journal!.source_type,source_id:journal!.source_id,legacy_snapshot:0});equal(JSON.parse(String(event!.evidence_json)).lines,lines);}
}
async function finalPurchase(database:SqlDatabase,actor:WorkContext,before:Image,current:Image){
 const mode=await financePackageEnforcementMode(),purpose=before.receipt?'history' as const:'settle-existing' as const,access=await readPurchasedAccesses(database,mode,actor.companyId,['finance.banking','finance.inventory-valuation']);
 const capability=access['finance.banking'].codes[purpose]==='GRANTED'?'finance.banking':current.purchaseOwned?'finance.inventory-valuation':'finance.banking';
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability,purpose});
}
async function owned<Result>(actor:WorkContext,command:Command,run:(database:SqlDatabase)=>Promise<Result>,optional:boolean):Promise<{handled:true;result:Result}|{handled:false}>{
 const owner=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'SUPPLIER_MONEY_TRANSACTION_REQUIRED','Supplier Money requires its original Holding transaction.');
 try{return await owner.transaction(async database=>{
  await lockPackageSettlementSources(database);
  await database.prepare('LOCK TABLE active_work_contexts IN EXCLUSIVE MODE').run();
  // Receipt-first legacy reference writers must finish before any Finance
  // reference/source lease. Selected Work is leased before the receipt fence.
  await database.prepare('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE').run();
  await database.batch(financeSourceDocumentReadLeases(database,actor.companyId,true,'supplier-correction'));
  await database.prepare('LOCK TABLE finance_supplier_batches,finance_supplier_instructions IN SHARE ROW EXCLUSIVE MODE').run();
  await database.prepare('LOCK TABLE company_money_settings,company_currency_scales,company_fx_rates,finance_periods IN SHARE MODE').run();
  await lockQueuedCommandAuthority(database);
  const before=await image(database,actor,command);
  if(optional&&!before.instructions.some(row=>row.payment_id===(command.kind==='reverse'?command.paymentId:'')))return {handled:false as const};
  if(optional&&!before.native)throw changed();
  const current=await authority(database,actor,command.kind),refs=await references(database,actor.companyId),unit:Unit={actorImage:JSON.stringify(actor),command,before};units.set(database,unit);
  try{
   const result=await run(database),after=await image(database,actor,command);validate(unit,after,false);
   // Validate the original request hash/actor/receipt BEFORE its assertion
   // cleanup can produce deferred work. Final guards below are read-only.
   const replay=await replayCommand(database,actor,command.key,command.kind==='confirm'?'finance.supplier_batch.confirm':'finance.payment.reverse',command.kind==='confirm'?command.instructionId:command.paymentId,command.input);equal(result,replay);const receipt=await one(database,'SELECT to_jsonb(r)::text source FROM command_receipts r WHERE r.command_id=?',[command.key]);
   await flushFinanceDeferredEffects(database);
   const final=await image(database,actor,command);validate(unit,final,true);equal(receipt,final.receipt);equal(refs,await references(database,actor.companyId));equal(current,await authority(database,actor,command.kind));
   await finalPurchase(database,actor,before,final);equal(unit.actorImage,JSON.stringify(actor));equal(final,await image(database,actor,command));
   return {handled:true as const,result};
  }finally{units.delete(database);}
 });}catch(error){throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
/** Rare complete Supplier units serialize Commercial writers across Companies.
 * This is a source-finality fence, not a demonstrated performance guarantee. */
export async function withSupplierConfirmationMoney<Result>(actor:WorkContext,instructionId:string,input:ConfirmInput,key:string,run:(database:SqlDatabase)=>Promise<Result>){const result=await owned(actor,{kind:'confirm',instructionId,input,key},run,false);if(!result.handled)throw changed();return result.result;}
export async function withSupplierReversalMoney<Result>(actor:WorkContext,paymentId:string,input:ReverseInput,key:string,run:(database:SqlDatabase)=>Promise<Result>,fallback:()=>Promise<Result>){const result=await owned(actor,{kind:'reverse',paymentId,input,key},run,true);return result.handled?result.result:fallback();}
