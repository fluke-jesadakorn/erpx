import type {FinancePurchaseControls} from '@erpx/contracts/finance-purchase-controls';
import {nativeFinancePurchaseControls} from './native-finance-presentation';
import 'server-only';
import {cache} from 'react';
import {headers} from 'next/headers';
import type {FinanceRenderedWorkContext} from '@erpx/contracts/finance-journal-read';
import {createNativeServiceReceiptRead,NativeFinanceReadError,type NativeFinanceReadTransport,type NativeJournalReadInput} from './native-journal-detail';
import {nativeFinanceConfiguration,type NativeJournalPageContext} from './native-journal-page';

type Row=Record<string,string|number|null>;
type Purchase={capability:'finance.service-invoice';settleExisting:boolean;history:boolean;revision:string};
export type NativeServicePaymentWorkspace={payment:Row;allocations:Row[];reversal:Row|null;periods:Row[];invoices:Row[];bankTransactions:Row[];fxAccounts:Row[];sourcePurchase:Purchase};
type FilesMode='enabled'|'read-only'|'disabled';
type NativePaymentView=NativeServicePaymentWorkspace & {nativeFilesMode:FilesMode;nativeSourceOnly:boolean;nativePurchaseControls:FinancePurchaseControls};
type Reply={data:NativeServicePaymentWorkspace;context:FinanceRenderedWorkContext;filesMode:FilesMode;sourceOnly:boolean;purchaseControls:FinancePurchaseControls};
const invalid=():never=>{throw new NativeFinanceReadError('NATIVE_FINANCE_RESPONSE_INVALID');};
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const exact=(v:unknown,keys:readonly string[]):v is Record<string,unknown>=>object(v)&&Object.keys(v).length===keys.length&&keys.every(k=>Object.hasOwn(v,k));
const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0;
const integer=(v:unknown,min=0):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=min;
const row=(v:unknown):v is Row=>object(v)&&Object.values(v).every(x=>x===null||typeof x==='string'||typeof x==='number'&&Number.isSafeInteger(x));
const rows=(v:unknown):v is Row[]=>Array.isArray(v)&&v.length<=10000&&v.every(row);
const sameSet=(a:readonly string[],b:unknown)=>Array.isArray(b)&&b.every(x=>typeof x==='string')&&new Set(a).size===new Set(b).size&&a.every(x=>b.includes(x));
function runtime(value:unknown,_holding:{holdingId:string},input:NativeJournalReadInput):'rust'|'legacy'{
 if(!exact(value,['data'])||!exact(value.data,['id','runtime'])||value.data.id!==input.journalId||!['rust','legacy'].includes(String(value.data.runtime)))return invalid();
 return value.data.runtime as 'rust'|'legacy';
}
function workspace(value:unknown,holding:{holdingId:string},input:NativeJournalReadInput):Reply {
 if(!input.expected)return invalid();
 if(!exact(value,['data','context','filesMode','sourceOnly','purchaseControls'])||typeof value.sourceOnly!=='boolean'||!['enabled','read-only','disabled'].includes(String(value.filesMode))||!exact(value.data,['payment','allocations','reversal','periods','invoices','bankTransactions','fxAccounts','sourcePurchase'])||!object(value.context))return invalid();
 nativeFinancePurchaseControls(value.purchaseControls);
 const d=value.data,p=d.payment,c=value.context,s=d.sourcePurchase;
 if(!row(p)||p.id!==input.journalId||p.company_id!==input.expected?.companyId||!['payment','receipt'].includes(String(p.kind))||!['draft','approved','released','settled','voided'].includes(String(p.status))||!integer(p.version,1)||!integer(p.amount_minor,1)||!integer(p.outstanding_minor)||!text(p.currency)||!text(p.created_by)||!text(p.settlement_invoice_id)||!rows(d.allocations)||!rows(d.periods)||!rows(d.invoices)||!rows(d.bankTransactions)||!rows(d.fxAccounts)||!(d.reversal===null||row(d.reversal)))return invalid();
 if(d.allocations.some(a=>a.invoice_id!==p.settlement_invoice_id)||d.invoices.some(i=>i.id!==p.settlement_invoice_id)||!exact(s,['capability','settleExisting','history','revision'])||!['finance.service-invoice','finance.banking'].includes(String(s.capability))||typeof s.settleExisting!=='boolean'||typeof s.history!=='boolean'||!text(s.revision))return invalid();
 if(c.holdingId!==holding.holdingId||c.companyId!==input.expected?.companyId||c.assignmentId!==input.expected.assignmentId||c.assignmentVersion!==input.expected.assignmentVersion||c.revision!==input.expected.revision||!integer(c.expiresAt,1)||c.expiresAt<=Date.now())return invalid();
 return value as unknown as Reply;
}
/** Only a successful persisted-source legacy verdict returns undefined.
 * Missing configuration, native denial and transport errors never select TS. */
export function createNativeServicePaymentReader(raw:string,transport?:NativeFinanceReadTransport){
 const config=nativeFinanceConfiguration(raw),owner=createNativeServiceReceiptRead(config,transport,runtime,'/runtime'),read=createNativeServiceReceiptRead(config,transport,workspace);
 return async(actor:NativeJournalPageContext,incoming:Pick<Headers,'get'>,holdingSlug:string,id:string):Promise<NativePaymentView|undefined>=>{
  if(!actor.authority||!actor.iam||actor.holdingSlug!==holdingSlug)throw new NativeFinanceReadError('NATIVE_FINANCE_CONTEXT_CHANGED');
  const branchSlug=actor.scope.kind==='holding'?undefined:actor.scope.branchSlug,departmentSlug=actor.scope.kind==='department'?actor.scope.departmentSlug:undefined;
  const input={holdingSlug,journalId:id,branchSlug,departmentSlug,expected:{companyId:actor.companyId,assignmentId:actor.assignmentId,revision:actor.authority.revision,assignmentVersion:actor.authority.assignmentVersion}};
  if(await owner(incoming,input)==='legacy')return undefined;
  const result=await read(incoming,input),c=result.context;
  const expected={userId:actor.userId,holdingId:actor.holdingId,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,branchSlug:branchSlug??null,departmentSlug:departmentSlug??null,assignmentId:actor.assignmentId,assignmentVersion:actor.authority.assignmentVersion,revision:actor.authority.revision,role:actor.role,assignmentScopeKind:actor.assignmentScope};
  if(Object.entries(expected).some(([k,v])=>c[k as keyof FinanceRenderedWorkContext]!==v)||!sameSet(actor.modules,c.modules)||!sameSet(actor.iam.permissions,c.permissions))throw new NativeFinanceReadError('NATIVE_FINANCE_CONTEXT_CHANGED');
  return {...result.data,nativeFilesMode:result.filesMode,nativeSourceOnly:result.sourceOnly,nativePurchaseControls:result.purchaseControls};
 };
}
export const nativeServicePaymentWorkspace=cache(async(actor:NativeJournalPageContext,holdingSlug:string,id:string)=>{
 const raw=process.env.ERPX_NATIVE_FINANCE_SERVICE_PAYMENT_CONFIG;
 if(raw===undefined)throw new NativeFinanceReadError('NATIVE_FINANCE_CONFIGURATION_REQUIRED');
 return createNativeServicePaymentReader(raw)(actor,await headers(),holdingSlug,id);
});

/** The existing REST read uses the same native projection as SSR. Undefined
 * still means only a successful non-Field verdict, never a failed native read. */
export async function nativeServicePaymentReadback(request:Request,actor:NativeJournalPageContext,holdingSlug:string,id:string):Promise<Response|undefined>{
 const raw=process.env.ERPX_NATIVE_FINANCE_SERVICE_PAYMENT_CONFIG;
 try{
  if(raw===undefined)throw new NativeFinanceReadError('NATIVE_FINANCE_CONFIGURATION_REQUIRED',503);
  const data=await createNativeServicePaymentReader(raw)(actor,request.headers,holdingSlug,id);
  if(!data)return undefined;
  const {nativeFilesMode,nativeSourceOnly,nativePurchaseControls,...wireData}=data;
  void nativeFilesMode;void nativeSourceOnly;void nativePurchaseControls;
  return Response.json({data:wireData},{headers:{'Cache-Control':'private, no-store'}});
 }catch(error){
  const status=error instanceof NativeFinanceReadError&&[401,403,404,409,422].includes(error.status??0)?error.status!:error instanceof NativeFinanceReadError&&error.code==='NATIVE_FINANCE_CONTEXT_CHANGED'?409:503;
  return Response.json({error:{code:status===404?'PAYMENT_NOT_FOUND':error instanceof NativeFinanceReadError?error.code:'NATIVE_FINANCE_UNAVAILABLE',message:status===404?'Payment not found in this company.':'Payment details are currently unavailable. Refresh and retry.'}},{status,headers:{'Cache-Control':'private, no-store'}});
 }
}
