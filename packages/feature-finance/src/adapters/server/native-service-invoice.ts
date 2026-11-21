import type {FlowView} from '@erpx/feature-shared/lib/flow';
import type {FinancePurchaseControls} from '@erpx/contracts/finance-purchase-controls';
import {nativeFinancePurchaseControls} from './native-finance-presentation';
import type {FinancePostingPreview} from '@erpx/feature-finance/public/contracts';
import 'server-only';
import {cache} from 'react';
import {headers} from 'next/headers';
import type {FinanceServiceInvoiceWorkspaceResponse} from '@erpx/contracts/finance-journal-read';
import type {NativeJournalPageContext} from './native-journal-page';
import {createNativeFinanceWorkspaceRead,createNativeServiceInvoiceRuntimeRead,NativeFinanceReadError,type NativeFinanceReadTransport,type NativeJournalReadInput} from './native-journal-detail';
import {nativeFinanceConfiguration} from './native-journal-page';

type Row=Record<string,string|number|null>;
type Line=Row & {id:string;invoice_id:string;source_version:number;ordinal:number;description:string;quantity_millis:number|null;unit_price_minor:number|null;tax_bps:number|null;subtotal_minor:number;tax_minor:number;total_minor:number;account_id:string;tax_account_id:string|null;source_snapshot_json:string;currency:string;origin_kind:string};
type InvoiceWorkspace={invoice:Row;allocations:Row[];lines:Line[];lineHistory:Row[];thaiDocuments:Row[];options:{charts:Row[];accounts:Row[];periods:Row[];parties:Row[];invoices:Row[]};source:Result['data']['source'];sourceOnly:boolean;postingPreview:FinancePostingPreview;purchaseControls:FinancePurchaseControls;sourcePurchase:SourcePurchase;canPost:boolean;customerReceipts:Row[];noCharge:FinanceServiceInvoiceWorkspaceResponse['noCharge'];flow:FlowView};
type SourcePurchase={capability:'finance.service-invoice';settleExisting:boolean;history:boolean;revision:string};
type Result = FinanceServiceInvoiceWorkspaceResponse & {purchaseControls:FinancePurchaseControls;sourcePurchase:SourcePurchase;canPost:boolean;flow:FlowView};
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const text=(v:unknown,max=200)=>typeof v==='string'&&v.length>0&&new TextEncoder().encode(v).length<=max;
const integer=(v:unknown,min=0)=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=min;
const jsonValue=(v:unknown):boolean=>v===null||typeof v==='string'||typeof v==='boolean'||typeof v==='number'&&Number.isFinite(v)&&Math.abs(v)<=Number.MAX_SAFE_INTEGER||Array.isArray(v)&&v.every(jsonValue)||object(v)&&Object.values(v).every(jsonValue);
const ledger=(v:unknown):v is Record<string,unknown>=>object(v)&&Object.values(v).every(jsonValue);
const array=(v:unknown):v is Record<string,unknown>[]=>Array.isArray(v)&&v.length<=10000&&v.every(ledger);
const invalid=():never=>{throw new NativeFinanceReadError('NATIVE_FINANCE_RESPONSE_INVALID');};
function postingPreview(value:unknown,companyId:unknown):FinancePostingPreview {
 if(!object(value))return invalid();
 if(value.state==='voided'||value.state==='no-charge-confirmed'){
  if(Object.keys(value).length!==1)return invalid();return {state:value.state};
 }
 if(value.state!=='ready'||Object.keys(value).sort().join(',')!=='accounts,credit,debit,lines,posted,state'||typeof value.posted!=='boolean'||!integer(value.debit)||!integer(value.credit)||!array(value.accounts)||!Array.isArray(value.lines)||value.lines.length>10000)return invalid();
 if(value.accounts.some(a=>a.company_id!==companyId)||value.lines.some(l=>!object(l)||Object.keys(l).sort().join(',')!=='credit,debit,id'||!text(l.id)||!integer(l.debit)||!integer(l.credit)))return invalid();
 return {state:'ready',posted:value.posted,accounts:value.accounts.map(row),lines:value.lines as {id:string;debit:number;credit:number}[],debit:value.debit as number,credit:value.credit as number};
}
function flowView(value:unknown,id:string,version:unknown,state:unknown):FlowView{
 const nullable=(v:unknown)=>v===null||typeof v==='string';
 const person=(p:unknown)=>object(p)&&text(p.userId)&&typeof p.name==='string'&&nullable(p.teamId)&&typeof p.teamName==='string';
 const step=(s:unknown)=>object(s)&&['draft','posted'].includes(String(s.key))&&text(s.action)&&text(s.role)&&Array.isArray(s.allowedRoles)&&s.allowedRoles.every(v=>text(v))&&typeof s.teamName==='string'&&(s.primary===null||person(s.primary))&&Array.isArray(s.candidates)&&s.candidates.length<=500&&s.candidates.every(person)&&['current','upcoming','recorded'].includes(String(s.status))&&nullable(s.blocker);
 if(!object(value)||value.kind!=='invoice'||value.recordId!==id||value.recordVersion!==version||value.state!==state||!integer(value.version)||!integer(value.defaultsRevision)||!text(value.state)||typeof value.canAct!=='boolean'||typeof value.canManage!=='boolean'||!nullable(value.blocker)||!Array.isArray(value.steps)||value.steps.length!==2||!value.steps.every(step)||value.steps[0]?.key!=='draft'||value.steps[1]?.key!=='posted'||!(value.current===null||step(value.current))||!(value.next===null||step(value.next))||!Array.isArray(value.history)||value.history.length>10000||!value.history.every(h=>object(h)&&text(h.action)&&typeof h.actorName==='string'&&integer(h.at)&&typeof h.reason==='string'&&typeof h.step==='string'))return invalid();
 return value as unknown as FlowView;
}
function validate(value:unknown,holding:{holdingId:string},input:NativeJournalReadInput):Result {
 if(!object(value)||!object(value.data)||!object(value.context)||typeof value.sourceOnly!=='boolean')return invalid();
 nativeFinancePurchaseControls(value.purchaseControls);
 const purchase=value.sourcePurchase;
 if(typeof value.canPost!=='boolean'||!object(purchase)||Object.keys(purchase).sort().join(',')!=='capability,history,revision,settleExisting'||purchase.capability!=='finance.service-invoice'||typeof purchase.settleExisting!=='boolean'||typeof purchase.history!=='boolean'||!text(purchase.revision,1000))return invalid();
 const d=value.data,c=value.context,i=d.invoice,o=d.options,s=d.source;
 postingPreview(d.postingPreview,c.companyId);
 flowView(value.flow,input.journalId,object(i)?i.version:undefined,object(i)?i.status:undefined);
 const noCharge=value.noCharge;
 if(noCharge!==null&&(!object(noCharge)||Object.keys(noCharge).sort().join(',')!=='canConfirm,confirmed,contextKey,invoiceId,version'||noCharge.invoiceId!==input.journalId||noCharge.version!==(object(i)?i.version:undefined)||typeof noCharge.contextKey!=='string'||!/^[a-f0-9]{64}$/.test(noCharge.contextKey)||typeof noCharge.canConfirm!=='boolean'||typeof noCharge.confirmed!=='boolean'))return invalid();
 if(object(noCharge)&&object(i)&&(i.total_minor!==0||i.outstanding_minor!==0||!['draft','no_charge_confirmed'].includes(String(i.status))||noCharge.confirmed!==(i.status==='no_charge_confirmed')||noCharge.confirmed&&noCharge.canConfirm))return invalid();
 if(!ledger(i)||!object(o)||!object(s)||!array(d.allocations)||!array(d.lines)||!array(d.lineHistory)||!array(d.thaiDocuments)||!array(d.customerReceipts)||!array(o.charts)||!array(o.accounts)||!array(o.periods)||!array(o.parties)||!array(o.invoices)||o.invoices.length!==0||[d.allocations,d.lines,d.lineHistory,d.thaiDocuments,d.customerReceipts,o.charts,o.accounts,o.periods,o.parties].reduce((n,a)=>n+a.length,1)>10000)return invalid();
 if(i.id!==input.journalId||i.company_id!==c.companyId||i.source_type!=='field_service'||i.kind!=='receivable'||typeof i.invoice_number!=='string'||!text(i.invoice_date,32)||!text(i.status)||!integer(i.version,1)||!text(i.debit_account_id)||typeof i.party_name!=='string'||!text(i.currency,16)||!integer(i.total_minor)||!integer(i.outstanding_minor))return invalid();
 if(!text(s.commandKey)||i.id!==`field-invoice-${s.commandKey}`||s.releaseId!==`field-billing-${s.commandKey}`||s.orderId!==i.source_id||s.actorId!==i.created_by||s.currency!==i.currency||s.amountUnits!==String(i.total_minor)||typeof s.usdUnits!=='string'||!/^(0|[1-9][0-9]*)$/.test(s.usdUnits)||!(s.fxRateId===null||text(s.fxRateId))||typeof s.usedLatestFallback!=='boolean')return invalid();
 const chart=o.charts[0];
 if(o.charts.length!==1||!text(chart?.id)||chart?.company_id!==c.companyId||o.accounts.some(a=>a.company_id!==c.companyId||a.chart_id!==chart?.id)||o.periods.some(p=>p.company_id!==c.companyId)||o.parties.some(p=>p.company_id!==c.companyId)||d.lines.some(l=>l.invoice_id!==i.id||l.company_id!==c.companyId||!text(l.id)||!integer(l.source_version,1)||!integer(l.ordinal)||typeof l.description!=='string'||!['quantity_millis','unit_price_minor','tax_bps'].every(k=>l[k]===null||integer(l[k]))||!['subtotal_minor','tax_minor','total_minor'].every(k=>integer(l[k]))||!text(l.account_id)||!(l.tax_account_id===null||text(l.tax_account_id))||typeof l.source_snapshot_json!=='string'||!text(l.currency,16)||!text(l.origin_kind)))return invalid();
 if(Object.keys(c).length!==15||!['userId','holdingId','companyId','branchId','departmentId','branchSlug','departmentSlug','assignmentId','assignmentVersion','revision','role','assignmentScopeKind','modules','permissions','expiresAt'].every(k=>Object.hasOwn(c,k))||c.holdingId!==holding.holdingId||!text(c.userId)||!text(c.companyId)||!text(c.assignmentId)||!integer(c.assignmentVersion,1)||!integer(c.revision)||!integer(c.expiresAt,1)||!text(c.role)||!['holding','branch','department'].includes(String(c.assignmentScopeKind))||!Array.isArray(c.modules)||!c.modules.includes('finance')||!Array.isArray(c.permissions)||!c.permissions.includes('finance.read')||!c.permissions.includes('module.finance.read')||c.branchSlug!==(input.branchSlug??null)||c.departmentSlug!==(input.departmentSlug??null))return invalid();
 if(input.expected&&(c.companyId!==input.expected.companyId||c.assignmentId!==input.expected.assignmentId||c.revision!==input.expected.revision||c.assignmentVersion!==input.expected.assignmentVersion))throw new NativeFinanceReadError('NATIVE_FINANCE_CONTEXT_CHANGED');
 return value as unknown as Result;
}
const sameSet=(a:readonly string[],b:unknown)=>Array.isArray(b)&&b.every(x=>typeof x==='string')&&new Set(a).size===new Set(b).size&&a.every(x=>b.includes(x));
/** Map only the source-qualified native response into its retained view. */
function row(value:Record<string,unknown>):Row{return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,v===null||typeof v==='string'||typeof v==='number'?v:typeof v==='boolean'?Number(v):JSON.stringify(v)]));}
function view(result:Result):InvoiceWorkspace {const d=result.data;return {invoice:row(d.invoice),allocations:d.allocations.map(row),lines:d.lines.map(v=>row(v) as Line),lineHistory:d.lineHistory.map(row),thaiDocuments:d.thaiDocuments.map(row),options:{charts:d.options.charts.map(row),accounts:d.options.accounts.map(row),periods:d.options.periods.map(row),parties:d.options.parties.map(row),invoices:d.options.invoices.map(row)},source:d.source,sourceOnly:result.sourceOnly,noCharge:result.noCharge,flow:flowView(result.flow,String(d.invoice.id),d.invoice.version,d.invoice.status),customerReceipts:d.customerReceipts.map(row),purchaseControls:result.purchaseControls,sourcePurchase:result.sourcePurchase,canPost:result.canPost,postingPreview:postingPreview(d.postingPreview,result.context.companyId)};}
export function createNativeServiceInvoiceReader(raw:string,transport?:NativeFinanceReadTransport){
 const config=nativeFinanceConfiguration(raw),read=createNativeFinanceWorkspaceRead(config,transport,validate);
 const owner=createNativeServiceInvoiceRuntimeRead(config,transport,(value,_holding,input):'rust'|'legacy'=>{
  if(!object(value)||Object.keys(value).join(',')!=='data'||!object(value.data)||Object.keys(value.data).sort().join(',')!=='id,runtime'||value.data.id!==input.journalId||!['rust','legacy'].includes(String(value.data.runtime)))return invalid();
  return value.data.runtime as 'rust'|'legacy';
 });
 return async(actor:NativeJournalPageContext,incoming:Pick<Headers,'get'>,holdingSlug:string,id:string):Promise<InvoiceWorkspace|undefined>=>{
  if(!actor.authority||!actor.iam||actor.holdingSlug!==holdingSlug)throw new NativeFinanceReadError('NATIVE_FINANCE_CONTEXT_CHANGED');
  const branchSlug=actor.scope.kind==='holding'?undefined:actor.scope.branchSlug,departmentSlug=actor.scope.kind==='department'?actor.scope.departmentSlug:undefined;
  const input={holdingSlug,journalId:id,branchSlug,departmentSlug,expected:{companyId:actor.companyId,assignmentId:actor.assignmentId,revision:actor.authority.revision,assignmentVersion:actor.authority.assignmentVersion}};
  if(await owner(incoming,input)==='legacy')return undefined;
  const result=await read(incoming,input);
  const captured={userId:actor.userId,holdingId:actor.holdingId,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,branchSlug:branchSlug??null,departmentSlug:departmentSlug??null,assignmentId:actor.assignmentId,assignmentVersion:actor.authority.assignmentVersion,revision:actor.authority.revision,role:actor.role,assignmentScopeKind:actor.assignmentScope};
  if(Object.entries(captured).some(([key,value])=>result.context[key as keyof typeof result.context]!==value)||!sameSet(actor.modules,result.context.modules)||!sameSet(actor.iam.permissions,result.context.permissions))throw new NativeFinanceReadError('NATIVE_FINANCE_CONTEXT_CHANGED');
  return view(result);
 };
}
export const nativeServiceInvoiceWorkspace=cache(async(actor:NativeJournalPageContext,holdingSlug:string,id:string)=>{
 const raw=process.env.ERPX_NATIVE_FINANCE_SERVICE_INVOICE_CONFIG;if(raw===undefined)return undefined;
 return createNativeServiceInvoiceReader(raw)(actor,await headers(),holdingSlug,id);
});

/** Same native source proof for the retained REST detail shape. */
export async function nativeServiceInvoiceReadback(request:Request,actor:NativeJournalPageContext,holdingSlug:string,id:string):Promise<Response|undefined>{
 const raw=process.env.ERPX_NATIVE_FINANCE_SERVICE_INVOICE_CONFIG;
 try {
  if(raw===undefined)throw new NativeFinanceReadError('NATIVE_FINANCE_CONFIGURATION_REQUIRED');
  const data=await createNativeServiceInvoiceReader(raw)(actor,request.headers,holdingSlug,id);
  if(!data)return undefined;
  const {invoice,allocations,lines,lineHistory,thaiDocuments}=data;
  return Response.json({data:{invoice,allocations,lines,lineHistory,thaiDocuments}},{headers:{'Cache-Control':'private, no-store'}});
 }catch(error){
  const status=error instanceof NativeFinanceReadError&&[401,403,404,409,422].includes(error.status??0)?error.status!:503;
  return Response.json({error:{code:error instanceof NativeFinanceReadError?error.code:'NATIVE_FINANCE_UNAVAILABLE',message:'The invoice is currently unavailable.'}},{status,headers:{'Cache-Control':'private, no-store'}});
 }
}

/** Fresh transport read after an acknowledged command. Never reuse the
 * request-scoped SSR memo containing the earlier route version. */
export async function nativeServiceInvoiceFlowReadback(request:Request,actor:NativeJournalPageContext,holdingSlug:string,id:string,command?:{id:string;version:number}):Promise<Response|undefined>{
 try{
  const raw=process.env.ERPX_NATIVE_FINANCE_SERVICE_INVOICE_CONFIG;
  if(raw===undefined)throw new NativeFinanceReadError('NATIVE_FINANCE_CONFIGURATION_REQUIRED');
  const view=await createNativeServiceInvoiceReader(raw)(actor,request.headers,holdingSlug,id);
  if(!view){if(command)throw new NativeFinanceReadError('NATIVE_FINANCE_CONTEXT_CHANGED');return undefined;}
  if(command&&(command.id!==id||!Number.isSafeInteger(command.version)||command.version<2))throw new NativeFinanceReadError('NATIVE_FINANCE_RESPONSE_INVALID');
  return Response.json({data:view.flow,...(command?{command}:{})},{headers:{'Cache-Control':'private, no-store',...(command?{'X-ERPX-Native-Flow':'field-invoice'}:{})}});
 }catch(error){
  const status=error instanceof NativeFinanceReadError&&[401,403,404,409,422].includes(error.status??0)?error.status!:error instanceof NativeFinanceReadError&&error.code==='NATIVE_FINANCE_CONTEXT_CHANGED'?409:503;
  return Response.json({error:{code:error instanceof NativeFinanceReadError?error.code:'NATIVE_FINANCE_UNAVAILABLE',message:'The current invoice workflow is unavailable. Retain the original request for retry.'}},{status,headers:{'Cache-Control':'private, no-store'}});
 }
}
