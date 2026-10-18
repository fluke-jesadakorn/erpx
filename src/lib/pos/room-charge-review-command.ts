import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import {announceWorkChange} from '@/lib/work_projection/work-refresh';
import {validPosWorkspaceReceipt} from './workspace-command';
import type {roomChargeReversalQueue} from '@/server/modules/pos/public/room-charge-reversal-queue';

export type RoomChargeReviewRow=Awaited<ReturnType<typeof roomChargeReversalQueue>>[number];
export type RoomChargeReviewOwner={userId:string;companyId:string;property?:{id:string;companyId:string;branchId:string;version:number}};
export type RoomChargeReviewSource={base:string;propertyId?:string;owner:RoomChargeReviewOwner;rows:readonly RoomChargeReviewRow[]};
export type RoomChargeDecisionSource={id:string;number:string;companyId:string;branchId:string;saleId:string;propertyId:string;version:number;saleVersion:number;propertyVersion:number;amountMinor:number;restockLineIds:string[];state:'requested'|'pos_approved';stage:'pos'|'pms'};
type Owner={base:string;api:string;origin:string;userId:string;companyId:string;assignmentId:string;propertyId:string|null;frontApp:string|null;frontLocation:string|null};
type Operation='room_charge_reversal.approve_pos'|'room_charge_reversal.reject_pos'|'room_charge_reversal.approve_pms'|'room_charge_reversal.reject_pms';
export type RoomChargeReviewPending={version:1;owner:Owner;source:RoomChargeDecisionSource;operation:Operation;body:string;key:string;uncertain:boolean;receipt?:Record<string,unknown>};
type Store=Pick<Storage,'getItem'|'setItem'|'removeItem'>;
type Attempt=RoomChargeReviewPending&{completion:ReturnType<typeof retainWorkCommandCompletion>;hadForm:boolean;fields?:string;restored:boolean};
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0;
const integer=(v:unknown,min=1):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=min;
const same=(a:unknown,b:unknown)=>JSON.stringify(a)===JSON.stringify(b);
const changed=()=>Object.assign(Error('The charge-review owner, hotel or work role changed. Keep the original decision and return to its original context.'),{code:'WORK_CONTEXT_CHANGED'});
const invalidSaved=()=>Error('The saved charge-review decision cannot be safely identified. Its original bytes are retained; do not replace its key or assign it to the current Company.');
export function roomChargeReviewStorageKey(source:Pick<RoomChargeReviewSource,'base'|'propertyId'|'owner'>){return 'erpx-room-charge-review:'+JSON.stringify([source.base,source.owner.userId,source.owner.companyId,source.propertyId??null]);}
function api(base:string){return `/api/v1${base}${base.startsWith('/front/')?'':'/pos'}`;}
function owner(source:RoomChargeReviewSource|null):Owner{
 const work=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]'),front=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-front-app]');
 if(!source||!work?.isConnected||work.dataset.workStale==='true'||!text(work.dataset.workAssignment)||work.dataset.workUser!==source.owner.userId||work.dataset.workCompany!==source.owner.companyId||source.owner.property&&(source.owner.property.id!==source.propertyId||source.owner.property.companyId!==source.owner.companyId))throw changed();
 return {base:source.base,api:api(source.base),origin:window.location.origin,userId:source.owner.userId,companyId:source.owner.companyId,assignmentId:work.dataset.workAssignment,propertyId:source.propertyId??null,frontApp:front?.dataset.frontApp??null,frontLocation:front?.dataset.frontLocation??null};
}
export function roomChargeDecisionSource(row:RoomChargeReviewRow):RoomChargeDecisionSource{
 const lines:unknown=JSON.parse(row.restock_line_ids_json);
 if(!Array.isArray(lines)||lines.some(id=>!text(id)))throw Error('The native charge-review source is incomplete.');
 return {id:row.id,number:row.number,companyId:row.company_id,branchId:row.branch_id,saleId:row.sale_id,propertyId:row.property_id,version:row.version,saleVersion:row.sale_version,propertyVersion:row.property_version,amountMinor:row.amount_minor,restockLineIds:lines,state:row.state,stage:row.stage};
}
function validDecisionSource(value:unknown):value is RoomChargeDecisionSource{return object(value)&&['id','number','companyId','branchId','saleId','propertyId'].every(key=>text(value[key]))&&['version','saleVersion','propertyVersion','amountMinor'].every(key=>integer(value[key]))&&Array.isArray(value.restockLineIds)&&value.restockLineIds.every(text)&&(value.state==='requested'&&value.stage==='pos'||value.state==='pos_approved'&&value.stage==='pms');}
export function parseRoomChargeReviewRows(value:unknown,source:RoomChargeReviewSource):RoomChargeReviewRow[]{
 if(!Array.isArray(value))throw Error('Charge review is unavailable. The current queue was not confirmed.');
 for(const item of value){if(!object(item)||!['id','number','company_id','branch_id','sale_id','property_id','reason','restock_line_ids_json'].every(key=>text(item[key]))||!['version','sale_version','property_version','amount_minor'].every(key=>integer(item[key]))||typeof item.canAct!=='boolean'||typeof item.canApprove!=='boolean'||typeof item.source_current!=='boolean'||item.company_id!==source.owner.companyId||source.propertyId&&item.property_id!==source.propertyId||!(item.state==='requested'&&item.stage==='pos'||item.state==='pos_approved'&&item.stage==='pms'))throw Error('The current charge-review queue does not match this Company and hotel.');let lines:unknown;try{lines=JSON.parse(item.restock_line_ids_json as string);}catch{throw Error('The native charge-review stock source is incomplete.');}if(!Array.isArray(lines)||lines.some(id=>!text(id)))throw Error('The native charge-review stock source is incomplete.');}
 return value as RoomChargeReviewRow[];
}
function validOwner(value:unknown):value is Owner{return object(value)&&['base','api','origin','userId','companyId','assignmentId'].every(key=>text(value[key]))&&(value.propertyId===null||text(value.propertyId))&&(value.frontApp===null||text(value.frontApp))&&(value.frontLocation===null||typeof value.frontLocation==='string');}
function validPending(value:unknown):value is RoomChargeReviewPending{
 if(!object(value)||value.version!==1||!validOwner(value.owner)||!validDecisionSource(value.source)||!text(value.operation)||!text(value.body)||!text(value.key)||typeof value.uncertain!=='boolean'||value.receipt!==undefined&&!object(value.receipt))return false;
 const storedOwner=value.owner,source=value.source,operation=value.operation;
 if(!['room_charge_reversal.approve_','room_charge_reversal.reject_'].some(prefix=>operation===prefix+source.stage)||source.companyId!==storedOwner.companyId||storedOwner.propertyId!==null&&source.propertyId!==storedOwner.propertyId||storedOwner.api!==api(storedOwner.base))return false;
 try{const input:unknown=JSON.parse(value.body);return object(input)&&Object.keys(input).sort().join(',')==='expectedVersion,id,reason'&&input.id===source.id&&input.expectedVersion===source.version&&typeof input.reason==='string'&&input.reason.trim().length>=3&&input.reason.trim().length<=1000;}catch{return false;}
}
function image(form:HTMLFormElement){return JSON.stringify(Array.from(new FormData(form),([key,value])=>[key,typeof value==='string'?value:{name:value.name,size:value.size,type:value.type,lastModified:value.lastModified}]));}
function firstRollback(operation:Operation,status:number,code:unknown){return status===409&&(code==='POS_VERSION_CONFLICT'||operation.startsWith('room_charge_reversal.approve_')&&code==='POS_REVERSAL_STALE');}
/** The original POS transaction remains the receipt/money/stock owner. A queue
 * row disappearing is never an acknowledgement: the exact native receipt is
 * mandatory, including the distinct final PMS entry/cost result with no version. */
export function createRoomChargeReviewCommands(current:()=>RoomChargeReviewSource|null,send:typeof workFetch=workFetch,storage:()=>Store=()=>sessionStorage){
 let attempt:Attempt|null=null,running=false;
 function saved(){const source=current();if(!source)throw changed();const raw=storage().getItem(roomChargeReviewStorageKey(source));if(raw===null)return null;let parsed:unknown;try{parsed=JSON.parse(raw);}catch{throw invalidSaved();}if(!validPending(parsed))throw invalidSaved();return parsed;}
 function persist(value:Attempt){const source=current();if(!source)throw changed();const prior=saved();if(prior&&(prior.key!==value.key||prior.body!==value.body||!same(prior.owner,value.owner)||!same(prior.source,value.source)||prior.receipt!==undefined&&!same(prior.receipt,value.receipt)))throw invalidSaved();const {completion:_completion,hadForm:_hadForm,fields:_fields,restored:_restored,...durable}=value;void _completion;void _hadForm;void _fields;void _restored;storage().setItem(roomChargeReviewStorageKey(source),JSON.stringify(durable));}
 function pending(){const prior=saved();if(prior&&attempt&&(prior.key!==attempt.key||prior.body!==attempt.body||!same(prior.owner,attempt.owner)||!same(prior.source,attempt.source)))throw invalidSaved();return prior;}
 function fence(){const source=current();if(!source)throw changed();const nativeOwner=owner(source),assertWork=captureWorkCommandContext(),stamp=JSON.stringify(source),path=window.location.pathname,query=window.location.search,front=document.querySelector('[data-front-app]');assertWork();return{source,nativeOwner,assertCurrent(){assertWork();if(!current()||JSON.stringify(current())!==stamp||!same(owner(current()),nativeOwner)||window.location.pathname!==path||window.location.search!==query||front&&(front.isConnected===false||document.querySelector('[data-front-app]')!==front))throw changed();}};}
 async function read(){const scope=fence(),response=await send(`${scope.nativeOwner.api}/room-charge-reversals?${new URLSearchParams(scope.source.propertyId?{property:scope.source.propertyId}:{})}`,{cache:'no-store'}),body:unknown=await response.json();scope.assertCurrent();if(!response.ok||!object(body))throw Error(object(body)&&object(body.error)&&typeof body.error.message==='string'?body.error.message:'Charge review is unavailable.');return {rows:parseRoomChargeReviewRows(body.data,scope.source),assertCurrent:scope.assertCurrent};}
 async function execute(source:RoomChargeDecisionSource,reject:boolean,reason:string,form?:HTMLFormElement){
  if(running)throw Error('Wait for the original charge-review request.');const scope=fence(),operation:Operation=source.stage==='pos'?(reject?'room_charge_reversal.reject_pos':'room_charge_reversal.approve_pos'):(reject?'room_charge_reversal.reject_pms':'room_charge_reversal.approve_pms'),body=JSON.stringify({id:source.id,expectedVersion:source.version,reason});
  const prior=saved();if(prior&&!attempt)attempt={...prior,uncertain:true,completion:()=>undefined,hadForm:false,restored:true};
  if(attempt&&(attempt.operation!==operation||attempt.body!==body||!same(attempt.owner,scope.nativeOwner)||!same(attempt.source,source)))throw Error('Recover the original charge-review decision before submitting another reason, record or stage. Your newer draft is retained.');
  if(!attempt){const actual=scope.source.rows.find(row=>row.id===source.id&&row.stage===source.stage);if(!actual||!same(roomChargeDecisionSource(actual),source)||!(reject?actual.canAct:actual.canApprove)||source.companyId!==scope.nativeOwner.companyId)throw Error('Refresh and select the current permitted charge-review request.');if(reason.trim().length<3||reason.trim().length>1000)throw Error('Enter the native review reason of 3–1000 characters.');const completion:WorkCommandCompletion|undefined=form?{form,formEditVersion:form.getAttribute('data-erpx-edit-version')}:undefined;attempt={version:1,owner:scope.nativeOwner,source,operation,body,key:crypto.randomUUID(),uncertain:false,completion:retainWorkCommandCompletion(completion),hadForm:!!form,fields:form?image(form):undefined,restored:false};}
  const selected=attempt,wasUncertain=selected.uncertain;scope.assertCurrent();persist(selected);selected.uncertain=true;persist(selected);running=true;
  try{
   const response=await send(`${scope.nativeOwner.api}/${selected.operation.replace('.','/')}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':selected.key},body:selected.body},selected.completion(),'caller'),parsed:unknown=await response.json();scope.assertCurrent();
   if(!response.ok){const error=object(parsed)&&object(parsed.error)?parsed.error:null;if(!wasUncertain&&!selected.receipt&&firstRollback(selected.operation,response.status,error?.code)){const saved=pending();if(!saved||saved.key!==selected.key||saved.body!==selected.body)throw invalidSaved();scope.assertCurrent();storage().removeItem(roomChargeReviewStorageKey(scope.source));attempt=null;}throw Object.assign(Error(typeof error?.message==='string'?error.message:'The original review decision was not confirmed.'),{status:response.status,code:error?.code});}
   const input:Record<string,unknown>=JSON.parse(selected.body);if(!object(parsed)||!validPosWorkspaceReceipt(selected.operation,input,scope.nativeOwner.companyId,parsed.data,{saleId:selected.source.saleId}))throw Error('The native charge-review receipt does not match its original request and linked sale.');
   const receipt=parsed.data;if(selected.receipt&&!same(selected.receipt,receipt))throw Error('The original native review receipt changed. Keep its key for investigation.');selected.receipt=receipt;persist(selected);scope.assertCurrent();
   const latest=await read();latest.assertCurrent();scope.assertCurrent();const stillQueued=latest.rows.find(row=>row.id===selected.source.id);if(stillQueued){const now=roomChargeDecisionSource(stillQueued),old=selected.source;if(!same({...now,state:old.state,stage:old.stage,version:old.version},old)||now.version<=old.version)throw Error('The queue contradicts the confirmed original review source. Keep its native receipt.');}
   let consumed=false;return{receipt,rows:latest.rows,assertCurrent:scope.assertCurrent,acknowledge(){scope.assertCurrent();if(consumed||attempt!==selected)return false;const prior=pending();if(!prior||prior.key!==selected.key||prior.body!==selected.body||!same(prior.receipt,receipt)||!same(prior.source,selected.source)||!same(prior.owner,selected.owner))throw invalidSaved();const completion=selected.completion(),matching=!selected.restored&&selected.hadForm&&!!completion&&completion.form.isConnected&&completion.form.getAttribute('data-erpx-edit-version')===completion.formEditVersion&&image(completion.form)===selected.fields;storage().removeItem(roomChargeReviewStorageKey(scope.source));attempt=null;consumed=true;if(matching)window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:completion}));announceWorkChange();return{newer:!matching};}};
  }finally{running=false;}
 }
 async function retry(){const original=pending();if(!original)throw Error('There is no saved charge-review decision.');const input=JSON.parse(original.body) as {reason:string};return execute(original.source,original.operation.includes('.reject_'),input.reason);}
 return {read,execute,retry,pending,busy:()=>running};
}
