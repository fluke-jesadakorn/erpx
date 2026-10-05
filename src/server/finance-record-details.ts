import {invoiceHistoryCte} from './finance-history';
import 'server-only';
import {z} from 'zod';
import type {WorkContext} from './authorization';
import {requirePermission} from './iam-permissions';
import {holdingResources} from './runtime';
import {ApiError} from './http';
import {businessContractWorkspace} from './finance-business-contracts';
import {bankAutomationWorkspace} from './finance-bank-automation';
import {collectionWorkspace,cashDueForecast} from './finance-collections';
export const financeExtraKinds=['finance_business_contract','finance_business_bill','finance_recognition','finance_obligation','bank_match_group','bank_profile','collection_case','cash_forecast_sources'] as const;
export type FinanceExtraKind=typeof financeExtraKinds[number];
export function isFinanceExtraKind(kind:string):kind is FinanceExtraKind{return financeExtraKinds.some(k=>k===kind);}
export async function financeExtraRecord(actor:WorkContext,kind:FinanceExtraKind,id:string,args:Record<string,string>,page:number){
 requirePermission(actor,'finance.read');const {write}=await holdingResources(actor.holdingId);
 if(kind==='bank_match_group'||kind==='bank_profile'){
  const data=await bankAutomationWorkspace(actor),table=kind==='bank_match_group'?'finance_bank_match_groups':'finance_bank_csv_mappings';
  const row=await write.prepare(`SELECT * FROM ${table} WHERE company_id=? AND id=?`).bind(actor.companyId,id).first<Record<string,string|number|null>>();
  if(!row)throw new ApiError(404,'BANK_RECORD_NOT_FOUND','Bank record not found.');
  return {kind,row,data:{...data,groups:kind==='bank_match_group'?[row]:[],mappings:kind==='bank_profile'?[row]:[],suggestions:[],rows:[],targets:[]}};
 }
 if(kind==='collection_case'){
  const data=await collectionWorkspace(actor,args.asOf??new Date().toISOString().slice(0,10));
  const row=await write.prepare('SELECT c.*,i.version current_invoice_version,CASE WHEN EXISTS(SELECT 1 FROM finance_collection_cases child WHERE child.parent_case_id=c.id) THEN 0 ELSE 1 END is_current FROM finance_collection_cases c JOIN finance_invoices i ON i.id=c.invoice_id AND i.company_id=c.company_id WHERE c.company_id=? AND c.id=?').bind(actor.companyId,id).first<Record<string,string|number|null>>();
  if(!row)throw new ApiError(404,'CASE_NOT_FOUND','Collection case not found.');
  return {kind,row,data:{...data,cases:[row],invoices:[]}};
 }
 if(kind==='cash_forecast_sources'){
  const chartId=z.string().min(1).max(180).parse(args.chartId),asOf=z.iso.date().parse(args.asOf),forecast=await cashDueForecast(actor,{chartId,asOf}),month=forecast.months.find(row=>row.month===id);
  if(!month)throw new ApiError(404,'FORECAST_MONTH_NOT_FOUND','Month not found in this forecast.');
  const invoiceIds=forecast.months.filter(m=>m.month<=id).flatMap(m=>m.invoiceIds),selected=invoiceIds.slice((page-1)*30,page*30);
  const rows=selected.length?(await write.prepare(`${invoiceHistoryCte} SELECT id,invoice_number,invoice_date,due_date,currency,status,historical_outstanding_minor outstanding_minor FROM historical_invoice_balances WHERE id IN (${selected.map(()=>'?').join(',')}) ORDER BY due_date,id`).bind(actor.companyId,asOf,...selected).all()).results:[];
  const accounts=(await write.prepare(`SELECT a.id,a.name,a.code FROM finance_accounts a WHERE a.company_id=? AND a.chart_id=? AND a.control_kind='bank' AND EXISTS(SELECT 1 FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE l.account_id=a.id AND j.status IN ('posted','reversed') AND j.entry_date<=?) ORDER BY a.code`).bind(actor.companyId,chartId,asOf).all<{id:string}>()).results;
  return {kind,row:{name:`Cash forecast · ${id}`,currency:forecast.currency,reported_total:month.closing,description:`Opening cash ${forecast.opening} · Month movement ${month.movement} · ${forecast.basis}`},rows,page,hasNext:invoiceIds.length>page*30,related:accounts.map(account=>({kind:'finance_account' as const,id:account.id,args:{endDate:asOf}}))};
 }
 const data=await businessContractWorkspace(actor),table=kind==='finance_business_contract'?'finance_business_contracts':kind==='finance_recognition'?'finance_recognition_runs':'finance_business_billing_runs';
 const row=await write.prepare(`SELECT * FROM ${table} WHERE company_id=? AND id=?`).bind(actor.companyId,id).first<Record<string,string|number|null>>();
 if(!row)throw new ApiError(404,'BUSINESS_RECORD_NOT_FOUND','Business record not found.');
 if(kind==='finance_business_contract')return {kind,row,data:{...data,contracts:[row],bills:[],recognitions:[]}};
 if(kind==='finance_recognition')return {kind,row,data:{...data,contracts:[],bills:[],recognitions:[row]}};
 const bill=await write.prepare('SELECT b.*,COALESCE(b.contract_content_json,c.content_json) effective_contract_content_json,i.invoice_number,i.version current_invoice_version,COALESCE(i.status,b.no_charge_status) invoice_status,CASE WHEN EXISTS(SELECT 1 FROM finance_business_billing_runs child WHERE child.parent_run_id=b.id) THEN 0 ELSE 1 END is_current FROM finance_business_billing_runs b JOIN finance_business_contracts c ON c.id=b.contract_id AND c.company_id=b.company_id LEFT JOIN finance_invoices i ON i.id=b.invoice_id WHERE b.id=? AND b.company_id=?').bind(id,actor.companyId).first<Record<string,string|number|null>>();
 if(!bill)throw new ApiError(404,'BILL_NOT_FOUND','Business invoice not found.');
 if(kind==='finance_obligation'){
  const result=JSON.parse(String(bill.result_json)) as {items:Record<string,unknown>[]},item=result.items.find(item=>item.key===args.item);
  if(!item)throw new ApiError(404,'OBLIGATION_NOT_FOUND','Obligation not found in this billing plan.');
  bill.result_json=JSON.stringify({...result,items:[item]});
 }
 return {kind,row:bill,data:{...data,contracts:[],bills:[bill],recognitions:data.recognitions.filter(r=>r.billing_run_id===id)}};
}
