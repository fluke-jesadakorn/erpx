import type {FieldServiceReserveInput,FieldServiceReservationReadResponse,FieldServiceIssueInput,FieldServiceIssueReadResponse,FieldServiceRenderedContext} from '@erpx/contracts/field-service-reserve';
import {holdingFetch} from '@erpx/contracts/holding-fetch';
import {contextGeneration,contextIsSwitching} from '@erpx/feature-work_projection/lib/context-switch';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
import {retainWorkCommandCompletion} from '@erpx/feature-work_projection/lib/command-completion';
import type {WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';
import type {FieldServiceSource,FieldServiceReceipt} from './command';
export type ReserveContext=Omit<FieldServiceRenderedContext,'expiresAt'>;
type IssueInput=Omit<FieldServiceIssueInput,'entryDate'|'allocations'> & {entryDate?:string|null;allocations?:FieldServiceIssueInput['allocations'];warehouseId:string;productId:string};
type Input=(FieldServiceReserveInput & {action:'reserve-part'}) | IssueInput;
type Outcome={receipt:FieldServiceReceipt;agreement:false;completion:ReturnType<typeof retainWorkCommandCompletion>;unchanged:()=>boolean;acknowledge:()=>boolean};
type Attempt={body:string;input:Input;key:string;context:ReserveContext;generation:number;node:HTMLElement;source:FieldServiceSource;completion:ReturnType<typeof retainWorkCommandCompletion>;hadCompletion:boolean;receipt?:FieldServiceReceipt;uncertain:boolean;busy:boolean};
const retained=new Map<string,Attempt>();
const listeners=new Map<string,Set<()=>void>>();
const publish=(key:string)=>{for(const listener of listeners.get(key)??[])listener();};
const record=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const sameSet=(a:readonly string[],b:readonly string[])=>a.length===b.length&&new Set(a).size===new Set(b).size&&a.every(x=>b.includes(x));
const sameContext=(a:ReserveContext,b:ReserveContext)=>Object.entries(a).every(([k,v])=>k==='modules'||k==='permissions'||b[k as keyof ReserveContext]===v)&&sameSet(a.modules,b.modules)&&sameSet(a.permissions,b.permissions);
const matches=(n:HTMLElement,c:ReserveContext)=>JSON.stringify([n.dataset.workUser,n.dataset.workCompany,n.dataset.workAssignment,n.dataset.workRevision,n.dataset.workAssignmentVersion])===JSON.stringify([c.userId,c.companyId,c.assignmentId,String(c.revision),String(c.assignmentVersion)]);
function isInput(v:unknown):v is Input {
 if(!record(v)||typeof v.id!=='string'||typeof v.reason!=='string'||typeof v.expectedVersion!=='number'||!Number.isSafeInteger(v.expectedVersion)||v.expectedVersion<1||v.expectedVersion>=Number.MAX_SAFE_INTEGER||typeof v.quantityMilli!=='number'||!Number.isSafeInteger(v.quantityMilli))return false;
 if(v.action==='reserve-part')return typeof v.warehouseId==='string'&&typeof v.productId==='string'&&typeof v.expiresAt==='number'&&Number.isSafeInteger(v.expiresAt);
 return v.action==='issue-part'&&typeof v.reservationId==='string'&&typeof v.warehouseId==='string'&&typeof v.productId==='string'&&(v.entryDate===undefined||v.entryDate===null||typeof v.entryDate==='string')&&(v.allocations===undefined||Array.isArray(v.allocations)&&v.allocations.every(x=>record(x)&&typeof x.lotId==='string'&&typeof x.quantityMilli==='number'&&Number.isSafeInteger(x.quantityMilli)&&typeof x.balanceVersion==='number'&&Number.isSafeInteger(x.balanceVersion)));
}
function confirmed(value:unknown,attempt:Attempt): value is FieldServiceReservationReadResponse | FieldServiceIssueReadResponse {
 if(!record(value)||!record(value.context)||!record(value.order)||!record(value.receipt)||!record(value.event)||!record(value.reservation)||!record(value.stock)||!attempt.receipt)return false;
 const c=value.context,o=value.order,r=value.receipt,e=value.event,p=value.reservation,s=value.stock;
 if(!Array.isArray(c.modules)||!c.modules.every(x=>typeof x==='string')||!Array.isArray(c.permissions)||!c.permissions.every(x=>typeof x==='string')||!sameContext(attempt.context,c as unknown as ReserveContext))return false;
 const input=attempt.input,receipt=attempt.receipt;
 const common=o.id===input.id&&typeof o.status==='string'&&typeof o.version==='number'&&Number.isSafeInteger(o.version)&&o.version>=receipt.version
  &&r.id===receipt.id&&r.status===receipt.status&&r.version===receipt.version&&e.version===receipt.version&&e.actorId===attempt.context.userId&&e.reason===input.reason.trim().toWellFormed()&&e.status===receipt.status
  &&p.orderId===input.id&&p.warehouseId===input.warehouseId&&p.productId===input.productId&&typeof p.quantityMilli==='number'&&Number.isSafeInteger(p.quantityMilli)&&p.quantityMilli>0
  &&typeof p.consumedMilli==='number'&&Number.isSafeInteger(p.consumedMilli)&&p.consumedMilli>=0&&typeof p.releasedMilli==='number'&&Number.isSafeInteger(p.releasedMilli)&&p.releasedMilli>=0&&p.consumedMilli+p.releasedMilli<=p.quantityMilli
  &&s.warehouseId===input.warehouseId&&s.productId===input.productId&&typeof s.quantityMilli==='number'&&Number.isSafeInteger(s.quantityMilli)&&s.quantityMilli>=0&&typeof s.version==='number'&&Number.isSafeInteger(s.version)&&s.version>=1;
 if(!common)return false;
 if(input.action==='reserve-part')return p.id===`field-part-${attempt.key}`&&p.quantityMilli===input.quantityMilli&&p.expiresAt===input.expiresAt;
 return p.id===input.reservationId&&Number(p.consumedMilli)>=input.quantityMilli&&e.reservationId===input.reservationId&&e.consumptionId===`operational-part-${attempt.key}`&&typeof e.journalId==='string'&&e.journalId.length>0&&e.quantityMilli===input.quantityMilli&&typeof e.costMinor==='number'&&Number.isSafeInteger(e.costMinor)&&e.costMinor>0;

}
const owner=(s:FieldServiceSource)=>JSON.stringify([s.base,s.recordId]);
export function createNativeReserveCommands(current:()=>FieldServiceSource|null,initialContext:ReserveContext|undefined,ownerKey:string){
 let currentContext=initialContext;const context=()=>currentContext;const get=()=>retained.get(ownerKey);
 async function run(attempt:Attempt):Promise<Outcome>{
  if(attempt.busy)throw Error('Wait for the original Service request.');let c=context(),source=current();
  if(!attempt.node.isConnected&&c&&attempt.generation===contextGeneration()&&!contextIsSwitching()&&sameContext(c,attempt.context)){const replacement=document.querySelector<HTMLElement>('[data-work-revision]');if(replacement?.isConnected&&replacement.dataset.workStale!=='true'&&matches(replacement,attempt.context))attempt.node=replacement;}
  const node=attempt.node;const stillCurrent=()=>{c=context();source=current();return !!c&&!!source&&owner(source)===owner(attempt.source)&&sameContext(c,attempt.context)&&attempt.generation===contextGeneration()&&!contextIsSwitching()&&node.isConnected&&node.dataset.workStale!=='true'&&matches(node,attempt.context);};
  if(!stillCurrent())throw Error('Return to the original Service order and work role to confirm the request.');
  attempt.busy=true;const ctx=attempt.context,headers=new Headers({'x-erpx-context-company':ctx.companyId,'x-erpx-context-assignment':ctx.assignmentId,'x-erpx-context-revision':String(ctx.revision),'x-erpx-context-assignment-version':String(ctx.assignmentVersion)});if(ctx.branchSlug)headers.set('x-erpx-workspace-branch',ctx.branchSlug);if(ctx.departmentSlug)headers.set('x-erpx-workspace-department',ctx.departmentSlug);
  try {
   if(!attempt.receipt){const h=new Headers(headers);h.set('content-type','application/json');h.set('idempotency-key',attempt.key);const response=await holdingFetch(`/api/v1${attempt.source.base}/field-service`,{method:'POST',headers:h,body:attempt.body});const result:unknown=await response.json();const data=record(result)?result.data:undefined;const original=attempt.source.targets[`order:${attempt.input.id}`];
    if(response.status!==200){const error=record(result)&&record(result.error)?result.error:undefined;if(!attempt.uncertain&&typeof error?.message==='string'&&((response.status===422&&error.code==='VALIDATION_FAILED')||(response.status===409&&error.code==='CONFLICT'&&error.message==='The record changed, a reference is unavailable, or this value already exists. Refresh and review again.'))&&stillCurrent()){retained.delete(ownerKey);publish(ownerKey);throw Error(error.message);}throw Error(typeof error?.message==='string'?error.message:'Unable to confirm the original parts request. Retry the original request.');}
    if(!record(data)||Object.keys(data).sort().join(',')!=='id,status,version'||data.id!==attempt.input.id||data.version!==attempt.input.expectedVersion+1||data.status!==original?.status)throw Error('Unable to confirm the original parts request. Retry the original request.');
    attempt.receipt={id:data.id as string,status:data.status as string,version:data.version as number};publish(ownerKey);
   }
   if(!stillCurrent())throw Error('Parts request saved; readback is pending.');
   const query=new URLSearchParams(attempt.input.action==='reserve-part'?{nativeReserveOrder:attempt.input.id,nativeReserveId:`field-part-${attempt.key}`}:{nativeIssueOrder:attempt.input.id,nativeIssueKey:attempt.key});
   const response=await holdingFetch(`/api/v1${attempt.source.base}/field-service?${query}`,{headers,cache:'no-store'});const value:unknown=await response.json();
   if(!response.ok||!confirmed(value,attempt)||!stillCurrent())throw Error('Parts request saved; readback is pending. Retry confirmation.');
   const receipt=attempt.receipt;
   return {receipt,agreement:false,completion:attempt.completion,unchanged(){const form=attempt.completion();return attempt.hadCompletion?!!form&&form.form.isConnected&&form.form.getAttribute('data-erpx-edit-version')===form.formEditVersion:true;},acknowledge(){if(!stillCurrent()||retained.get(ownerKey)!==attempt)return false;retained.delete(ownerKey);publish(ownerKey);const form=attempt.completion();if(form&&typeof window!=='undefined')window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...form,fieldServiceAcknowledged:true}}));announceWorkChange();return true;}};
  }catch(e){if(retained.get(ownerKey)===attempt)attempt.uncertain=true;throw attempt.receipt?Error('Parts request saved; readback is pending. Retry confirmation.'):e;}finally{attempt.busy=false;}
 }
 return {submit(input:unknown,completion?:WorkCommandCompletion,displayed?:FieldServiceSource){const source=current(),ctx=context();if(!source||!ctx||!isInput(input))throw Error('Review the original Service order and form.');let attempt=get();if(attempt){if(attempt.body!==JSON.stringify(input))throw Error('Confirm the original Service request first. Your newer entries remain unsent.');return run(attempt);}const currentTarget=source.targets[`order:${input.id}`],displayedTarget=displayed?.targets[`order:${input.id}`];if(!currentTarget||currentTarget.version!==input.expectedVersion||displayed&&JSON.stringify(currentTarget)!==JSON.stringify(displayedTarget))throw Error('Review the current Service source before submitting this draft.');const node=document.querySelector<HTMLElement>('[data-work-revision]');if(!node||node.dataset.workStale==='true'||contextIsSwitching()||!matches(node,ctx))throw Error('Your work role changed. Review this request.');attempt={body:JSON.stringify(input),input:structuredClone(input),key:crypto.randomUUID(),context:{...ctx,modules:[...ctx.modules],permissions:[...ctx.permissions]},generation:contextGeneration(),node,source:structuredClone(source),completion:retainWorkCommandCompletion(completion),hadCompletion:!!completion,uncertain:false,busy:false};retained.set(ownerKey,attempt);publish(ownerKey);return run(attempt);},retry(){const attempt=get();if(!attempt)throw Error('There is no original Service request to confirm.');return run(attempt);},pending:()=>!!get(),saved:()=>!!get()?.receipt,subscribe:(listener:()=>void)=>{let set=listeners.get(ownerKey);if(!set){set=new Set();listeners.set(ownerKey,set);}set.add(listener);return()=>{set.delete(listener);if(!set.size)listeners.delete(ownerKey);};},setContext:(value:ReserveContext|undefined)=>{currentContext=value;}};
}
