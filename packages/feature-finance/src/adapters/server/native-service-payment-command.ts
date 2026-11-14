import 'server-only';
import type {NativeJournalPageContext} from './native-journal-page';
import {nativeFinanceConfiguration} from './native-journal-page';
import {createNativeServiceReceiptRead} from './native-journal-detail';
import {nativeFinanceCommandHttp,nativeFinanceReplyBytes} from './native-journal-post';
const limit=2*1024*1024;
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0;
const closed=(status:number,code:string,message='Could not verify this payment. Your draft is retained.')=>Response.json({error:{code,message}},{status,headers:{'Cache-Control':'private, no-store'}});
type Source={kind:'create'|'bank'|'decision'|'export'|'flow';ids:string[];action?:string};
export function isNativeServicePaymentRequest(parts:readonly string[]){return parts[0]==='payments'||parts[0]==='bank'&&parts[1]==='statements'&&parts[2]==='import';}
function source(parts:readonly string[],input:unknown):Source|undefined {
 const body=object(input)?input:{};
 if(parts[0]==='invoices'&&text(parts[1])&&parts[2]==='flow'&&parts.length===3)return {kind:'flow',ids:[parts[1]]};
 if(parts[0]==='payments'&&parts.length===1)return text(body.settlementInvoiceId)?{kind:'create',ids:[body.settlementInvoiceId]}:undefined;
 if(parts[0]==='bank'&&parts[1]==='statements'&&parts[2]==='import')return text(body.settlementPaymentId)?{kind:'bank',ids:[body.settlementPaymentId]}:undefined;
 if(parts[0]==='payments'&&parts[1]==='export')return Array.isArray(body.paymentIds)&&body.paymentIds.length>0&&body.paymentIds.every(text)?{kind:'export',ids:body.paymentIds}:undefined;
 if(parts[0]==='payments'&&text(parts[1]))return {kind:'decision',ids:[parts[1]],action:parts[2]};
 return undefined;
}
/** IDs are lookup references only. Rust's persisted-source verdict decides
 * transport; the consuming Rust command independently proves current lineage.
 * Unlinked generic Finance creation/import remains outside the Field scope. */
export function createNativeServicePaymentCommands(raw:string,transport:typeof nativeFinanceCommandHttp=nativeFinanceCommandHttp){
 const settings=nativeFinanceConfiguration(raw);
 createNativeServiceReceiptRead(settings,undefined,()=>undefined); // Existing exact loopback/Holding configuration validation.
 return async(request:Request,actor:NativeJournalPageContext,holdingSlug:string,parts:readonly string[],input:unknown):Promise<Response|undefined>=>{
  const selected=source(parts,input);if(!selected)return undefined;
  const host=request.headers.get('host'),target=settings.holdings.find(h=>h.holdingSlug===holdingSlug&&new URL(h.origin).host===host);
  if(!target||target.holdingId!==actor.holdingId||actor.holdingSlug!==holdingSlug||!actor.authority||!actor.iam)return closed(403,'NATIVE_FINANCE_AUTHORITY_INVALID');
  const expected={'x-erpx-context-company':actor.companyId,'x-erpx-context-assignment':actor.assignmentId,'x-erpx-context-revision':String(actor.authority.revision),'x-erpx-context-assignment-version':String(actor.authority.assignmentVersion)};
  if(Object.entries(expected).some(([name,value])=>request.headers.get(name)!==value))return closed(409,'WORK_CONTEXT_CHANGED');
  const origin=request.headers.get('origin');if(origin!==null&&origin!==target.origin)return closed(403,'ORIGIN_FORBIDDEN','Use the application origin.');
  const headers=new Headers({host:host!,accept:'application/json',...expected});
  for(const name of ['cookie','origin','x-erpx-workspace-branch','x-erpx-workspace-department']){const value=request.headers.get(name);if(value!==null){if(Buffer.byteLength(value)>8192)return closed(413,'BODY_TOO_LARGE');headers.set(name,value);}}
  const controller=new AbortController(),cancel=()=>controller.abort();request.signal.addEventListener('abort',cancel,{once:true});if(request.signal.aborted)cancel();const timer=setTimeout(cancel,settings.timeoutMs);
  const segment=(v:string)=>encodeURIComponent(v.toWellFormed());
  const base=`/api/v1/holdings/${segment(holdingSlug)}/finance`;
  async function exchange(path:string,method:'GET'|'POST',body:Uint8Array=new Uint8Array()){
   const response=await transport(new URL(path,settings.privateBaseUrl),headers,body,controller.signal,method);
   if(response.redirected||response.status>=300&&response.status<400||response.headers.get('content-type')?.split(';')[0]?.trim()!=='application/json')throw Error('protocol');
   const value:unknown=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(await nativeFinanceReplyBytes(response.body,limit)));
   if(!object(value))throw Error('protocol');
   return {status:response.status,value};
  }
  function error(reply:{status:number;value:Record<string,unknown>}){
   const e=reply.value.error;if(reply.status<400||reply.status>599||!object(e)||!text(e.code)||e.code.length>100||!text(e.message)||e.message.length>1000)return closed(502,'NATIVE_FINANCE_RESPONSE_INVALID');
   return closed(reply.status,e.code,e.message);
  }
  try {
   let native=false,createPath:'field'|'generic'='field';
   for(const id of selected.ids){
    const path=selected.kind==='create'?`${base}/invoices/${segment(id)}/payment-runtime`:selected.kind==='flow'?`${base}/invoices/${segment(id)}/post-runtime`:`${base}/payments/${segment(id)}/runtime`;
    const reply=await exchange(path,'GET');if(reply.status!==200)return error(reply);
    const d=reply.value.data;const allowPath=selected.kind==='create',expectedKeys=(allowPath?['id','path','runtime']:['id','runtime']).join(',');
    if(!object(d)||Object.keys(d).sort().join(',')!==expectedKeys||d.id!==id.toWellFormed()||!['rust','legacy'].includes(String(d.runtime))||allowPath&&!['field','generic'].includes(String(d.path)))return closed(502,'NATIVE_FINANCE_RESPONSE_INVALID');
    native ||= d.runtime==='rust';if(allowPath&&d.runtime==='rust')createPath=d.path==='generic'?'generic':'field';
   }
   if(!native)return undefined;
   if(selected.kind==='export'||selected.kind==='decision'&&!['approve','release','confirm'].includes(selected.action??''))return closed(503,'NATIVE_SERVICE_PAYMENT_ACTION_UNAVAILABLE','This payment action is currently unavailable. Your draft is retained.');
   let body:Uint8Array;try{body=await nativeFinanceReplyBytes(request.body,limit);}catch(error){if(error instanceof Error&&error.message==='BODY_LIMIT')return closed(413,'BODY_TOO_LARGE','Use a smaller request.');throw error;}
   const key=request.headers.get('idempotency-key');headers.set('content-type','application/json');if(key!==null)headers.set('idempotency-key',key);
   const path=selected.kind==='flow'?`${base}/invoices/${segment(selected.ids[0]!)}/flow`:selected.kind==='create'?`${base}/${createPath==='generic'?'payments/generic':'payments'}`:selected.kind==='bank'?`${base}/bank/statements/import`:`${base}/payments/${segment(selected.ids[0]!)}/${selected.action}`;
   const reply=await exchange(path,'POST',body);if(reply.status>=400)return error(reply);
   const d=reply.value.data,created=selected.kind==='create'||selected.kind==='bank';
   if(reply.status!==(created?201:200)||!object(d))return closed(502,'NATIVE_FINANCE_RESPONSE_INVALID');
   // Creation retains the original server-assigned ID across receipt replays.
   // The command key identifies the request; it does not derive that ID.
   if(selected.kind==='create'&&(!text(d.id)||Buffer.byteLength(d.id)>180||Array.from(d.id).some(char=>char.charCodeAt(0)<=32||'/\\'.includes(char))||d.settlementInvoiceId!==selected.ids[0]!.toWellFormed()||d.status!=='draft'||d.version!==1))return closed(502,'NATIVE_FINANCE_RESPONSE_INVALID');
   if(selected.kind==='bank'&&(d.id!==`statement-${key}`||d.settlementPaymentId!==selected.ids[0]!.toWellFormed()||d.imported!==1))return closed(502,'NATIVE_FINANCE_RESPONSE_INVALID');
   if(selected.kind==='flow'&&(d.id!==selected.ids[0]!.toWellFormed()||!Number.isSafeInteger(d.version)||Number(d.version)<2))return closed(502,'NATIVE_FINANCE_RESPONSE_INVALID');
   if(selected.kind==='decision'&&(d.id!==selected.ids[0]!.toWellFormed()||d.status!==({approve:'approved',release:'released',confirm:'settled'}[selected.action!])||!Number.isSafeInteger(d.version)||Number(d.version)<2))return closed(502,'NATIVE_FINANCE_RESPONSE_INVALID');
   return Response.json(reply.value,{status:reply.status,headers:{'Cache-Control':'private, no-store'}});
  }catch{return closed(503,'NATIVE_FINANCE_UNAVAILABLE');}
  finally{clearTimeout(timer);request.signal.removeEventListener('abort',cancel);}
 };
}
export async function nativeServicePaymentCommand(request:Request,actor:NativeJournalPageContext,holdingSlug:string,parts:readonly string[],input:unknown){
 if(!source(parts,input))return undefined;
 const raw=process.env.ERPX_NATIVE_FINANCE_SERVICE_PAYMENT_CONFIG;if(raw===undefined)return closed(503,'NATIVE_FINANCE_CONFIGURATION_REQUIRED');
 return createNativeServicePaymentCommands(raw)(request,actor,holdingSlug,parts,input);
}

export async function nativeServiceInvoiceFlowCommand(request:Request,actor:NativeJournalPageContext,holdingSlug:string,id:string){
 const raw=process.env.ERPX_NATIVE_FINANCE_SERVICE_INVOICE_CONFIG;
 if(raw===undefined)return closed(503,'NATIVE_FINANCE_CONFIGURATION_REQUIRED');
 return createNativeServicePaymentCommands(raw)(request,actor,holdingSlug,['invoices',id,'flow'],{});
}
