import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import {projectPurchased,type ProjectPurchaseControls} from './purchase-controls';
import {projectMilestoneSchema,projectMilestoneTransitionSchema,projectWorkLogSchema,projectWorkLogDecisionSchema,projectCostAllocationSchema,projectCostAllocationDecisionSchema,type ProjectMilestoneRow,type ProjectWorkLogRow,type ProjectCostAllocationRow,type ProjectCostSourceRow} from '@/server/modules/projects/public/rules/record-command-inputs';
import type {ProjectWorkerDisplay} from './worker-display';

export type ProjectRecordOwner={companyId:string;actorId:string;projectId:string;version:number;lifecycleStatus:string;isProjectOwner:boolean;canWriteCosts:boolean;canApproveCosts:boolean};
type Common={base:string;projectId:string;owner?:ProjectRecordOwner;purchases?:ProjectPurchaseControls;draftVersion:number;dirty:boolean};
export type ProjectRecordCurrent=Common&({kind:'milestones';milestones:readonly ProjectMilestoneRow[];members:readonly string[];people:readonly ProjectWorkerDisplay[];canManage:boolean}|{kind:'time';workLogs:readonly ProjectWorkLogRow[];editable:boolean}|{kind:'costs';costs:readonly ProjectCostAllocationRow[];costSources:readonly ProjectCostSourceRow[];canReadCosts:boolean;canWriteCosts:boolean;canApproveCosts:boolean;editable:boolean});
export type ProjectRecordAcknowledgement={id:string;version:number;state?:string};
type Operation='milestone-save'|'milestone-transition'|'time-save'|'time-review'|'cost-save'|'cost-review';
type Native={operation:Operation;id:string;version:number;state?:string;creating:boolean};
type Attempt={base:string;projectId:string;companyId:string;actorId:string;kind:ProjectRecordCurrent['kind'];workOwner:string;path:string;body:string;key:string;draftVersion:number;uncertain:boolean;hadForm:boolean;native:Native;completion:ReturnType<typeof retainWorkCommandCompletion>};
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
function changed(){return Object.assign(Error('Your Project source, Company or work role changed. Return to this owner and recover the original request; your newer draft is retained.'),{code:'WORK_CONTEXT_CHANGED'});}
function workOwner(){const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');return node?JSON.stringify([node.dataset.workUser,node.dataset.workCompany,node.dataset.workAssignment]):'server';}
function stamp(source:ProjectRecordCurrent){return JSON.stringify({...source,draftVersion:undefined,dirty:undefined});}
function expected(source:ProjectRecordCurrent,path:string,input:unknown,key:string):Native{
 if(!object(input)||!source.owner||source.owner.projectId!==source.projectId)throw Error('The current native Project source is unavailable.');
 const root=`projects/${encodeURIComponent(source.projectId)}`,owner=source.owner;
 if(source.kind==='milestones'){
  const collection=`${root}/milestones`,row=source.milestones.find(row=>path===`${collection}/${encodeURIComponent(row.id)}`||path===`${collection}/${encodeURIComponent(row.id)}/transition`);
  if(path===collection||row&&path===`${collection}/${encodeURIComponent(row.id)}`){
   if(!projectMilestoneSchema.safeParse(input).success||row&&(row.company_id!==owner.companyId||row.project_id!==source.projectId||row.version!==input.expectedVersion))throw Error('Review the current milestone fields and version before submitting.');
   return {operation:'milestone-save',id:row?.id??`milestone-${key}`,version:row?Number(input.expectedVersion)+1:1,creating:!row};
  }
  if(row&&path===`${collection}/${encodeURIComponent(row.id)}/transition`&&row.company_id===owner.companyId&&row.project_id===source.projectId&&row.version===input.expectedVersion&&projectMilestoneTransitionSchema.safeParse(input).success){return {operation:'milestone-transition',id:row.id,version:Number(input.expectedVersion)+1,state:input.action==='complete'?'completed':input.action==='cancel'?'cancelled':'in_progress',creating:false};}
 }else if(source.kind==='time'){
  const collection=`${root}/work-logs`;
  if(path===collection&&projectWorkLogSchema.safeParse(input).success){
   const row=source.workLogs.find(row=>row.is_actor&&row.work_date===input.workDate);
   if(row&&(row.project_id!==source.projectId||row.state!=='rejected'||row.version!==input.expectedVersion)||!row&&input.expectedVersion!==undefined)throw Error('Review the current native time entry before submitting.');
   return {operation:'time-save',id:row?.id??`project-time-${key}`,version:row?Number(input.expectedVersion)+1:1,state:'submitted',creating:!row};
  }
  const row=source.workLogs.find(row=>path===`${collection}/${encodeURIComponent(row.id)}/review`);
  if(row&&row.project_id===source.projectId&&row.version===input.expectedVersion&&projectWorkLogDecisionSchema.safeParse(input).success)return {operation:'time-review',id:row.id,version:Number(input.expectedVersion)+1,state:String(input.action)==='approve'?'approved':'rejected',creating:false};
 }else{
  const collection='finance/project-cost-allocations';
  if(path===collection&&input.projectId===source.projectId&&projectCostAllocationSchema.safeParse(input).success){
   const row=source.costs.find(row=>row.journal_line_id===input.sourceLineId&&row.is_creator);
   if(row&&(row.project_id!==source.projectId||row.state!=='rejected'||row.version!==input.expectedVersion)||!row&&input.expectedVersion!==undefined)throw Error('Review the current native cost allocation before submitting.');
   if(!source.costSources.some(row=>row.source_line_id===input.sourceLineId))throw Error('Choose an actual posted expense source from this Project scope.');
   return {operation:'cost-save',id:row?.id??`project-cost-${key}`,version:row?Number(input.expectedVersion)+1:1,state:'draft',creating:!row};
  }
  const row=source.costs.find(row=>path===`${collection}/${encodeURIComponent(row.id)}/review`);
  if(row&&row.project_id===source.projectId&&row.version===input.expectedVersion&&projectCostAllocationDecisionSchema.safeParse(input).success)return {operation:'cost-review',id:row.id,version:Number(input.expectedVersion)+1,state:String(input.action)==='approve'?'approved':'rejected',creating:false};
 }
 throw Error('Unsupported native Project record operation.');
}
function allowed(source:ProjectRecordCurrent,selected:Attempt,replay:boolean){
 const owner=source.owner,input:Record<string,unknown>=JSON.parse(selected.body);if(!owner)return false;
 const capability=source.kind==='milestones'?'projects.wbs':source.kind==='time'?'projects.time':'projects.costs',save=selected.native.operation.endsWith('-save'),newWork=save||selected.native.operation==='milestone-transition'&&input.action==='reopen';
 if(!projectPurchased(source.purchases,capability,replay?'history':newWork?'newWork':'settleExisting'))return false;
 if(!replay&&(owner.lifecycleStatus!=='active'||source.kind!=='milestones'&&!source.editable))return false;
 if(source.kind==='milestones')return owner.isProjectOwner&&(replay||source.canManage);
 if(source.kind==='time')return save||owner.isProjectOwner&&(replay||source.workLogs.some(row=>row.id===selected.native.id&&row.can_review&&row.state==='submitted'));
 return source.canReadCosts&&(save?owner.canWriteCosts:owner.canApproveCosts)&&(replay||(save?source.canWriteCosts:source.canApproveCosts&&source.costs.some(row=>row.id===selected.native.id&&row.can_review&&row.state==='draft')));
}
// These exact native errors are raised before commands.commit. SQL constraints
// use a different status for dependency cycles; Work/source/status-wide errors
// are never treated as a proven rollback. A prior unknown is never released.
function preEffect(selected:Attempt,status:number,code:unknown){
 const operation=selected.native.operation;
 return operation==='milestone-save'&&status===422&&['PROJECT_MEMBER_REQUIRED','PROJECT_PACKAGE_PARENT_INVALID','PROJECT_DEPENDENCY_CYCLE','PROJECT_DEPENDENCY_SCOPE_INVALID','PROJECT_DEPENDENCY_CANCELLED'].includes(String(code))
  ||operation==='milestone-transition'&&status===409&&['PROJECT_MILESTONE_TRANSITION_INVALID','PROJECT_DEPENDENCIES_INCOMPLETE','PROJECT_MILESTONE_REQUIRED_PREREQUISITE','PROJECT_MILESTONE_DEPENDENT_COMPLETED'].includes(String(code))
  ||operation==='time-save'&&status===422&&['PROJECT_WORK_DATE_INVALID','PROJECT_TIME_DAILY_LIMIT'].includes(String(code))
  ||operation==='cost-save'&&status===422&&['INVALID_PROJECT_COST','PROJECT_COST_SOURCE_INVALID'].includes(String(code));
}
function validReceipt(native:Native,value:unknown):value is ProjectRecordAcknowledgement{return object(value)&&value.id===native.id&&value.version===native.version&&(native.state===undefined?value.state===undefined:value.state===native.state)&&Object.keys(value).every(key=>['id','version',...(native.state===undefined?[]:['state'])].includes(key));}
/** Closed Milestones/Actuals clients only; original generic callers are unchanged. */
export function createProjectRecordCommands(send:typeof workFetch=workFetch,newKey:()=>string=()=>crypto.randomUUID()){
 let read:()=>ProjectRecordCurrent|null=()=>null,attempt:Attempt|null=null,busy=false;
 function fence(){const captured=read(),work=captureWorkCommandContext();if(!captured?.owner)throw changed();work();const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');if(node&&(node.dataset.workUser!==captured.owner.actorId||node.dataset.workCompany!==captured.owner.companyId))throw changed();const source=stamp(captured);return {captured,assertCurrent:()=>{work();const now=read();if(!now||stamp(now)!==source)throw changed();}};}
 async function execute(selected:Attempt,replay:boolean){
  if(busy)throw Error('The original Project request is still running.');
  const {captured,assertCurrent}=fence(),owner=captured.owner!;
  if(captured.base!==selected.base||captured.projectId!==selected.projectId||owner.projectId!==selected.projectId||owner.companyId!==selected.companyId||owner.actorId!==selected.actorId||captured.kind!==selected.kind||workOwner()!==selected.workOwner)throw changed();
  if(!allowed(captured,selected,replay))throw Error('The current Project role or purchase does not permit this operation. The original request is retained.');
  const wasUncertain=selected.uncertain;busy=true;attempt=selected;selected.uncertain=true;
  try{
   const response=await send(`/api/v1${selected.base}/${selected.path}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':selected.key},body:selected.body},selected.completion(),'caller'),envelope:unknown=await response.json();assertCurrent();
   if(!response.ok){const error=object(envelope)&&object(envelope.error)?envelope.error:{};if(!wasUncertain&&preEffect(selected,response.status,error.code)){assertCurrent();attempt=null;}throw Error(typeof error.message==='string'?error.message:'Unable to save. Recover the original Project request.');}
   if(!object(envelope)||!validReceipt(selected.native,envelope.data))throw Error('The native Project record acknowledgement is incomplete. Recover the original request.');
   assertCurrent();let acknowledged=false;
   return {data:envelope.data,assertCurrent,completion:selected.completion,unchanged:()=>{const form=selected.completion();return read()?.draftVersion===selected.draftVersion&&(!selected.hadForm||!!form&&form.form.isConnected&&form.form.getAttribute('data-erpx-edit-version')===form.formEditVersion);},acknowledge:()=>{assertCurrent();if(acknowledged||attempt!==selected)return false;acknowledged=true;attempt=null;return true;}};
  }finally{busy=false;}
 }
 return {bind:(current:()=>ProjectRecordCurrent|null)=>{read=current;},send:(path:string,input:unknown,completion?:WorkCommandCompletion)=>{
  if(attempt||busy)return Promise.reject(Error('Recover the original Project request first. Your newer draft is retained.'));
  const source=read();if(!source?.owner)return Promise.reject(changed());
  try{const key=newKey(),native=expected(source,path,input,key),selected:Attempt={base:source.base,projectId:source.projectId,companyId:source.owner.companyId,actorId:source.owner.actorId,kind:source.kind,workOwner:workOwner(),path,body:JSON.stringify(input),key,draftVersion:source.draftVersion,uncertain:false,hadForm:!!completion,native,completion:retainWorkCommandCompletion(completion)};return execute(selected,false);}catch(cause){return Promise.reject(cause);}
 },retry:()=>attempt?execute(attempt,true):Promise.reject(Error('No original Project request needs recovery.')),hasPending:()=>!!attempt,busy:()=>busy};
}
