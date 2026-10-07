import {z} from 'zod';
import type {WorkContext} from '@/server/modules/identity_access/public/authorization';
import type {FinanceError} from '../domain/invoice-line-revision';
import type {FinanceExtraKind,FinanceRecordRow as Row} from '../domain/record-kinds';
export type ContractWorkspaceFacts={recognitions:Row[]};
export type ForecastFacts={currency:string;opening:string;basis:string;months:{month:string;closing:string;movement:string;invoiceIds:string[]}[]};
export interface FinanceRecordDetailPorts<Contract extends ContractWorkspaceFacts,Bank extends object,Collection extends object,Forecast extends ForecastFacts>{error:FinanceError;permission(actor:WorkContext,permission:'finance.read'):WorkContext;ready(actor:WorkContext):Promise<void>;today():string;bankWorkspace(actor:WorkContext):Promise<Bank>;contractWorkspace(actor:WorkContext):Promise<Contract>;collectionWorkspace(actor:WorkContext,asOf:string):Promise<Collection>;forecast(actor:WorkContext,input:{chartId:string;asOf:string}):Promise<Forecast>;row(actor:WorkContext,kind:FinanceExtraKind,id:string):Promise<Row|null>;collectionCase(actor:WorkContext,id:string):Promise<Row|null>;bill(actor:WorkContext,id:string):Promise<Row|null>;historicalInvoices(actor:WorkContext,asOf:string,ids:string[]):Promise<Record<string,unknown>[]>;bankAccounts(actor:WorkContext,chart:string,asOf:string):Promise<{id:string}[]>;}
export function createFinanceRecordDetails<Contract extends ContractWorkspaceFacts,Bank extends object,Collection extends object,Forecast extends ForecastFacts>(ports:FinanceRecordDetailPorts<Contract,Bank,Collection,Forecast>){async function financeExtraRecord(actor:WorkContext,kind:FinanceExtraKind,id:string,args:Record<string,string>,page:number){
 ports.permission(actor,'finance.read');await ports.ready(actor);
 if(kind==='bank_match_group'||kind==='bank_profile'){
  const data=await ports.bankWorkspace(actor);
  const row=await ports.row(actor,kind,id);
  if(!row)throw ports.error(404,'BANK_RECORD_NOT_FOUND','Bank record not found.');
  return {kind,row,data:{...data,groups:kind==='bank_match_group'?[row]:[],mappings:kind==='bank_profile'?[row]:[],suggestions:[],rows:[],targets:[]}};
 }
 if(kind==='collection_case'){
  const data=await ports.collectionWorkspace(actor,args.asOf??ports.today());
  const row=await ports.collectionCase(actor,id);
  if(!row)throw ports.error(404,'CASE_NOT_FOUND','Collection case not found.');
  return {kind,row,data:{...data,cases:[row],invoices:[]}};
 }
 if(kind==='cash_forecast_sources'){
  const chartId=z.string().min(1).max(180).parse(args.chartId),asOf=z.iso.date().parse(args.asOf),forecast=await ports.forecast(actor,{chartId,asOf}),month=forecast.months.find(row=>row.month===id);
  if(!month)throw ports.error(404,'FORECAST_MONTH_NOT_FOUND','Month not found in this forecast.');
  const invoiceIds=forecast.months.filter(m=>m.month<=id).flatMap(m=>m.invoiceIds),selected=invoiceIds.slice((page-1)*30,page*30);
  const rows=selected.length?await ports.historicalInvoices(actor,asOf,selected):[];
  const accounts=await ports.bankAccounts(actor,chartId,asOf);
  return {kind,row:{name:`Cash forecast · ${id}`,currency:forecast.currency,reported_total:month.closing,description:`Opening cash ${forecast.opening} · Month movement ${month.movement} · ${forecast.basis}`},rows,page,hasNext:invoiceIds.length>page*30,related:accounts.map(account=>({kind:'finance_account' as const,id:account.id,args:{endDate:asOf}}))};
 }
 const data=await ports.contractWorkspace(actor);
 const row=await ports.row(actor,kind,id);
 if(!row)throw ports.error(404,'BUSINESS_RECORD_NOT_FOUND','Business record not found.');
 if(kind==='finance_business_contract')return {kind,row,data:{...data,contracts:[row],bills:[],recognitions:[]}};
 if(kind==='finance_recognition')return {kind,row,data:{...data,contracts:[],bills:[],recognitions:[row]}};
 const bill=await ports.bill(actor,id);
 if(!bill)throw ports.error(404,'BILL_NOT_FOUND','Business invoice not found.');
 if(kind==='finance_obligation'){
  const result=JSON.parse(String(bill.result_json)) as {items:Record<string,unknown>[]},item=result.items.find(item=>item.key===args.item);
  if(!item)throw ports.error(404,'OBLIGATION_NOT_FOUND','Obligation not found in this billing plan.');
  bill.result_json=JSON.stringify({...result,items:[item]});
 }
 return {kind,row:bill,data:{...data,contracts:[],bills:[bill],recognitions:data.recognitions.filter(r=>r.billing_run_id===id)}};
}
return {financeExtraRecord};}
