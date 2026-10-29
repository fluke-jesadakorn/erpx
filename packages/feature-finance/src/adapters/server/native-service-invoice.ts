import 'server-only';
import {cache} from 'react';
import {headers} from 'next/headers';
import type {FinanceServiceInvoiceWorkspaceResponse} from '@erpx/contracts/finance-journal-read';
import type {NativeJournalPageContext} from './native-journal-page';
import {createNativeFinanceWorkspaceRead,NativeFinanceReadError,type NativeFinanceReadConfiguration,type NativeFinanceReadTransport,type NativeJournalReadInput} from './native-journal-detail';
import {nativeFinanceConfiguration} from './native-journal-page';

type Row=Record<string,string|number|null>;
type Line=Row & {id:string;invoice_id:string;source_version:number;ordinal:number;description:string;quantity_millis:number|null;unit_price_minor:number|null;tax_bps:number|null;subtotal_minor:number;tax_minor:number;total_minor:number;account_id:string;tax_account_id:string|null;source_snapshot_json:string;currency:string;origin_kind:string};
type InvoiceWorkspace={invoice:Row;allocations:Row[];lines:Line[];lineHistory:Row[];thaiDocuments:Row[];options:{charts:Row[];accounts:Row[];periods:Row[];parties:Row[];invoices:Row[]};source:Result['data']['source'];sourceOnly:boolean};
type Result = FinanceServiceInvoiceWorkspaceResponse;
type Holding = NativeFinanceReadConfiguration['holdings'][number] & {invoiceIds:string[]};
type Configuration = Omit<NativeFinanceReadConfiguration,'holdings'> & {holdings:Holding[]};
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const text=(v:unknown,max=200)=>typeof v==='string'&&v.length>0&&new TextEncoder().encode(v).length<=max;
const integer=(v:unknown,min=0)=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=min;
const jsonValue=(v:unknown):boolean=>v===null||typeof v==='string'||typeof v==='boolean'||typeof v==='number'&&Number.isFinite(v)&&Math.abs(v)<=Number.MAX_SAFE_INTEGER||Array.isArray(v)&&v.every(jsonValue)||object(v)&&Object.values(v).every(jsonValue);
const ledger=(v:unknown):v is Record<string,unknown>=>object(v)&&Object.values(v).every(jsonValue);
const array=(v:unknown):v is Record<string,unknown>[]=>Array.isArray(v)&&v.length<=10000&&v.every(ledger);
const invalid=():never=>{throw new NativeFinanceReadError('NATIVE_FINANCE_RESPONSE_INVALID');};
function validate(value:unknown,holding:{holdingId:string},input:NativeJournalReadInput):Result {
 if(!object(value)||!object(value.data)||!object(value.context)||typeof value.sourceOnly!=='boolean')return invalid();
 const d=value.data,c=value.context,i=d.invoice,o=d.options,s=d.source;
 if(!ledger(i)||!object(o)||!object(s)||!array(d.allocations)||!array(d.lines)||!array(d.lineHistory)||!array(d.thaiDocuments)||!array(o.charts)||!array(o.accounts)||!array(o.periods)||!array(o.parties)||!array(o.invoices)||o.invoices.length!==0||[d.allocations,d.lines,d.lineHistory,d.thaiDocuments,o.charts,o.accounts,o.periods,o.parties].reduce((n,a)=>n+a.length,1)>10000)return invalid();
 if(i.id!==input.journalId||i.company_id!==c.companyId||i.source_type!=='field_service'||i.kind!=='receivable'||typeof i.invoice_number!=='string'||!text(i.invoice_date,32)||!text(i.status)||!integer(i.version,1)||!text(i.debit_account_id)||typeof i.party_name!=='string'||!text(i.currency,16)||!integer(i.total_minor)||!integer(i.outstanding_minor))return invalid();
 if(!text(s.commandKey)||i.id!==`field-invoice-${s.commandKey}`||s.releaseId!==`field-billing-${s.commandKey}`||s.orderId!==i.source_id||s.actorId!==i.created_by||s.currency!==i.currency||s.amountUnits!==String(i.total_minor)||typeof s.usdUnits!=='string'||!/^(0|[1-9][0-9]*)$/.test(s.usdUnits)||!(s.fxRateId===null||text(s.fxRateId))||typeof s.usedLatestFallback!=='boolean')return invalid();
 const chart=o.charts[0];
 if(o.charts.length!==1||!text(chart?.id)||chart?.company_id!==c.companyId||o.accounts.some(a=>a.company_id!==c.companyId||a.chart_id!==chart?.id)||o.periods.some(p=>p.company_id!==c.companyId)||o.parties.some(p=>p.company_id!==c.companyId)||d.lines.some(l=>l.invoice_id!==i.id||l.company_id!==c.companyId||!text(l.id)||!integer(l.source_version,1)||!integer(l.ordinal)||typeof l.description!=='string'||!['quantity_millis','unit_price_minor','tax_bps'].every(k=>l[k]===null||integer(l[k]))||!['subtotal_minor','tax_minor','total_minor'].every(k=>integer(l[k]))||!text(l.account_id)||!(l.tax_account_id===null||text(l.tax_account_id))||typeof l.source_snapshot_json!=='string'||!text(l.currency,16)||!text(l.origin_kind)))return invalid();
 if(Object.keys(c).length!==15||!['userId','holdingId','companyId','branchId','departmentId','branchSlug','departmentSlug','assignmentId','assignmentVersion','revision','role','assignmentScopeKind','modules','permissions','expiresAt'].every(k=>Object.hasOwn(c,k))||c.holdingId!==holding.holdingId||!text(c.userId)||!text(c.companyId)||!text(c.assignmentId)||!integer(c.assignmentVersion,1)||!integer(c.revision)||!integer(c.expiresAt,1)||!text(c.role)||!['holding','branch','department'].includes(String(c.assignmentScopeKind))||!Array.isArray(c.modules)||!c.modules.includes('finance')||!Array.isArray(c.permissions)||!c.permissions.includes('finance.read')||!c.permissions.includes('module.finance.read')||c.branchSlug!==(input.branchSlug??null)||c.departmentSlug!==(input.departmentSlug??null))return invalid();
 if(input.expected&&(c.companyId!==input.expected.companyId||c.assignmentId!==input.expected.assignmentId||c.revision!==input.expected.revision||c.assignmentVersion!==input.expected.assignmentVersion))throw new NativeFinanceReadError('NATIVE_FINANCE_CONTEXT_CHANGED');
 return value as unknown as Result;
}
function configuration(raw:string):Configuration {
 const base=nativeFinanceConfiguration(raw);const value=JSON.parse(raw) as {holdings:unknown[]};
 const holdings=value.holdings.map((entry,index)=>{
  if(!object(entry)||!Array.isArray(entry.invoiceIds)||entry.invoiceIds.length>1000||!entry.invoiceIds.every(id=>text(id)&&!id.includes('/')&&!id.includes('\\'))||new Set(entry.invoiceIds).size!==entry.invoiceIds.length)throw new NativeFinanceReadError('NATIVE_FINANCE_CONFIGURATION_INVALID');
  return {...base.holdings[index]!,invoiceIds:entry.invoiceIds as string[]};
 });
 if(new Set(holdings.map(h=>h.holdingSlug)).size!==holdings.length)throw new NativeFinanceReadError('NATIVE_FINANCE_CONFIGURATION_INVALID');
 return {...base,holdings};
}
const sameSet=(a:readonly string[],b:unknown)=>Array.isArray(b)&&b.every(x=>typeof x==='string')&&new Set(a).size===new Set(b).size&&a.every(x=>b.includes(x));
/** Explicit development selection; the list grants no access. */
function row(value:Record<string,unknown>):Row{return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,v===null||typeof v==='string'||typeof v==='number'?v:typeof v==='boolean'?Number(v):JSON.stringify(v)]));}
function view(result:Result):InvoiceWorkspace {const d=result.data;return {invoice:row(d.invoice),allocations:d.allocations.map(row),lines:d.lines.map(v=>row(v) as Line),lineHistory:d.lineHistory.map(row),thaiDocuments:d.thaiDocuments.map(row),options:{charts:d.options.charts.map(row),accounts:d.options.accounts.map(row),periods:d.options.periods.map(row),parties:d.options.parties.map(row),invoices:d.options.invoices.map(row)},source:d.source,sourceOnly:result.sourceOnly};}
export function createNativeServiceInvoiceReader(raw:string,transport?:NativeFinanceReadTransport){
 const config=configuration(raw);const read=createNativeFinanceWorkspaceRead(config,transport,validate);
 return async(actor:NativeJournalPageContext,incoming:Pick<Headers,'get'>,holdingSlug:string,id:string):Promise<InvoiceWorkspace|undefined>=>{
  const holding=config.holdings.find(h=>h.holdingSlug===holdingSlug);
  if(!holding||!holding.invoiceIds.includes(id))return undefined;
  if(!actor.authority||!actor.iam||actor.holdingSlug!==holdingSlug)throw new NativeFinanceReadError('NATIVE_FINANCE_CONTEXT_CHANGED');
  const branchSlug=actor.scope.kind==='holding'?undefined:actor.scope.branchSlug,departmentSlug=actor.scope.kind==='department'?actor.scope.departmentSlug:undefined;
  const result=await read(incoming,{holdingSlug,journalId:id,branchSlug,departmentSlug,expected:{companyId:actor.companyId,assignmentId:actor.assignmentId,revision:actor.authority.revision,assignmentVersion:actor.authority.assignmentVersion}});
  const captured={userId:actor.userId,holdingId:actor.holdingId,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,branchSlug:branchSlug??null,departmentSlug:departmentSlug??null,assignmentId:actor.assignmentId,assignmentVersion:actor.authority.assignmentVersion,revision:actor.authority.revision,role:actor.role,assignmentScopeKind:actor.assignmentScope};
  if(Object.entries(captured).some(([key,value])=>result.context[key as keyof typeof result.context]!==value)||!sameSet(actor.modules,result.context.modules)||!sameSet(actor.iam.permissions,result.context.permissions))throw new NativeFinanceReadError('NATIVE_FINANCE_CONTEXT_CHANGED');
  return view(result);
 };
}
export const nativeServiceInvoiceWorkspace=cache(async(actor:NativeJournalPageContext,holdingSlug:string,id:string)=>{
 const raw=process.env.ERPX_NATIVE_FINANCE_SERVICE_INVOICE_CONFIG;if(raw===undefined)return undefined;
 return createNativeServiceInvoiceReader(raw)(actor,await headers(),holdingSlug,id);
});
