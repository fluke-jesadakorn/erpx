import {workFetch,type WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {retainWorkCommandCompletion} from '@erpx/feature-work_projection/lib/command-completion';
import {projectPurchased,type ProjectPurchaseControls} from '@erpx/feature-projects/lib/purchase-controls';
import {projectSchema} from '@erpx/server-projects/public/rules/editor-command-inputs';
import {taskSchema,templateSchema} from '@erpx/server-operations/public/rules/editor-command-inputs';
export type ProjectEditorRecord={kind:'project'|'task'|'routine';id:string;version:number;projectId?:string|null;state?:string;lifecycleStatus?:string};
export type ProjectEditorScope={companyId:string;actorId:string;available:boolean;record:ProjectEditorRecord|null};
export type ProjectEditorCurrent={base:string;kind:'project'|'task';scope:ProjectEditorScope|null;record:ProjectEditorRecord|null;projects:readonly {id:string;title:string}[];options:unknown;members?:readonly string[];purchases?:ProjectPurchaseControls;draftVersion:number;dirty:boolean};
export type ProjectEditorReceipt={id:string;version:number;state?:string};
type Operation='project.create'|'project.update'|'task.create'|'task.update'|'task.revise'|'routine.create'|'routine.update'|'routine.clone';
type Native=ProjectEditorRecord&{version:number;projectId:string|null;operation:Operation};
type Attempt={base:string;companyId:string;actorId:string;workOwner:string;editor:ProjectEditorCurrent['kind'];anchor:string|null;path:string;body:string;key:string;native:Native;draftVersion:number;uncertain:boolean;hadForm:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>};
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
function changed(){return Object.assign(Error('Your Company, Project source or work role changed. Return to the original owner and recover the submitted request.'),{code:'WORK_CONTEXT_CHANGED'});}
function workOwner(){const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');return node?JSON.stringify([node.dataset.workUser,node.dataset.workCompany,node.dataset.workAssignment,node.dataset.workAssignmentVersion]):'server';}
function stamp(source:ProjectEditorCurrent){return JSON.stringify({...source,draftVersion:undefined,dirty:undefined});}
function nativeRequest(source:ProjectEditorCurrent,path:string,input:unknown,key:string,known:Native|null):Native{
 if(!object(input)||!source.scope)throw Error('The actual Project command source is unavailable.');
 const current=source.scope.record??source.record;
 const record=known&&(!current||current.id!==known.id||known.version>current.version)?known:current;
 if(source.kind==='project'){
  if(!projectSchema.safeParse(input).success||path!==(record?`projects/${encodeURIComponent(record.id)}`:'projects')||record&&(record.kind!=='project'||input.expectedVersion!==record.version))throw Error('Review the current Project fields and native version.');
  return {kind:'project',operation:record?'project.update':'project.create',id:record?.id??`project-${key}`,version:record?Number(input.expectedVersion)+1:1,projectId:null};
 }
 const task=taskSchema.safeParse(input),routine=templateSchema.safeParse(input),projectId=typeof input.projectId==='string'?input.projectId:null;
 if(!projectId&&!record?.projectId&&!known)throw Error('This closed command requires an actual Project-affiliated source.');
 if(projectId&&!source.projects.some(row=>row.id===projectId)&&record?.projectId!==projectId)throw Error('Choose an actual visible Project source.');
 const taskPath=record?.kind==='task'?`staff-tasks/${encodeURIComponent(record.id)}`:'staff-tasks',routinePath=record?.kind==='routine'?`routine-templates/${encodeURIComponent(record.id)}`:'routine-templates';
 if(task.success&&(path===taskPath||record?.kind==='task'&&path===`${taskPath}/revise`)&&(!record||record.kind==='task'&&input.expectedVersion===record.version))return {kind:'task',operation:path.endsWith('/revise')?'task.revise':record?'task.update':'task.create',id:record?.id??`task-${key}`,version:record?Number(input.expectedVersion)+1:1,state:task.data.startApprovers.length+task.data.completionApprovers.length?'draft':'ready',projectId};
 const clone=record?.kind==='routine'&&path===`${routinePath}/clone`;
 if(routine.success&&(path===routinePath||clone)&&(!record||record.kind==='routine'&&(clone||input.expectedVersion===record.version)))return {kind:'routine',operation:clone?'routine.clone':record?'routine.update':'routine.create',id:clone||!record?`routine-${key}`:record.id,version:clone||!record?1:Number(input.expectedVersion)+1,projectId};
 throw Error('Unsupported native Project editor operation.');
}
// Exact native validation throws before commit, never a status-wide release.
function preEffect(operation:Operation,status:number,code:unknown){
 if(status!==422)return false;
 if(code==='TASK_ROUTE_INVALID')return ['task.create','task.update','task.revise','routine.create','routine.update','routine.clone'].includes(operation);
 if(code==='TASK_REVIEW_REQUIRED')return operation==='task.update'||operation==='task.revise';
 return code==='REVISION_REASON_REQUIRED'&&operation==='task.revise';
}
function valid(native:Native,value:unknown):value is ProjectEditorReceipt{return object(value)&&value.id===native.id&&value.version===native.version&&(native.kind==='task'?value.state===native.state:value.state===undefined)&&Object.keys(value).every(key=>['id','version',...(native.kind==='task'?['state']:[])].includes(key));}
/** Per-editor closed native client. No generic Project/Staff transport defaults change. */
export function createProjectEditorCommands(send:typeof workFetch=workFetch,newKey:()=>string=()=>crypto.randomUUID()){
 let read:()=>ProjectEditorCurrent|null=()=>null,attempt:Attempt|null=null,busy=false,known:{companyId:string;actorId:string;record:Native}|null=null;
 function fence(){const source=read(),work=captureWorkCommandContext();if(!source?.scope)throw changed();work();const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');if(node&&(node.dataset.workUser!==source.scope.actorId||node.dataset.workCompany!==source.scope.companyId))throw changed();const image=stamp(source);return {source,assertCurrent:()=>{work();const now=read();if(!now||stamp(now)!==image)throw changed();}};}
 async function execute(selected:Attempt,replay:boolean){
  if(busy)throw Error('The original Project editor request is still running.');const {source,assertCurrent}=fence(),scope=source.scope!;
  if(source.base!==selected.base||source.kind!==selected.editor||scope.companyId!==selected.companyId||scope.actorId!==selected.actorId||workOwner()!==selected.workOwner)throw changed();
  const currentRecord=scope.record??source.record;if(currentRecord&&currentRecord.id!==selected.anchor&&currentRecord.id!==selected.native.id)throw changed();
  const capability=source.kind==='project'?'projects.wbs':'projects.tasks';
  if(!projectPurchased(source.purchases,capability,replay?'history':'newWork')||!replay&&!scope.available)throw Error('The current Project authority or purchase does not permit this command. The original request is retained.');
  const wasUncertain=selected.uncertain;busy=true;attempt=selected;selected.uncertain=true;
  try{
   const response=await send(`/api/v1${selected.base}/${selected.path}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':selected.key},body:selected.body},selected.completion(),'caller'),envelope:unknown=await response.json();assertCurrent();
   if(!response.ok){const error=object(envelope)&&object(envelope.error)?envelope.error:{};if(!wasUncertain&&preEffect(selected.native.operation,response.status,error.code)){assertCurrent();attempt=null;}throw Error(typeof error.message==='string'?error.message:'Unable to save. Recover the original Project request.');}
   if(!object(envelope)||!valid(selected.native,envelope.data))throw Error('The native Project editor acknowledgement is incomplete. Recover the original request.');
   assertCurrent();let acknowledged=false;
   return {data:envelope.data,assertCurrent,completion:selected.completion,unchanged:()=>{const completion=selected.completion();return read()?.draftVersion===selected.draftVersion&&(!selected.hadForm||!!completion&&completion.form.isConnected&&completion.form.getAttribute('data-erpx-edit-version')===completion.formEditVersion);},acknowledge:()=>{assertCurrent();if(acknowledged||attempt!==selected)return false;acknowledged=true;known={companyId:selected.companyId,actorId:selected.actorId,record:selected.native};attempt=null;return true;}};
  }finally{busy=false;}
 }
 return {bind:(current:()=>ProjectEditorCurrent|null)=>{read=current;},send:(path:string,input:unknown,completion?:WorkCommandCompletion)=>{
  if(attempt||busy)return Promise.reject(Error('Recover the original Project request before submitting another body or record kind. Your newer draft is retained.'));
  const source=read();if(!source?.scope)return Promise.reject(changed());
  try{const key=newKey(),owned=known?.companyId===source.scope.companyId&&known.actorId===source.scope.actorId?known.record:null,native=nativeRequest(source,path,input,key,owned),selected:Attempt={base:source.base,companyId:source.scope.companyId,actorId:source.scope.actorId,workOwner:workOwner(),editor:source.kind,anchor:(source.scope.record??source.record)?.id??null,path,body:JSON.stringify(input),key,native,draftVersion:source.draftVersion,uncertain:false,hadForm:!!completion,completion:retainWorkCommandCompletion(completion)};return execute(selected,false);}catch(cause){return Promise.reject(cause);}
 },retry:()=>attempt?execute(attempt,true):Promise.reject(Error('No original Project editor request needs recovery.')),hasPending:()=>!!attempt,busy:()=>busy};
}
