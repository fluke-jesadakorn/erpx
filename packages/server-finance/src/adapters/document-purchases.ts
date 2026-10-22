import {serviceInitialStockJournalCapabilities,serviceInitialStockJournalMarked,serviceInitialStockJournalMarkedPredicate,serviceInitialStockJournalPredicate,serviceInitialStockJournalReadPredicate} from '@erpx/server-finance/adapters/service-initial-stock-journal-sources';
import {posInitialStockJournalCapabilities,posInitialStockJournalMarked,posInitialStockJournalMarkedPredicate,posInitialStockJournalPredicate,posInitialStockJournalReadPredicate} from '@erpx/server-finance/adapters/pos-initial-stock-journal-sources';
import {manufacturingInitialStockJournalCapabilities,manufacturingInitialStockJournalMarked,manufacturingInitialStockJournalMarkedPredicate,manufacturingInitialStockJournalPredicate,manufacturingInitialStockJournalReadPredicate} from '@erpx/server-finance/adapters/manufacturing-initial-stock-journal-sources';
import {manufacturingFinanceRecordSources,manufacturingFinanceSourceReadLocks} from "@erpx/server-manufacturing/public/finance-record-sources";
import {fieldServiceFinanceRecordSources,fieldServiceFinanceSourceReadLocks} from "@erpx/server-field_service/public/finance-record-sources";
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {financePackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability,companyPurchaseReadLocks} from "@erpx/server-commercial/public/purchase-command-boundary";
import {purchasedCapabilityPredicate,packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";
import {posFinanceRecordSources} from "@erpx/server-pos/public/finance-record-sources";
import {pmsFinanceRecordSources} from "@erpx/server-pms/public/finance-record-sources";
import {payrollFinanceRecordSources} from "@erpx/server-people_payroll/public/finance-record-sources";
import {commerceFinanceRecordSources,commerceSalesFinanceOrdinaryRecordSources,commercePurchaseFinanceOrdinaryRecordSources,commerceFinanceSourceReadLocks} from "@erpx/server-commerce/public/finance-record-sources";
import {inventoryFinanceRecordSources,inventoryFinanceSourceReadLocks} from "@erpx/server-inventory/public/finance-record-sources";
import {maintenanceFinanceRecordSources,maintenanceFinanceSourceReadLocks} from "@erpx/server-maintenance/public/finance-record-sources";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {isTransactionalSqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {requireFinanceCurrentAuthority,requireFinancePurchasedRead,withFinancePurchasedCommand} from "@erpx/server-finance/adapters/purchase-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {financeSourceAccess} from "@erpx/server-finance/adapters/source-purchases";
import {standaloneSalesCompletionReceiptQuery} from "@erpx/server-finance/adapters/sales-completion-proof";

export type FinanceDocumentKind="invoice"|"journal"|"payment"|"refund"|"supplier-batch";
const tables={invoice:"finance_invoices",journal:"finance_journals",payment:"finance_payments",refund:"finance_refund_obligations","supplier-batch":"finance_supplier_batches"} as const;
const foundations=["finance.pos-posting","finance.hotel-posting","finance.asset-recognition","finance.asset-disposal","people.payroll","finance.sales-posting","finance.inventory-valuation","assets.parts-costs","finance.production-costing","finance.service-invoice"] as const;
const base=(kind:FinanceDocumentKind)=>kind==="payment"||kind==="refund"||kind==="supplier-batch"?"finance.banking":"finance.accounting";
function roots(kind:FinanceDocumentKind,alias="d"){
  // A projected book journal retains the identity of its actual primary. A
  // free-form source_type/source_id cannot manufacture this book relationship.
  return kind==="journal"?`SELECT ${alias}.id,${alias}.company_id UNION SELECT p.id,p.company_id FROM finance_book_postings b JOIN finance_journals p ON p.id=b.primary_journal_id AND p.company_id=${alias}.company_id WHERE b.secondary_journal_id=${alias}.id`:`SELECT ${alias}.id,${alias}.company_id`;
}
type CommerceProof="ordinary"|"retained";
const commerceProofs=(kind:FinanceDocumentKind,capability:string):readonly (CommerceProof|undefined)[]=>(capability==="finance.sales-posting"||capability==="finance.inventory-valuation")&&(kind==="journal"||kind==="payment")?["ordinary","retained"]:[undefined];
function commerceOwnerPredicate(kind:FinanceDocumentKind,capability:string,commerceProof?:CommerceProof){
  const commerce=capability==="finance.sales-posting"?(commerceProof==="ordinary"?commerceSalesFinanceOrdinaryRecordSources:commerceFinanceRecordSources.sales):capability==="finance.inventory-valuation"?(commerceProof==="ordinary"?commercePurchaseFinanceOrdinaryRecordSources:commerceFinanceRecordSources.purchase):null;
  if(!commerce)return "FALSE";
  const linked=`EXISTS(SELECT 1 FROM (${roots(kind)}) root JOIN (${commerce[kind]}) owner_source ON owner_source.id=root.id AND owner_source.company_id=root.company_id)`;
  if(capability!=="finance.sales-posting"||kind!=="invoice")return linked;
  // Standalone service invoices are born from Sales' exact completion command,
  // even though they have no Commerce fulfillment FK. Tie the included Finance
  // history/posting right to that committed command receipt and its normal
  // audit/outbox evidence; source labels and a caller-supplied invoice ID alone
  // never establish Sales ownership.
  const standalone=`(d.kind='receivable' AND d.source_type='standalone' AND d.source_id IS NULL AND EXISTS(${standaloneSalesCompletionReceiptQuery("d")}))`;
  return `(${linked} OR ${standalone})`;
}
function ownerPredicate(kind:FinanceDocumentKind,capability:string,commerceProof?:CommerceProof){
  if(capability==="finance.service-invoice"){const original=kind==="invoice"||kind==="journal"||kind==="payment"?`EXISTS(SELECT 1 FROM (${roots(kind)}) root JOIN (${fieldServiceFinanceRecordSources[kind]}) owner_source ON owner_source.id=root.id AND owner_source.company_id=root.company_id)`:"FALSE";return kind==="journal"?`(${original} OR ${serviceInitialStockJournalPredicate("d")})`:original;}
  if(capability==="finance.sales-posting"||capability==="finance.inventory-valuation"){
    const commerce=commerceOwnerPredicate(kind,capability,commerceProof);
    const inventory=capability==="finance.inventory-valuation"?inventoryFinanceRecordSources.journal:inventoryFinanceRecordSources.salesCountJournal;
    return kind==="journal"?`(${commerce} OR EXISTS(SELECT 1 FROM (${roots(kind)}) root JOIN (${inventory}) owner_source ON owner_source.id=root.id AND owner_source.company_id=root.company_id))`:commerce;
  }
  if(capability==="assets.parts-costs")return kind==="journal"?`EXISTS(SELECT 1 FROM (${roots(kind)}) root JOIN (${maintenanceFinanceRecordSources.journal}) owner_source ON owner_source.id=root.id AND owner_source.company_id=root.company_id)`:"FALSE";
  if(capability==="finance.production-costing")return kind==="journal"?`(EXISTS(SELECT 1 FROM (${roots(kind)}) root JOIN (${manufacturingFinanceRecordSources.journal}) owner_source ON owner_source.id=root.id AND owner_source.company_id=root.company_id) OR ${manufacturingInitialStockJournalPredicate("d")})`:"FALSE";
  if(kind==="refund"||kind==="supplier-batch")return "FALSE";
  const source=capability==="finance.pos-posting"?posFinanceRecordSources[kind]:capability==="finance.hotel-posting"?pmsFinanceRecordSources[kind]:null;
  if(source){
    const direct=`EXISTS(SELECT 1 FROM (${roots(kind)}) root JOIN (${source}) owner_source ON owner_source.id=root.id AND owner_source.company_id=root.company_id)`;
    if(kind!=="payment")return kind==='journal'&&capability==='finance.pos-posting'?`(${direct} OR ${posInitialStockJournalPredicate('d')})`:direct;
    const journals=capability==="finance.pos-posting"?posFinanceRecordSources.journal:pmsFinanceRecordSources.journal;
    return `(${direct} OR EXISTS(SELECT 1 FROM (${journals}) owner_journal WHERE owner_journal.company_id=d.company_id AND owner_journal.id=d.journal_id))`;
  }
  if(kind==="journal"&&capability==="finance.asset-recognition")return `EXISTS(SELECT 1 FROM (${roots(kind)}) root JOIN finance_assets a ON a.activation_journal_id=root.id AND a.company_id=root.company_id)`;
  if(kind==="journal"&&capability==="finance.asset-disposal")return `EXISTS(SELECT 1 FROM (${roots(kind)}) root JOIN finance_asset_disposals a ON a.journal_id=root.id AND a.company_id=root.company_id AND a.status='posted')`;
  if(kind==="journal"&&capability==="people.payroll")return `EXISTS(SELECT 1 FROM (${roots(kind)}) root JOIN (${payrollFinanceRecordSources.journal}) owner_source ON owner_source.id=root.id AND owner_source.company_id=root.company_id)`;
  return "FALSE";
}
function servicePartsPredicate(kind:FinanceDocumentKind){return kind==="journal"?`EXISTS(SELECT 1 FROM (${roots(kind)}) root JOIN (${fieldServiceFinanceRecordSources.partsJournal}) owner_source ON owner_source.id=root.id AND owner_source.company_id=root.company_id)`:"FALSE";}
/** Known private initial sources never fall back to an unrelated purchase or
 * legacy disabled mode. The fixed owner proof is checked separately below. */
export async function initialPhysicalStockJournalMarked(database:SqlDatabase,companyId:string,id:string){return !!await database.prepare(`SELECT 1 FROM finance_journals d WHERE d.id=? AND d.company_id=? AND (EXISTS(SELECT 1 FROM (${roots('journal')}) root JOIN (${inventoryFinanceRecordSources.initialPhysicalCountMarkedJournal}) source ON source.id=root.id AND source.company_id=root.company_id) OR ${manufacturingInitialStockJournalMarkedPredicate('d')} OR ${posInitialStockJournalMarkedPredicate('d')} OR ${serviceInitialStockJournalMarkedPredicate('d')})`).bind(id,companyId).first();}
const initialStockJournalPredicate=`EXISTS(SELECT 1 FROM (${roots('journal')}) root JOIN (${inventoryFinanceRecordSources.salesInitialPhysicalCountJournal}) source ON source.id=root.id AND source.company_id=root.company_id)`;
/** Row-specific filter for this source; ordinary Finance visibility is left to
 * its original collection, notification and record policies. */
export function initialPhysicalStockFinanceJournalFilter(alias:string,clock:string,current=true){
 const rootsSql=roots('journal','initial_finance_document');
 const marked=`EXISTS(SELECT 1 FROM finance_journals initial_finance_document WHERE initial_finance_document.id=${alias}.id AND initial_finance_document.company_id=${alias}.company_id AND (EXISTS(SELECT 1 FROM (${rootsSql}) root JOIN (${inventoryFinanceRecordSources.initialPhysicalCountMarkedJournal}) marker ON marker.id=root.id AND marker.company_id=root.company_id) OR ${manufacturingInitialStockJournalMarkedPredicate('initial_finance_document')} OR ${posInitialStockJournalMarkedPredicate('initial_finance_document')} OR ${serviceInitialStockJournalMarkedPredicate('initial_finance_document')}))`;
 const valid=`EXISTS(SELECT 1 FROM finance_journals initial_finance_document WHERE initial_finance_document.id=${alias}.id AND initial_finance_document.company_id=${alias}.company_id AND EXISTS(SELECT 1 FROM (${rootsSql}) root JOIN (${inventoryFinanceRecordSources.salesInitialPhysicalCountJournal}) source ON source.id=root.id AND source.company_id=root.company_id) AND ${['sales.orders','inventory.stock-core','finance.sales-posting'].map(cap=>purchasedCapabilityPredicate('initial_finance_document.company_id',cap,'history',clock)).join(' AND ')} AND ${['sales','inventory','finance'].map(module=>scheduledFeaturePredicate('initial_finance_document.company_id',module)).join(' AND ')})`;
 return `(NOT (${marked}) OR (${current?'TRUE':'FALSE'} AND (CASE WHEN ${serviceInitialStockJournalMarkedPredicate(alias)} THEN ${serviceInitialStockJournalReadPredicate(alias,clock)} WHEN ${posInitialStockJournalMarkedPredicate(alias)} THEN ${posInitialStockJournalReadPredicate(alias,clock)} WHEN ${manufacturingInitialStockJournalMarkedPredicate(alias)} THEN ${manufacturingInitialStockJournalReadPredicate(alias,clock)} ELSE ${valid} END)))`;
}

async function additionalCapabilities(database:SqlDatabase,actor:Pick<WorkContext,"companyId">,kind:FinanceDocumentKind,id:string,capability:string){if(kind==='journal'&&await initialPhysicalStockJournalMarked(database,actor.companyId,id))return await serviceInitialStockJournalMarked(database,actor.companyId,id)?serviceInitialStockJournalCapabilities.filter(cap=>cap!=='finance.service-invoice'):await posInitialStockJournalMarked(database,actor.companyId,id)?posInitialStockJournalCapabilities.filter(cap=>cap!=='finance.pos-posting'):await manufacturingInitialStockJournalMarked(database,actor.companyId,id)?manufacturingInitialStockJournalCapabilities.filter(cap=>'finance.production-costing'!==cap):['sales.orders','inventory.stock-core'];return capability==="finance.service-invoice"&&kind==="journal"&&await database.prepare(`SELECT 1 FROM finance_journals d WHERE d.id=? AND d.company_id=? AND ${servicePartsPredicate(kind)}`).bind(id,actor.companyId).first()?["service.parts-assets"]:[];}
/** Keep Finance authority and original record rules. Included purchase rights
 * are selected only from actual immutable owner links, never a source label. */
export async function requireFinanceDocumentHistory(database:SqlDatabase,actor:WorkContext,kind:FinanceDocumentKind,id:string,purpose:CapabilityPurpose="history",strictPurchase=false){
  const initial=kind==='journal'&&await initialPhysicalStockJournalMarked(database,actor.companyId,id);
  const mode=strictPurchase||initial?"enforced" as const:await financePackageEnforcementMode();if(mode==="disabled")return {capability:base(kind),canSettle:true,additionalCapabilities:[] as string[],commerceProof:undefined as CommerceProof|undefined};
  await requireFinanceCurrentAuthority(database,actor,`finance-document:${crypto.randomUUID()}`);
  if(initial&&await serviceInitialStockJournalMarked(database,actor.companyId,id)){for(const capability of serviceInitialStockJournalCapabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});if(!await database.prepare(`SELECT 1 FROM finance_journals d WHERE d.id=? AND d.company_id=? AND ${serviceInitialStockJournalPredicate('d')} AND ${['field_service','inventory','finance'].map(module=>scheduledFeaturePredicate('d.company_id',module)).join(' AND ')}`).bind(id,actor.companyId).first())throw new ApiError(404,'JOURNAL_NOT_FOUND','The original Service physical Count journal is unavailable.');return {capability:'finance.service-invoice',canSettle:false,additionalCapabilities:serviceInitialStockJournalCapabilities.filter(cap=>cap!=='finance.service-invoice'),commerceProof:undefined as CommerceProof|undefined};}
  if(initial&&await posInitialStockJournalMarked(database,actor.companyId,id)){for(const capability of posInitialStockJournalCapabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});if(!await database.prepare(`SELECT 1 FROM finance_journals d WHERE d.id=? AND d.company_id=? AND ${posInitialStockJournalPredicate('d')} AND ${['pos','inventory','finance'].map(module=>scheduledFeaturePredicate('d.company_id',module)).join(' AND ')}`).bind(id,actor.companyId).first())throw new ApiError(404,'JOURNAL_NOT_FOUND','The original POS physical Count journal is unavailable.');return {capability:'finance.pos-posting',canSettle:false,additionalCapabilities:posInitialStockJournalCapabilities.filter(cap=>cap!=='finance.pos-posting'),commerceProof:undefined as CommerceProof|undefined};}
  if(initial&&await manufacturingInitialStockJournalMarked(database,actor.companyId,id)){
   for(const capability of manufacturingInitialStockJournalCapabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
   if(!await database.prepare(`SELECT 1 FROM finance_journals d WHERE d.id=? AND d.company_id=? AND ${manufacturingInitialStockJournalPredicate('d')} AND ${['manufacturing','inventory','finance'].map(module=>scheduledFeaturePredicate('d.company_id',module)).join(' AND ')}`).bind(id,actor.companyId).first())throw new ApiError(404,'JOURNAL_NOT_FOUND','The original Manufacturing physical Count journal is unavailable.');
   return {capability:'finance.production-costing',canSettle:false,additionalCapabilities:manufacturingInitialStockJournalCapabilities.filter(cap=>cap!=='finance.production-costing'),commerceProof:undefined as CommerceProof|undefined};
  }
  if(initial){
   for(const capability of ['sales.orders','inventory.stock-core','finance.sales-posting'])await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose});
   if(!await database.prepare(`SELECT 1 FROM finance_journals d WHERE d.id=? AND d.company_id=? AND ${initialStockJournalPredicate} AND ${['sales','inventory','finance'].map(module=>scheduledFeaturePredicate('d.company_id',module)).join(' AND ')}`).bind(id,actor.companyId).first())throw new ApiError(404,'JOURNAL_NOT_FOUND','The original Initial Physical Stock Count journal is unavailable.');
   return {capability:'finance.sales-posting',canSettle:false,additionalCapabilities:['sales.orders','inventory.stock-core'],commerceProof:undefined as CommerceProof|undefined};
  }
  const access=await readPurchasedAccesses(database,mode,actor.companyId,[base(kind),...foundations]);
  let capability=base(kind),commerceProof:CommerceProof|undefined;
  if(access[capability].codes[purpose]==="GRANTED")return {capability,canSettle:access[base(kind)].settleExisting,additionalCapabilities:[] as string[]};
  if(access[capability].codes[purpose]!=="GRANTED"){
    foundationsLoop:for(const foundation of foundations){
      if(access[foundation].codes[purpose]!=="GRANTED")continue;
      for(const proof of commerceProofs(kind,foundation))if(await database.prepare(`SELECT 1 FROM ${tables[kind]} d WHERE d.id=? AND d.company_id=? AND ${ownerPredicate(kind,foundation,proof)}`).bind(id,actor.companyId).first()){capability=foundation;commerceProof=proof;break foundationsLoop;}
    }
  }
  await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability,purpose});
  const required=await additionalCapabilities(database,actor,kind,id,capability);for(const extra of required)await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:extra,purpose});
  return {capability,canSettle:access[base(kind)].settleExisting,additionalCapabilities:required,commerceProof};
}
/** The original file transaction checks the exact owner relationship again
 * after deferred effects; retaining an included capability alone is not enough. */
/** A captured ordinary Commerce proof keeps its actual Commerce/Count owner
 * and rejects every private initial marker before and after the unit. Initial
 * proofs never select this branch and retain their complete original guards. */
function financeDocumentOwnerPredicate(kind:FinanceDocumentKind,capability:string,commerceProof?:CommerceProof){
  if(kind==='journal'&&(capability==='finance.sales-posting'||capability==='finance.inventory-valuation')&&commerceProof==='ordinary')return `(${commerceOwnerPredicate(kind,capability,commerceProof)} OR EXISTS(SELECT 1 FROM (${roots(kind)}) root JOIN (${capability==='finance.sales-posting'?inventoryFinanceRecordSources.salesQualityCountJournal:inventoryFinanceRecordSources.journal}) owner_source ON owner_source.id=root.id AND owner_source.company_id=root.company_id)) AND NOT (EXISTS(SELECT 1 FROM (${roots('journal')}) root JOIN (${inventoryFinanceRecordSources.initialPhysicalCountMarkedJournal}) marker ON marker.id=root.id AND marker.company_id=root.company_id) OR ${manufacturingInitialStockJournalMarkedPredicate('d')} OR ${posInitialStockJournalMarkedPredicate('d')} OR ${serviceInitialStockJournalMarkedPredicate('d')})`;
  return `${ownerPredicate(kind,capability,commerceProof)}${kind==='journal'?` AND (NOT (EXISTS(SELECT 1 FROM (${roots('journal')}) root JOIN (${inventoryFinanceRecordSources.initialPhysicalCountMarkedJournal}) marker ON marker.id=root.id AND marker.company_id=root.company_id) OR ${manufacturingInitialStockJournalMarkedPredicate('d')} OR ${posInitialStockJournalMarkedPredicate('d')} OR ${serviceInitialStockJournalMarkedPredicate('d')}) OR (CASE WHEN ${serviceInitialStockJournalMarkedPredicate('d')} THEN (SELECT ${serviceInitialStockJournalReadPredicate('d','service_initial_clock.now_ms')} FROM (SELECT ${packageClockSql} now_ms) service_initial_clock) WHEN ${posInitialStockJournalMarkedPredicate('d')} THEN (SELECT ${posInitialStockJournalReadPredicate('d','pos_initial_clock.now_ms')} FROM (SELECT ${packageClockSql} now_ms) pos_initial_clock) WHEN ${manufacturingInitialStockJournalMarkedPredicate('d')} THEN (SELECT ${manufacturingInitialStockJournalReadPredicate('d','manufacturing_initial_clock.now_ms')} FROM (SELECT ${packageClockSql} now_ms) manufacturing_initial_clock) ELSE (${initialStockJournalPredicate} AND ${['sales','inventory','finance'].map(module=>scheduledFeaturePredicate('d.company_id',module)).join(' AND ')}) END))`:''}`;
}
export function financeDocumentSourceGuards(database:SqlDatabase,actor:WorkContext,kind:FinanceDocumentKind,id:string,capability:string,key:string,commerceProof?:CommerceProof){
  if(capability===base(kind))return [];
  return [...(capability==="finance.service-invoice"?fieldServiceFinanceSourceReadLocks(database):[]),...(capability==="finance.production-costing"?manufacturingFinanceSourceReadLocks(database):[]),...(capability==="assets.parts-costs"?maintenanceFinanceSourceReadLocks(database):[]),...(commercePostCapability(capability)?commerceFinanceSourceReadLocks(database):[]),...(capability==="finance.inventory-valuation"||kind==="journal"&&["finance.sales-posting","finance.production-costing","finance.pos-posting","finance.service-invoice"].includes(capability)?inventoryFinanceSourceReadLocks(database):[]),database.prepare(`SELECT id FROM ${tables[kind]} WHERE id=? AND company_id=? FOR SHARE`).bind(id,actor.companyId),...(capability==="finance.sales-posting"&&kind==="invoice"?[database.prepare("SELECT command_id FROM command_receipts WHERE record_id=? AND operation='finance.sales-invoice.prepare' FOR SHARE").bind(id)]:[]),assertion(database,key,"finance-document-owner",`EXISTS(SELECT 1 FROM ${tables[kind]} d WHERE d.id=? AND d.company_id=? AND ${financeDocumentOwnerPredicate(kind,capability,commerceProof)})`,[id,actor.companyId])];
}
const commercePostCapability=(capability:string)=>["finance.sales-posting","finance.inventory-valuation"].includes(capability);
export function financeSourceDocumentReadLeases(database:SqlDatabase,companyId:string,paymentWriter=false,source?:"supplier-correction",refundCashWriter=false){
 const companyLocks=companyPurchaseReadLocks(database,companyId);
 return [companyLocks[0],...fieldServiceFinanceSourceReadLocks(database),...commerceFinanceSourceReadLocks(database,paymentWriter,refundCashWriter),...inventoryFinanceSourceReadLocks(database),...maintenanceFinanceSourceReadLocks(database),...manufacturingFinanceSourceReadLocks(database),...(paymentWriter?[database.prepare("LOCK TABLE finance_payments,finance_payment_reversals,finance_invoices,finance_bank_statements,finance_bank_transactions IN SHARE ROW EXCLUSIVE MODE")]:[]),...(source==="supplier-correction"?[database.prepare("LOCK TABLE finance_journals,finance_journal_lines,finance_book_postings,finance_financial_events,money_transaction_snapshots,command_receipts IN SHARE ROW EXCLUSIVE MODE"),database.prepare("LOCK TABLE finance_charts,finance_accounts,finance_book_rules,finance_book_account_mappings IN SHARE MODE")]:[])];
}
/** The complete source-first parent may acquire these query-free leases before
 * File/Tax/IAM leases. Original callers retain the identical full sequence. */
export function financeSourceDocumentReadLocks(database:SqlDatabase,companyId:string,paymentWriter=false,source?:"supplier-correction"){
 return [...financeSourceDocumentReadLeases(database,companyId,paymentWriter,source),...companyPurchaseReadLocks(database,companyId).slice(1)];
}
async function commercePostSource(database:SqlDatabase,actor:WorkContext,kind:FinanceDocumentKind,id:string,capability:string,commerceProof?:CommerceProof){
 return (commercePostCapability(capability)||capability==="assets.parts-costs"&&kind==="journal"||capability==="finance.service-invoice"&&(kind==="journal"||kind==="invoice"))&&!!await database.prepare(`SELECT 1 FROM ${tables[kind]} d WHERE d.id=? AND d.company_id=? AND ${capability==="assets.parts-costs"||capability==="finance.service-invoice"?ownerPredicate(kind,capability):commerceOwnerPredicate(kind,capability,commerceProof)}`).bind(id,actor.companyId).first();
}
/** Only original independent posting commands use these included foundations.
 * Generic document creation/edit/reversal retains its full Finance purchase. */
export async function withFinanceDocumentPost<Result>(actor:WorkContext,kind:"invoice"|"journal",id:string,command:{key:string;operation:"finance.journal.post"|"finance.invoice.post"|"flow.assign"},run:(transaction?:SqlDatabase)=>Promise<Result>){
 return withFinanceSourceDocuments(actor,[{kind,id}],command,run);
}
/** Closed owner operations supply actual IDs. An included source never grants
 * an arbitrary Finance operation just because it shares the same table. */
export async function withFinanceSourceDocuments<Result>(actor:WorkContext,targets:readonly {kind:FinanceDocumentKind;id:string}[],command:{key:string;operation:"finance.journal.post"|"finance.invoice.post"|"flow.assign"|"finance.refund.approve"|"finance.refund.prepare"|"finance.refund.export"|"finance.refund.exception"|"finance.refund.confirm"|"finance.bank.import"|`finance.supplier_batch.${"create"|"approve"|"cancel"|"export"|"result"|"confirm"}`;baseCapability?:"finance.banking";strictPurchase?:true},run:(transaction?:SqlDatabase)=>Promise<Result>,owned?:SqlDatabase){
 const mode=command.strictPurchase?"enforced" as const:await financePackageEnforcementMode();if(mode==="disabled")return run(owned);
 const owner=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,"TRANSACTION_REQUIRED","Source posting requires the original Holding transaction.");
 const execute=async(transaction:SqlDatabase)=>{
  // These closed source commands must establish the owner relation leases
  // before their first receipt/source snapshot, including absent relations.
  await transaction.batch(financeSourceDocumentReadLocks(transaction,actor.companyId));
  const receipt=await transaction.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,command.operation,actor.userId,actor.companyId).first(),purpose=receipt?"history" as const:"settle-existing" as const;
  const sources:{kind:FinanceDocumentKind;id:string;capability:string;additionalCapabilities?:readonly string[];commerceProof?:CommerceProof}[]=[];for(const target of [...targets].sort((a,b)=>a.kind.localeCompare(b.kind)||a.id.localeCompare(b.id))){
   if(command.baseCapability){
    await requireFinanceCurrentAuthority(transaction,actor,`finance-bank-source:${crypto.randomUUID()}`);const purchases=await readPurchasedAccesses(transaction,mode,actor.companyId,[command.baseCapability,"finance.inventory-valuation"]);let capability:string=command.baseCapability;
    if(purchases[capability].codes[purpose]!=="GRANTED"&&purchases["finance.inventory-valuation"].codes[purpose]==="GRANTED"&&await commercePostSource(transaction,actor,target.kind,target.id,"finance.inventory-valuation"))capability="finance.inventory-valuation";
    await checkPurchasedCapability(transaction,mode,{companyId:actor.companyId,capability,purpose});sources.push({...target,capability});
   }else{const access=await requireFinanceDocumentHistory(transaction,actor,target.kind,target.id,purpose,command.strictPurchase);sources.push({...target,additionalCapabilities:access.additionalCapabilities,commerceProof:access.commerceProof,capability:await commercePostSource(transaction,actor,target.kind,target.id,access.capability,access.commerceProof)?access.capability:base(target.kind)});}
  }
  const capability=sources[0]?.capability??"finance.banking",key=`finance-source-post:${crypto.randomUUID()}`;
  const fence=()=>sources.flatMap((target,index)=>[...(target.capability===command.baseCapability?[]:financeDocumentSourceGuards(transaction,actor,target.kind,target.id,target.capability,`${key}:${index}`,target.commerceProof)),transaction.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(`${key}:${index}`)]);
  await transaction.batch(fence());
  const result=await withFinancePurchasedCommand(actor,{...command,purpose:"settle-existing",capability,companyPurchases:async()=>[...sources.slice(1).map(target=>({companyId:actor.companyId,capability:target.capability,purpose})),...sources.flatMap(target=>(target.additionalCapabilities??[]).map(capability=>({companyId:actor.companyId,capability,purpose})))]},run,transaction);
  // The normal Finance boundary has flushed complete deferred money/book
  // effects and checked current authority before this final owner fence.
  await transaction.batch(fence());return result;
 };
 try{if(owned){const ownership=owner as SqlDatabase&{assertActiveTransaction?:(database:SqlDatabase)=>void};if(!ownership.assertActiveTransaction)throw new ApiError(503,"TRANSACTION_OWNERSHIP_REQUIRED","Finance requires its original owned Holding transaction.");ownership.assertActiveTransaction(owned);return await execute(owned);}return await owner.transaction(execute);}catch(error){if((error as {code?:string}).code==="23514"&&String((error as {detail?:string}).detail).includes("finance-document-owner"))throw new ApiError(409,"FINANCE_DOCUMENT_SOURCE_CHANGED","The original Finance document source changed before posting committed. Refresh its owner workflow.");throw error;}
}
export async function financeDocumentPostAccess(actor:WorkContext,kind:"invoice"|"journal",id:string){
 if(await financePackageEnforcementMode()==="disabled")return undefined;
 const database=(await holdingResources(actor.holdingId)).write,access=await requireFinanceDocumentHistory(database,actor,kind,id),source=await commercePostSource(database,actor,kind,id,access.capability,access.commerceProof),capabilities=[...new Set([base(kind),...(source?[access.capability,...access.additionalCapabilities]:[])])],purchases=await readPurchasedAccesses(database,await financePackageEnforcementMode(),actor.companyId,capabilities);
 return {canPost:purchases[base(kind)].settleExisting||source&&[access.capability,...access.additionalCapabilities].every(capability=>purchases[capability].settleExisting)};
}
export async function financeDocumentFlowAccess(database:SqlDatabase,actor:WorkContext,kind:"invoice"|"journal",id:string){
 const source=await requireFinanceDocumentHistory(database,actor,kind,id),capability=await commercePostSource(database,actor,kind,id,source.capability,source.commerceProof)?source.capability:base(kind);
 const access=await financeSourceAccess(database,actor,[capability,...source.additionalCapabilities]),selected=access[capability];return {...selected,newWork:selected.newWork&&source.additionalCapabilities.every(value=>access[value].newWork),settleExisting:selected.settleExisting&&source.additionalCapabilities.every(value=>access[value].settleExisting),history:selected.history&&source.additionalCapabilities.every(value=>access[value].history)};
}
export async function requireFinanceWorkspaceHistory(database:SqlDatabase,actor:WorkContext){await requireFinancePurchasedRead(database,actor);}
export async function financeFlowAccess(database:SqlDatabase,actor:WorkContext){return (await financeSourceAccess(database,actor,["finance.accounting"]))["finance.accounting"];}
export function withFinanceFlowAssignment<Result>(actor:WorkContext,key:string,run:(transaction?:SqlDatabase)=>Promise<Result>,target?:{kind:"invoice"|"journal";id:string}){return target?withFinanceDocumentPost(actor,target.kind,target.id,{key,operation:"flow.assign"},run):withFinancePurchasedCommand(actor,{key,operation:"flow.assign",purpose:"settle-existing"},run);}
/** Generic Finance preview/search records remain read-only. Filter before the
 * shared search LIMIT, with fresh assignment and one materialized SQL clock. */
export async function financeWorkspaceSearchScope(database:SqlDatabase,actor:WorkContext){
 const mode=await financePackageEnforcementMode();
 if(mode==='shadow')await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:'finance.accounting',purpose:'history'});
 const purchase=mode==='enforced'?`(SELECT ${purchasedCapabilityPredicate('workspace_records.company_id','finance.accounting','history','finance_search_clock.now_ms')} FROM finance_search_clock)`:'TRUE';
 const initial=`EXISTS(SELECT 1 FROM finance_journals d WHERE d.id=workspace_records.id AND d.company_id=workspace_records.company_id AND (${initialStockJournalPredicate} OR ${manufacturingInitialStockJournalPredicate('d')} OR ${posInitialStockJournalPredicate('d')} OR ${serviceInitialStockJournalPredicate('d')}))`;
 const rowFilter=`(SELECT ${initialPhysicalStockFinanceJournalFilter('workspace_records','finance_search_clock.now_ms')} FROM finance_search_clock)`;
 const authority=assignmentReadSql('workspace_records','finance.read');
 return {clock:`WITH finance_search_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) `,where:`(module_id<>'finance' OR ((CASE WHEN ${initial} THEN ${rowFilter} AND ${authority} ELSE ${rowFilter} AND ${purchase}${mode==='enforced'?` AND ${authority}`:''} END)))`,args:mode==='enforced'?[actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.userId,actor.role]:[actor.assignmentId,actor.userId,actor.role]};
}

/** Notifications remain durable; visibility is rechecked at read time before
 * search/cursor limits. The same original owner reader resolves each result. */
export async function financeNotificationReadScope(database:SqlDatabase,actor:WorkContext,surface:"staff"|"inbox"){
  const mode=await financePackageEnforcementMode();
  const type=surface==="staff"?"record_type":"e.source_type",id=surface==="staff"?"record_id":"e.source_id",company=surface==="staff"?"staff_notifications.company_id":"e.company_id";
  const purchase=(capability:string)=>`(SELECT ${purchasedCapabilityPredicate("d.company_id",capability,"history","finance_notice_clock.now_ms")} FROM finance_notice_clock)`;
  const document=(kind:"invoice"|"journal"|"refund"|"supplier-batch")=>`(${type}='${kind==="supplier-batch"?"supplier_batch":kind}' AND EXISTS(SELECT 1 FROM ${tables[kind]} d WHERE d.id=${id} AND d.company_id=${company} AND (${purchase(base(kind))} OR ${foundations.map(capability=>`(${purchase(capability)} AND ${ownerPredicate(kind,capability)}${capability==="finance.service-invoice"&&kind==="journal"?` AND (NOT ${servicePartsPredicate(kind)} OR ${purchase("service.parts-assets")})`:""})`).join(" OR ")})))`;
  const initialRow=`EXISTS(SELECT 1 FROM finance_journals d WHERE d.id=${id} AND d.company_id=${company} AND (${initialStockJournalPredicate} OR ${manufacturingInitialStockJournalPredicate('d')} OR ${posInitialStockJournalPredicate('d')} OR ${serviceInitialStockJournalPredicate('d')}))`;
  const knownInitial=`EXISTS(SELECT 1 FROM finance_journals d WHERE d.id=${id} AND d.company_id=${company} AND (EXISTS(SELECT 1 FROM (${roots('journal')}) root JOIN (${inventoryFinanceRecordSources.initialPhysicalCountMarkedJournal}) marker ON marker.id=root.id AND marker.company_id=root.company_id) OR ${manufacturingInitialStockJournalMarkedPredicate('d')} OR ${posInitialStockJournalMarkedPredicate('d')} OR ${serviceInitialStockJournalMarkedPredicate('d')}))`;
  const initialAllowed=`EXISTS(SELECT 1 FROM finance_journals d WHERE d.id=${id} AND d.company_id=${company} AND (SELECT ${initialPhysicalStockFinanceJournalFilter('d','finance_notice_clock.now_ms')} FROM finance_notice_clock))`;
  const current=`(SELECT ${assignmentReadSql('finance_notice_scope','finance.read')} FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) finance_notice_scope)`;
  const ordinary=mode==='enforced'?`(${document('invoice')} OR ${document('journal')} OR ${document('refund')} OR ${document('supplier-batch')})`:'TRUE';
  return {clock:`WITH finance_notice_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) `,where:`(${type} NOT IN ('invoice','journal','refund','supplier_batch') OR CASE WHEN ${type}='journal' AND ${knownInitial} THEN ${initialRow} AND ${initialAllowed} AND ${current} ELSE ${mode==='enforced'?`${current} AND `:''}${ordinary} END)`,args:mode==='enforced'?[actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId,actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId]:[actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId]};
}
export function financeRefundHistoryPredicate(alias:string,clock:string){
 const purchased=(capability:string)=>purchasedCapabilityPredicate(`${alias}.company_id`,capability,"history",clock);
 return `(${purchased("finance.banking")} OR EXISTS(SELECT 1 FROM finance_refund_obligations d WHERE d.id=${alias}.id AND d.company_id=${alias}.company_id AND (${["finance.sales-posting","finance.inventory-valuation"].map(capability=>`(${purchased(capability)} AND ${ownerPredicate("refund",capability)})`).join(" OR ")})))`;
}

export type FinanceProductEntryScope={companyId:string;branchId:string|null;departmentId:string|null;userId:string;assignmentId:string;assignmentVersion:number;role:string};
export type FinanceProductDocumentSummary={id:string;number:string;date:string;status:string;version:number};
/** Product discovery is strict even while legacy command rollout is disabled.
 * The final native SELECT repeats purchase, release and current role/version
 * alongside actual source ownership; request labels cannot establish a source.
 * Only private same-owner presentation uses the minimal summary. */
export async function financeProductDocumentRecord(database:SqlDatabase,scope:FinanceProductEntryScope,target:{kind:FinanceDocumentKind;id:string}){
 if(!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1||target.kind==="supplier-batch"&&(scope.branchId||scope.departmentId))return null;
 const initial=target.kind==='journal'&&await initialPhysicalStockJournalMarked(database,scope.companyId,target.id),serviceInitial=initial&&await serviceInitialStockJournalMarked(database,scope.companyId,target.id),posInitial=initial&&await posInitialStockJournalMarked(database,scope.companyId,target.id),manufacturingInitial=initial&&await manufacturingInitialStockJournalMarked(database,scope.companyId,target.id);
 const access=await readPurchasedAccesses(database,"enforced",scope.companyId,[base(target.kind),...foundations,"service.parts-assets",'sales.orders','inventory.stock-core',...manufacturingInitialStockJournalCapabilities,...posInitialStockJournalCapabilities,...serviceInitialStockJournalCapabilities]);
 const summary=target.kind==="invoice"?["invoice_number","invoice_date"]:target.kind==="journal"?["journal_number","entry_date"]:target.kind==="payment"?["payment_number","payment_date"]:null;
 for(const capability of [base(target.kind),...foundations]){
  if(!access[capability].history||initial&&(capability!==(serviceInitial?'finance.service-invoice':posInitial?'finance.pos-posting':manufacturingInitial?'finance.production-costing':'finance.sales-posting')||!(serviceInitial?serviceInitialStockJournalCapabilities:posInitial?posInitialStockJournalCapabilities:manufacturingInitial?manufacturingInitialStockJournalCapabilities:['sales.orders','inventory.stock-core']).every(cap=>access[cap].history)))continue;
  for(const proof of commerceProofs(target.kind,capability)){
  const where=initial?(serviceInitial?serviceInitialStockJournalPredicate('d'):posInitial?posInitialStockJournalPredicate('d'):manufacturingInitial?manufacturingInitialStockJournalPredicate('d'):initialStockJournalPredicate):capability===base(target.kind)?"TRUE":ownerPredicate(target.kind,capability,proof);
  const extra=capability==="finance.service-invoice"&&target.kind==="journal"?`(NOT (${servicePartsPredicate(target.kind)}) OR ${purchasedCapabilityPredicate("finance_entry_scope.company_id","service.parts-assets","history","finance_entry_clock.now_ms")})`:"TRUE";
  const row=await database.prepare(`WITH finance_entry_scope AS MATERIALIZED(SELECT ?::text company_id,?::text branch_id,?::text department_id),finance_entry_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms)
   SELECT d.id${summary?`,d.${summary[0]} number,d.${summary[1]}::text date,d.status,d.version`:""}
   FROM ${tables[target.kind]} d CROSS JOIN finance_entry_scope CROSS JOIN finance_entry_clock
   WHERE d.id=? AND d.company_id=finance_entry_scope.company_id AND ${assignmentReadSql("finance_entry_scope","finance.read")}
   AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND version=?) AND ${scheduledFeaturePredicate("finance_entry_scope.company_id","finance")}
   AND ${purchasedCapabilityPredicate("finance_entry_scope.company_id",capability,"history","finance_entry_clock.now_ms")} AND ${where} AND ${extra} ${initial?`AND ${(serviceInitial?serviceInitialStockJournalCapabilities:posInitial?posInitialStockJournalCapabilities:manufacturingInitial?manufacturingInitialStockJournalCapabilities:['sales.orders','inventory.stock-core']).map(cap=>purchasedCapabilityPredicate('finance_entry_scope.company_id',cap,'history','finance_entry_clock.now_ms')).join(' AND ')} AND ${(serviceInitial?['field_service','inventory']:posInitial?['pos','inventory']:manufacturingInitial?['manufacturing','inventory']:['sales','inventory']).map(module=>scheduledFeaturePredicate('finance_entry_scope.company_id',module)).join(' AND ')}`:''}`)
   .bind(scope.companyId,scope.branchId,scope.departmentId,target.id,scope.assignmentId,scope.userId,scope.role,scope.assignmentId,scope.assignmentVersion).first<{id:string;number?:string;date?:string;status?:string;version?:number}>();
  if(row)return {capability,purchasePurpose:"history" as const,summary:summary&&typeof row.number==="string"&&typeof row.date==="string"&&typeof row.status==="string"&&typeof row.version==="number"?{id:row.id,number:row.number,date:row.date,status:row.status,version:row.version}:null};
  }
 }
 return null;
}
/** Candidate role discovery exposes only its decision, not document metadata. */
export async function financeProductDocumentEntry(database:SqlDatabase,scope:FinanceProductEntryScope,target:{kind:FinanceDocumentKind;id:string}){
 const source=await financeProductDocumentRecord(database,scope,target);return source?{capability:source.capability,purchasePurpose:source.purchasePurpose}:null;
}
