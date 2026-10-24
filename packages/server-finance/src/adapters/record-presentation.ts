import {posInitialStockJournalMarked} from '@erpx/server-finance/adapters/pos-initial-stock-journal-sources';
import {resolveFlow} from '@erpx/server-workflow/public/flow-routing';
import {withManufacturingInitialStockJournalRead} from '@erpx/server-finance/adapters/manufacturing-initial-stock-journal-read';
import {manufacturingInitialStockJournalMarked} from '@erpx/server-finance/adapters/manufacturing-initial-stock-journal-sources';
import "server-only";
import {financePackageEnforcementMode,readPurchasedAccesses} from "@erpx/server-commercial/public/purchase-command-boundary";
import {financeDocumentFlowAccess,requireFinanceDocumentHistory,financeProductDocumentRecord,type FinanceDocumentKind} from "@erpx/server-finance/adapters/document-purchases";
import {financeSourceAccess} from "@erpx/server-finance/adapters/source-purchases";
import {requireFinanceCurrentAuthority} from "@erpx/server-finance/adapters/purchase-boundary";
import {documentFlow} from "@erpx/server-workflow/public/document-flow";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {ApiError} from "@erpx/server-runtime/public/http";
import {getInvoice} from '@erpx/server-finance/public/finance-invoice-read';
import {journalDetail} from "@erpx/server-finance/adapters/ledger";
import {getRefund} from "@erpx/server-finance/adapters/refunds";
import type {LedgerRow} from "@erpx/server-finance/domain/ledger";
import type {FinanceRow} from "@erpx/server-finance/domain/finance";

/** A record page needs references for that book, not every financial record and
 * company-wide statements from the full register workspace. */
async function references(db:SqlDatabase,actor:WorkContext,chartId:string){
 const chart=await db.prepare("SELECT * FROM finance_charts WHERE id=? AND company_id=?").bind(chartId,actor.companyId).first<LedgerRow>();
 if(!chart)throw new ApiError(404,"CHART_NOT_FOUND","The record's book is unavailable.");
 const [accounts,periods,dimensions,dimensionValues]=await Promise.all([
  db.prepare("SELECT * FROM finance_accounts WHERE chart_id=? AND company_id=? ORDER BY code,id").bind(chartId,actor.companyId).all<FinanceRow>(),
  db.prepare("SELECT * FROM finance_periods WHERE company_id=? ORDER BY start_date DESC,id").bind(actor.companyId).all<FinanceRow>(),
  db.prepare("SELECT id,code,name,required FROM finance_dimension_definitions WHERE company_id=? AND status='active' ORDER BY code").bind(actor.companyId).all<LedgerRow>(),
  db.prepare("SELECT id,dimension_id,code,name FROM finance_dimension_values WHERE company_id=? AND status='active' ORDER BY code").bind(actor.companyId).all<LedgerRow>(),
 ]);
 return {chart,accounts:accounts.results,periods:periods.results,dimensions:dimensions.results,dimensionValues:dimensionValues.results};
}
export async function journalRecordWorkspace(actor:WorkContext,id:string){
 return withManufacturingInitialStockJournalRead(actor,id,async db=>{const detail=await journalDetail(actor,id,db);return {...detail,...await references(db,actor,String(detail.journal.chart_id))};});
}
export async function invoiceRecordWorkspace(actor:WorkContext,id:string){
 const db=(await holdingResources(actor.holdingId)).write,detail=await getInvoice(actor,id,db);
 const account=await db.prepare("SELECT chart_id FROM finance_accounts WHERE id=? AND company_id=?").bind(detail.invoice.debit_account_id,actor.companyId).first<{chart_id:string}>();
 if(!account)throw new ApiError(404,"ACCOUNT_NOT_FOUND","The invoice account is unavailable.");
 const refs=await references(db,actor,account.chart_id);
 const parties=await db.prepare(`SELECT * FROM finance_parties WHERE company_id=? AND (id=? OR (? AND status='active' AND person_id IS NULL)) ORDER BY name,id`).bind(actor.companyId,detail.invoice.party_id,detail.invoice.status==="draft"&&detail.invoice.source_type==="standalone").all<FinanceRow>();
 return {...detail,options:{charts:[refs.chart],accounts:refs.accounts,periods:refs.periods,parties:parties.results,invoices:[] as FinanceRow[]}};
}

/** Read-only route context, never an acting authority or a command grant. */
export type FinanceProductPresentationOptions={strictProduct?:true};
async function strictProductRecord(db:SqlDatabase,actor:WorkContext,kind:FinanceDocumentKind,id:string){
 await requireFinanceCurrentAuthority(db,actor,`finance-product-presentation:${crypto.randomUUID()}`);
 const version=actor.authority?.assignmentVersion??actor.iam?.queuedAssignmentVersion??await db.prepare("SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=?").bind(actor.assignmentId,actor.userId,actor.companyId,actor.role).first<number>("version");
 const record=typeof version==="number"?await financeProductDocumentRecord(db,{companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,userId:actor.userId,assignmentId:actor.assignmentId,assignmentVersion:version,role:actor.role},{kind,id}):null;
 if(!record)throw new ApiError(404,"FINANCE_RECORD_NOT_FOUND","The financial source record is unavailable.");
 return record;
}
export async function financeDocumentPresentationAccess(actor:WorkContext,kind:FinanceDocumentKind,id:string,options:FinanceProductPresentationOptions={}){
 const initialOwner=(await holdingResources(actor.holdingId)).write;if(kind==='journal'&&(await manufacturingInitialStockJournalMarked(initialOwner,actor.companyId,id)||await posInitialStockJournalMarked(initialOwner,actor.companyId,id)))return withManufacturingInitialStockJournalRead(actor,id,async()=>({sourceOnly:true,historyOnly:true}));
 if(options.strictProduct){const db=(await holdingResources(actor.holdingId)).write,source=await strictProductRecord(db,actor,kind,id),capability=kind==="payment"||kind==="refund"||kind==="supplier-batch"?"finance.banking":"finance.accounting",access=(await readPurchasedAccesses(db,"enforced",actor.companyId,[capability]))[capability];return {sourceOnly:source.capability!==capability,historyOnly:!access.settleExisting};}
 if(await financePackageEnforcementMode()!=="enforced")return {sourceOnly:false,historyOnly:false};
 const db=(await holdingResources(actor.holdingId)).write,capability=kind==="payment"||kind==="refund"||kind==="supplier-batch"?"finance.banking":"finance.accounting",access=(await financeSourceAccess(db,actor,[capability]))[capability];
 if(!access.history)await requireFinanceDocumentHistory(db,actor,kind,id);
 return {sourceOnly:!access.history,historyOnly:!access.settleExisting};
}
/** A parallel document slot must not require the entire financial register or
 * serialize unrelated rows to construct its background on a hard reload. */
export async function financeSourceDocumentBackdrop(actor:WorkContext,kind:FinanceDocumentKind,id:string,options:FinanceProductPresentationOptions={}){
 if(kind==='journal'&&(await manufacturingInitialStockJournalMarked((await holdingResources(actor.holdingId)).write,actor.companyId,id)||await posInitialStockJournalMarked((await holdingResources(actor.holdingId)).write,actor.companyId,id)))return withManufacturingInitialStockJournalRead(actor,id,async database=>{
  const row=await database.prepare('SELECT id,journal_number number,entry_date::text date,status,version FROM finance_journals WHERE id=? AND company_id=?').bind(id,actor.companyId).first<{id:string;number:string;date:string;status:string;version:number}>();
  if(!row)throw new ApiError(404,'FINANCE_RECORD_NOT_FOUND','The financial source record is unavailable.');return {kind,...row};
 });
 if(options.strictProduct){const db=(await holdingResources(actor.holdingId)).write,source=await strictProductRecord(db,actor,kind,id),capability=kind==="payment"||kind==="refund"||kind==="supplier-batch"?"finance.banking":"finance.accounting";if(source.capability===capability)return null;if(source.summary)return {kind,...source.summary};}
 if(!(await financeDocumentPresentationAccess(actor,kind,id,options)).sourceOnly)return null;
 if(kind==="refund"){const record=await getRefund(actor,id);return {kind,id,number:String(record.return_number),date:String(record.return_date),status:String(record.status),version:Number(record.version)};}
 if(kind==="supplier-batch"){const record=await (await import("@erpx/server-procurement/public/supplier-payments")).getSupplierBatch(actor,id);return {kind,id,number:String(record.id),date:String(record.planned_date),status:String(record.status),version:Number(record.version)};}
 const db=(await holdingResources(actor.holdingId)).write;
 const projection=kind==="invoice"?"finance_invoices,invoice_number,invoice_date":kind==="journal"?"finance_journals,journal_number,entry_date":"finance_payments,payment_number,payment_date";
 const [table,number,date]=projection.split(",");
 const record=await db.prepare(`SELECT id,${number} number,${date}::text date,status,version FROM ${table} WHERE id=? AND company_id=?`).bind(id,actor.companyId).first<{id:string;number:string;date:string;status:string;version:number}>();
 if(!record)throw new ApiError(404,"FINANCE_RECORD_NOT_FOUND","The financial source record is unavailable.");
 return {kind,...record};
}
export async function financeDocumentFlow(actor:WorkContext,kind:"invoice"|"journal",id:string){
 if(kind==='journal'&&(await manufacturingInitialStockJournalMarked((await holdingResources(actor.holdingId)).write,actor.companyId,id)||await posInitialStockJournalMarked((await holdingResources(actor.holdingId)).write,actor.companyId,id)))return withManufacturingInitialStockJournalRead(actor,id,async database=>({...await resolveFlow(database,actor,'journal',id),canAct:false,canManage:false}));
 const db=(await holdingResources(actor.holdingId)).write;const access=await financeDocumentFlowAccess(db,actor,kind,id),flow=await documentFlow(actor,kind,id);
 return access.settleExisting?flow:{...flow,canAct:false,canManage:false};
}
