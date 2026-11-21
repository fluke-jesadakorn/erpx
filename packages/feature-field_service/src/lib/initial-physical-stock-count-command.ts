import {workFetch,type WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {retainWorkCommandCompletion} from '@erpx/feature-work_projection/lib/command-completion';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
import {countCommandContext,countFormUnchanged} from '@erpx/feature-inventory/lib/count-command-fetch';
import {contextGeneration,contextIsSwitching} from '@erpx/feature-work_projection/lib/context-switch';
import type {ReserveContext} from './native-reserve-command';
import type {ServiceInitialPhysicalCountAction,ServiceInitialPhysicalCountDecision,ServiceInitialPhysicalCountInput,ServiceInitialPhysicalCountProductInput,ServiceInitialPhysicalCountSelection,ServiceInitialPhysicalCountView,ServiceInitialPhysicalCountWarehouseInput} from './initial-physical-stock-count-model';
export type {ServiceInitialPhysicalCountAction,ServiceInitialPhysicalCountDecision,ServiceInitialPhysicalCountInput,ServiceInitialPhysicalCountProductInput,ServiceInitialPhysicalCountSelection,ServiceInitialPhysicalCountView,ServiceInitialPhysicalCountWarehouseInput} from './initial-physical-stock-count-model';
export type ServiceInitialPhysicalCountSource={base:string;companyId:string;recordId:string|null;contextKey:string|null;recordVersion:number|null;flowVersion:number|null;nativeCount?:boolean;nativeContext?:ReserveContext};
export type ServiceInitialPhysicalCountReceipt={id:string;status?:'draft'|'submitted'|'approved'|'posted'|'rejected'|'cancelled';version:number;journalId?:string|null};
const owner=(value:ServiceInitialPhysicalCountSource)=>JSON.stringify([value.base,value.companyId,value.recordId]);
const sameNative=(a:ServiceInitialPhysicalCountSource,b:ServiceInitialPhysicalCountSource)=>!!a.nativeCount===!!b.nativeCount&&JSON.stringify(a.nativeContext??null)===JSON.stringify(b.nativeContext??null);
const row=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const statuses={submit:'submitted',approve:'approved',post:'posted',reject:'rejected',cancel:'cancelled'} as const;
export type ServiceInitialPhysicalCountFlowInput={stepKey:string;userId:string;expectedVersion:number;recordVersion:number;reason:string;saveDefault:false};
type Request={kind:'flow';id:string;input:ServiceInitialPhysicalCountFlowInput}|{kind:'prepare';input:ServiceInitialPhysicalCountInput}|{kind:ServiceInitialPhysicalCountAction;id:string;input:ServiceInitialPhysicalCountDecision}|{kind:'warehouse';input:ServiceInitialPhysicalCountWarehouseInput}|{kind:'product';input:ServiceInitialPhysicalCountProductInput};
type Attempt={request:Request;source:ServiceInitialPhysicalCountSource;body:string;key:string;work:string;uncertain:boolean;hadCompletion:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>;nativeOwner?:string;busy:boolean};
const retainedNative=new Map<string,Attempt>();
const nativeOwner=(source:ServiceInitialPhysicalCountSource,work:string)=>JSON.stringify([owner(source),work,source.nativeContext??null]);
function acknowledged(value:unknown,captured:Attempt):value is ServiceInitialPhysicalCountReceipt{
 const request=captured.request;
 if(request.kind==='flow')return row(value)&&value.id===request.id&&value.version===(request.input.expectedVersion||1)+1;
 if(request.kind==='warehouse'||request.kind==='product')return row(value)&&value.id===`${request.kind==='warehouse'?'warehouses':'products'}-${captured.key}`&&value.version===1;
 const id=request.kind==='prepare'?`count-${captured.key}`:request.id,status=request.kind==='prepare'?'draft':statuses[request.kind],version=request.kind==='prepare'?1:request.input.expectedVersion+1;
 return row(value)&&value.id===id&&value.status===status&&value.version===version&&(value.journalId===undefined||value.journalId===null||typeof value.journalId==='string'&&value.journalId.length>0);
}
/** This named initial physical-count transport changes no generic Count gate.
 * Complete original JSON, exact receipt, original scope and captured completion
 * precede key release or local saved events; uncertainty retains the first body. */
export function createServiceInitialPhysicalStockCountCommands(current:()=>ServiceInitialPhysicalCountSource|null,send:typeof workFetch=workFetch){
 let attempt:Attempt|null=null,busy=false;
 const pending=()=>{const source=current();return source?.nativeCount?retainedNative.get(nativeOwner(source,countCommandContext()))??(attempt&&!attempt.nativeOwner?attempt:null):attempt?.nativeOwner?null:attempt;};
 const clear=(captured:Attempt)=>{if(captured.nativeOwner&&retainedNative.get(captured.nativeOwner)===captured)retainedNative.delete(captured.nativeOwner);if(attempt===captured)attempt=null;};
 async function run(captured:Attempt){
  if(busy||captured.busy)throw Error('Wait for the original physical-count request.');
  const source=current();if(!source||owner(source)!==owner(captured.source)||!sameNative(source,captured.source)||countCommandContext()!==captured.work)throw Error('Return to the original Company, physical count and work role to confirm this request.');
  const before={...source},wasUncertain=captured.uncertain;let sent=false;busy=true;captured.busy=true;
  try{
   const assertWork=captureWorkCommandContext(),generation=contextGeneration(),assertCurrent=()=>{assertWork();const live=current();if(contextIsSwitching()||generation!==contextGeneration()||captured.nativeOwner&&retainedNative.get(captured.nativeOwner)!==captured||!live||owner(live)!==owner(before)||!sameNative(live,before)||live.contextKey!==before.contextKey||live.recordVersion!==before.recordVersion||live.flowVersion!==before.flowVersion||countCommandContext()!==captured.work)throw Object.assign(Error('Your physical-count context changed. The original body, key and draft are retained.'),{code:'WORK_CONTEXT_CHANGED'});};
   assertCurrent();const request=captured.request,resource=request.kind==='warehouse'?'field-service/parts-setup/masters/warehouses':request.kind==='product'?'field-service/parts-setup/masters/products':`field-service/initial-physical-stock${request.kind==='prepare'?'':`/${encodeURIComponent(request.id)}/${request.kind}`}`;
   const commandHeaders=new Headers({'Content-Type':'application/json','Idempotency-Key':captured.key});if(captured.source.nativeCount&&request.kind!=='warehouse'&&request.kind!=='product'){const context=captured.source.nativeContext,node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');if(!context||!node||node.dataset.workUser!==context.userId||node.dataset.workCompany!==context.companyId||node.dataset.workAssignment!==context.assignmentId||node.dataset.workRevision!==String(context.revision)||node.dataset.workAssignmentVersion!==String(context.assignmentVersion))throw Object.assign(Error('Your physical-count context changed. The original body, key and draft are retained.'),{code:'WORK_CONTEXT_CHANGED'});for(const [name,value] of [['x-erpx-context-company',context.companyId],['x-erpx-context-assignment',context.assignmentId],['x-erpx-context-revision',String(context.revision)],['x-erpx-context-assignment-version',String(context.assignmentVersion)]] as const)commandHeaders.set(name,value);}
   sent=true;const response=await send(`/api/v1${captured.source.base}/${resource}`,{method:'POST',headers:commandHeaders,body:captured.body},captured.completion(),'caller'),body=await response.json() as {data?:unknown;error?:{code?:string;message?:string}};
   assertCurrent();
   if(!response.ok){if(!wasUncertain&&response.status<500&&body.error?.code!=='WORK_CONTEXT_CHANGED')clear(captured);throw Object.assign(Error(body.error?.message??'The physical-count result is uncertain. Retry its original request.'),{code:body.error?.code});}
   if(!acknowledged(body.data,captured))throw Error('The original physical-count receipt is incomplete. Confirm the same request.');
   captured.uncertain=true;const receipt=body.data;
   return {kind:captured.request.kind,receipt,unchanged:()=>{const completion=captured.completion();return captured.hadCompletion?!!completion&&countFormUnchanged(completion):true;},acknowledge(){assertCurrent();if(captured.nativeOwner?retainedNative.get(captured.nativeOwner)!==captured:attempt!==captured)return false;clear(captured);const completion=captured.completion();if(completion&&typeof window!=='undefined')window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...completion,countAcknowledged:true}}));announceWorkChange();return true;}};
  }catch(cause){if(captured.nativeOwner?retainedNative.get(captured.nativeOwner)===captured:attempt===captured){if(!sent&&!wasUncertain)clear(captured);else captured.uncertain=true;}throw cause;}finally{busy=false;captured.busy=false;}
 }
 function submit(request:Request,completion?:WorkCommandCompletion){
  const source=current();if(!source)throw Error('Return to the original physical-count workspace.');if(source.nativeCount&&request.kind!=='warehouse'&&request.kind!=='product'&&!source.nativeContext)throw Object.assign(Error('Your physical-count context changed. The original draft is retained.'),{code:'WORK_CONTEXT_CHANGED'});
  if(request.kind==='prepare'){if(source.recordId!==null||request.input.contextKey!==source.contextKey||!request.input.contextKey||request.input.input.lines.length!==1)throw Error('Explicitly review one actual warehouse, stock product and count date before preparing this initial physical count.');}
  else if(request.kind==='warehouse'||request.kind==='product'){if(source.recordId!==null)throw Error('Return to the initial physical-count preparation page to create its prerequisites.');}
  else if(source.recordId!==request.id)throw Error('This decision does not belong to the current initial physical count.');
  const body=JSON.stringify(request.input),work=countCommandContext(),existing=pending();if(existing&&(existing.body!==body||owner(existing.source)!==owner(source)||!sameNative(existing.source,source)||existing.request.kind!==request.kind||work!==existing.work))throw Error('Confirm the original physical-count request first. Your newer entries are retained.');
  if(!existing&&source.nativeCount&&request.kind!=='warehouse'&&request.kind!=='product'&&retainedNative.size>=64)throw Error('Confirm an original physical-count request before starting another.');attempt=existing??{request:structuredClone(request),source:structuredClone(source),body,key:crypto.randomUUID(),work,uncertain:false,hadCompletion:!!completion,completion:retainWorkCommandCompletion(completion),busy:false,...(source.nativeCount&&request.kind!=='warehouse'&&request.kind!=='product'?{nativeOwner:nativeOwner(source,work)}:{})};if(attempt.nativeOwner)retainedNative.set(attempt.nativeOwner,attempt);return run(attempt);
 }
 return {assign:(id:string,input:ServiceInitialPhysicalCountFlowInput,completion?:WorkCommandCompletion)=>submit({kind:'flow',id,input},completion),createWarehouse:(input:ServiceInitialPhysicalCountWarehouseInput,completion?:WorkCommandCompletion)=>submit({kind:'warehouse',input},completion),createProduct:(input:ServiceInitialPhysicalCountProductInput,completion?:WorkCommandCompletion)=>submit({kind:'product',input},completion),prepare:(input:ServiceInitialPhysicalCountInput,completion?:WorkCommandCompletion)=>submit({kind:'prepare',input},completion),transition:(id:string,action:ServiceInitialPhysicalCountAction,input:ServiceInitialPhysicalCountDecision,completion?:WorkCommandCompletion)=>submit({kind:action,id,input},completion),retry(){const captured=pending();if(!captured)throw Error('There is no original physical-count request to confirm.');attempt=captured;return run(captured);},pending:()=>pending()!==null,pendingRequest:()=>pending()?.request??null};
}

const text=(value:unknown)=>typeof value==='string'&&value.length>0;
const nullableText=(value:unknown)=>value===null||text(value);
const booleans=(value:Record<string,unknown>,keys:string[])=>keys.every(key=>typeof value[key]==='boolean');
export function validServiceInitialPhysicalStockCountView(value:unknown,companyId:string):value is ServiceInitialPhysicalCountView{
 if(!row(value)||value.companyId!==companyId||!booleans(value,['available','canPrepare','canCreateWarehouse','canCreateProduct'])||!nullableText(value.currency)||!nullableText(value.documentCurrency)||!row(value.selection)||!['warehouseId','productId','countDate'].every(key=>nullableText((value.selection as Record<string,unknown>)[key]))||!row(value.readiness)||!booleans(value.readiness,['postingConfigured','varianceAccountAvailable','postingPeriodAvailable','schemaAvailable']))return false;
 if(!Array.isArray(value.branches)||!value.branches.every(item=>row(item)&&['id','name'].every(key=>text(item[key])))||!Array.isArray(value.warehouses)||!value.warehouses.every(item=>row(item)&&['id','name','branchId'].every(key=>text(item[key])))||!Array.isArray(value.products)||!value.products.every(item=>row(item)&&['id','sku','name','unit','trackingMode'].every(key=>text(item[key]))))return false;
 if(value.contextKey!==null&&(typeof value.contextKey!=='string'||!/^[a-f0-9]{64}$/.test(value.contextKey)))return false;
 if(![null,'purchase-required','stock-authority-required','service-posting-required','selection-required','selection-unavailable','variance-account-required','period-required','prior-stock-history','schema-required'].includes(value.unavailableReason as string|null))return false;
 if(!value.available)return value.contextKey===null&&value.record===null&&value.currency===null&&!value.canPrepare&&!value.canCreateWarehouse&&!value.canCreateProduct&&value.branches.length===0&&value.warehouses.length===0&&value.products.length===0;
 if(value.record===null)return !value.canPrepare||typeof value.contextKey==='string'&&['warehouseId','productId','countDate'].every(key=>text((value.selection as Record<string,unknown>)[key]));
 const record=value.record;
 return row(record)&&record.sourceKind==='service-initial-physical-stock-count'&&['id','number','status','countDate','reason','warehouseId','warehouseName','productId','sku','productName','unit','trackingMode','countedQuantity','countedValue'].every(key=>text(record[key]))&&Number.isSafeInteger(record.version)&&Number(record.version)>0&&nullableText(record.unitCost)&&nullableText(record.journalId)&&booleans(record,['canSubmit','canApprove','canPost','canReject','canCancel'])&&Array.isArray(record.lots)&&record.lots.every(item=>row(item)&&['lotCode','serialNumber','manufacturedDate','expiryDate','unitCost'].every(key=>nullableText(item[key]))&&text(item.countedQuantity))&&Array.isArray(record.decisions)&&record.decisions.every(item=>row(item)&&text(item.action)&&nullableText(item.reason)&&text(item.actorName)&&typeof item.createdAt==='number');
}
/** Explicit source review only; matching initial server content never triggers
 * a mount GET. New selection is not authority until the owner returns its key. */
export async function readServiceInitialPhysicalStockCount(current:()=>ServiceInitialPhysicalCountSource|null,selection:ServiceInitialPhysicalCountSelection={},send:typeof workFetch=workFetch,signal?:AbortSignal){
 const source=current();if(!source)throw Error('Return to the original physical-count workspace.');const captured={...source},assertWork=captureWorkCommandContext(),query=new URLSearchParams();
 for(const key of ['warehouseId','productId','countDate'] as const)if(selection[key])query.set(key,selection[key]);
 const resource=source.recordId?`/${encodeURIComponent(source.recordId)}`:query.size?`?${query}`:'';
 const response=await send(`/api/v1${captured.base}/field-service/initial-physical-stock${resource}`,{cache:'no-store',signal}),body=await response.json() as {data?:unknown;error?:{message?:string}};
 assertWork();const live=current();if(signal?.aborted||!live||owner(live)!==owner(captured)||!sameNative(live,captured)||live.contextKey!==captured.contextKey||live.recordVersion!==captured.recordVersion)throw Object.assign(Error('Your physical-count selection changed. Your draft is retained.'),{code:'WORK_CONTEXT_CHANGED'});
 if(!response.ok||!validServiceInitialPhysicalStockCountView(body.data,captured.companyId)||source.recordId&&body.data.record?.id!==source.recordId)throw Error(body.error?.message??'Current physical-count sources could not be loaded.');return body.data;
}
