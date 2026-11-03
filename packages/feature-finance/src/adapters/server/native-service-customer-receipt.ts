import 'server-only';
import {createHash} from 'node:crypto';
import {cache} from 'react';
import {headers} from 'next/headers';
import type {FinanceRenderedWorkContext} from '@erpx/contracts/finance-journal-read';
import {createNativeServiceCustomerReceiptRead,NativeFinanceReadError,type NativeFinanceReadTransport,type NativeJournalReadInput} from './native-journal-detail';
import {nativeFinanceConfiguration,type NativeJournalPageContext} from './native-journal-page';
type Row=Record<string,string|number|null>;
type Snapshot={schema:'finance.customer-receipt.v1';documentNumber:string;companyName:string;customerName:string;customerCode:string;date:string;currency:string;amountMinor:string;paymentId:string;paymentVersion:number;bankReference:string;journalId:string;allocations:{invoiceId:string;invoiceNumber:string;amountMinor:string}[]};
export type NativeServiceCustomerReceipt={sha256:string;receipt:Row;snapshot:Snapshot;status:'issued'|'cancelled'};
type Reply={data:NativeServiceCustomerReceipt;export:{content:string;sha256:string;document_number:string};context:FinanceRenderedWorkContext};
const invalid=():never=>{throw new NativeFinanceReadError('NATIVE_FINANCE_RESPONSE_INVALID');};
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const exact=(v:unknown,keys:readonly string[]):v is Record<string,unknown>=>object(v)&&Object.keys(v).length===keys.length&&keys.every(k=>Object.hasOwn(v,k));
const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0;
const integer=(v:unknown,min=0):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=min;
const amount=(v:unknown):v is string=>typeof v==='string'&&/^(0|[1-9][0-9]{0,15})$/.test(v)&&Number.isSafeInteger(Number(v));
const row=(v:unknown):v is Row=>object(v)&&Object.values(v).every(x=>x===null||typeof x==='string'||typeof x==='number'&&Number.isSafeInteger(x));
function runtime(value:unknown,_holding:{holdingId:string},input:NativeJournalReadInput):'rust'|'legacy'{
 if(!exact(value,['data'])||!exact(value.data,['id','runtime'])||value.data.id!==input.journalId||!['rust','legacy'].includes(String(value.data.runtime)))return invalid();
 return value.data.runtime as 'rust'|'legacy';
}
function workspace(value:unknown,holding:{holdingId:string},input:NativeJournalReadInput):Reply{
 if(!input.expected||!exact(value,['data','export','context'])||!exact(value.data,['sha256','receipt','snapshot','status'])||!exact(value.export,['content','sha256','document_number'])||!object(value.context))return invalid();
 const d=value.data,r=d.receipt,s=d.snapshot,e=value.export,c=value.context;
 if(!row(r)||r.id!==input.journalId||r.company_id!==input.expected.companyId||Object.hasOwn(r,'export_html')||!['issued','cancelled'].includes(String(d.status))||typeof d.sha256!=='string'||! /^[a-f0-9]{64}$/.test(d.sha256)||typeof e.content!=='string'||e.sha256!==d.sha256||e.document_number!==r.document_number||createHash('sha256').update(e.content).digest('hex')!==d.sha256)return invalid();
 if(!exact(s,['schema','documentNumber','companyName','customerName','customerCode','date','currency','amountMinor','paymentId','paymentVersion','bankReference','journalId','allocations'])||s.schema!=='finance.customer-receipt.v1'||!['documentNumber','companyName','customerName','customerCode','date','currency','paymentId','bankReference','journalId'].every(k=>text(s[k]))||!amount(s.amountMinor)||Number(s.amountMinor)!==r.amount_minor||s.paymentId!==r.payment_id||s.currency!==r.currency||s.documentNumber!==r.document_number||s.date!==r.receipt_date||!integer(s.paymentVersion,1)||!integer(r.payment_version,1)||s.paymentVersion>r.payment_version||!Array.isArray(s.allocations)||s.allocations.length>1000||s.allocations.some(a=>!exact(a,['invoiceId','invoiceNumber','amountMinor'])||!text(a.invoiceId)||!text(a.invoiceNumber)||!amount(a.amountMinor)))return invalid();
 if(c.holdingId!==holding.holdingId||c.companyId!==input.expected.companyId||c.assignmentId!==input.expected.assignmentId||c.assignmentVersion!==input.expected.assignmentVersion||c.revision!==input.expected.revision||!integer(c.expiresAt,1)||c.expiresAt<=Date.now())return invalid();
 return value as unknown as Reply;
}
const sameSet=(a:readonly string[],b:unknown)=>Array.isArray(b)&&b.every(x=>typeof x==='string')&&new Set(a).size===new Set(b).size&&a.every(x=>b.includes(x));
export function createNativeServiceCustomerReceiptReader(raw:string,transport?:NativeFinanceReadTransport){
 const config=nativeFinanceConfiguration(raw),owner=createNativeServiceCustomerReceiptRead(config,transport,runtime,'/runtime'),read=createNativeServiceCustomerReceiptRead(config,transport,workspace);
 return async(actor:NativeJournalPageContext,incoming:Pick<Headers,'get'>,holdingSlug:string,id:string):Promise<Reply|undefined>=>{
  if(!actor.authority||!actor.iam||actor.holdingSlug!==holdingSlug)throw new NativeFinanceReadError('NATIVE_FINANCE_CONTEXT_CHANGED');
  const branchSlug=actor.scope.kind==='holding'?undefined:actor.scope.branchSlug,departmentSlug=actor.scope.kind==='department'?actor.scope.departmentSlug:undefined;
  const input={holdingSlug,journalId:id,branchSlug,departmentSlug,expected:{companyId:actor.companyId,assignmentId:actor.assignmentId,revision:actor.authority.revision,assignmentVersion:actor.authority.assignmentVersion}};
  if(await owner(incoming,input)==='legacy')return undefined;
  const result=await read(incoming,input),c=result.context;
  const expected={userId:actor.userId,holdingId:actor.holdingId,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,branchSlug:branchSlug??null,departmentSlug:departmentSlug??null,assignmentId:actor.assignmentId,assignmentVersion:actor.authority.assignmentVersion,revision:actor.authority.revision,role:actor.role,assignmentScopeKind:actor.assignmentScope};
  if(Object.entries(expected).some(([k,v])=>c[k as keyof FinanceRenderedWorkContext]!==v)||!sameSet(actor.modules,c.modules)||!sameSet(actor.iam.permissions,c.permissions))throw new NativeFinanceReadError('NATIVE_FINANCE_CONTEXT_CHANGED');
  return result;
 };
}
export const nativeServiceCustomerReceipt=cache(async(actor:NativeJournalPageContext,holdingSlug:string,id:string)=>{
 const raw=process.env.ERPX_NATIVE_FINANCE_SERVICE_PAYMENT_CONFIG;
 if(raw===undefined)throw new NativeFinanceReadError('NATIVE_FINANCE_CONFIGURATION_REQUIRED');
 return createNativeServiceCustomerReceiptReader(raw)(actor,await headers(),holdingSlug,id);
});
export async function nativeServiceCustomerReceiptReadback(request:Request,actor:NativeJournalPageContext,holdingSlug:string,id:string,exportCopy:boolean):Promise<Response|undefined>{
 try{
  const raw=process.env.ERPX_NATIVE_FINANCE_SERVICE_PAYMENT_CONFIG;
  if(raw===undefined)throw new NativeFinanceReadError('NATIVE_FINANCE_CONFIGURATION_REQUIRED');
  const result=await createNativeServiceCustomerReceiptReader(raw)(actor,request.headers,holdingSlug,id);
  if(!result)return undefined;
  if(!exportCopy)return Response.json({data:result.data},{headers:{'Cache-Control':'private, no-store'}});
  return new Response(result.export.content,{headers:{'Cache-Control':'private, no-store','Content-Type':'text/html; charset=utf-8','Content-Security-Policy':"default-src 'none'; style-src 'unsafe-inline'",'X-Content-Type-Options':'nosniff','X-Document-SHA256':result.export.sha256}});
 }catch(error){
  const status=error instanceof NativeFinanceReadError&&[401,403,404,409,422].includes(error.status??0)?error.status!:error instanceof NativeFinanceReadError&&error.code==='NATIVE_FINANCE_CONTEXT_CHANGED'?409:503;
  return Response.json({error:{code:status===404?'RECEIPT_NOT_FOUND':error instanceof NativeFinanceReadError?error.code:'NATIVE_FINANCE_UNAVAILABLE',message:'The issued receipt is currently unavailable.'}},{status,headers:{'Cache-Control':'private, no-store'}});
 }
}
