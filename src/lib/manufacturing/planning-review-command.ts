import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import type {ManufacturingPurchaseControls} from './purchase-controls';

export type PlanningReviewActor={userId:string;companyId:string;assignmentId:string;branchId:string|null;departmentId:string|null};
export type PlanningReviewRow={id:string;companyId:string;branchId:string|null;scenarioId:string;createdBy:string;version:number;status:string;feasibleDate:string};
export type PlanningReviewRequest={entity:'recommendation';id:string;action:'approve'|'reject';expectedVersion:number;reason:string};
export type PlanningReviewReceipt={id:string;status:'approved'|'rejected';version:number;orderId:string|null};
export type PlanningReviewCurrent={base:string;actor:PlanningReviewActor;rows:readonly Record<string,unknown>[];canReview:boolean;purchase?:ManufacturingPurchaseControls;recordPurchases?:Record<string,ManufacturingPurchaseControls>;blocked:boolean;draftVersion:number};
type Attempt={owner:string;href:string;work:string;row:PlanningReviewRow;body:string;key:string;draftVersion:number;completion:ReturnType<typeof retainWorkCommandCompletion>};
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
export function planningReviewRow(value:unknown):PlanningReviewRow|null{
 if(!object(value)||typeof value.id!=='string'||!value.id||typeof value.company_id!=='string'||!value.company_id||typeof value.scenario_id!=='string'||!value.scenario_id||typeof value.created_by!=='string'||!value.created_by||typeof value.status!=='string'||!(value.branch_id===null||typeof value.branch_id==='string'))return null;
 if(typeof value.version!=='number'&&(typeof value.version!=='string'||!/^\d+$/.test(value.version)))return null;const version=Number(value.version);if(!Number.isSafeInteger(version)||version<1)return null;
 return{id:value.id,companyId:value.company_id,branchId:value.branch_id,scenarioId:value.scenario_id,createdBy:value.created_by,version,status:value.status,feasibleDate:String(value.feasible_date??'')};
}
function changed(){return Object.assign(Error('Return to the original Planning source, Company and selected work revision to recover the original review. Your newer draft is retained.'),{code:'WORK_CONTEXT_CHANGED'});}
const owner=(value:PlanningReviewCurrent)=>JSON.stringify([value.base,value.actor]);
const image=(value:PlanningReviewCurrent)=>JSON.stringify([value.base,value.actor,value.rows,value.canReview,value.purchase,value.recordPurchases,value.blocked]);
function work(value:PlanningReviewCurrent){
 const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');
 if(!node?.isConnected||node.dataset.workStale==='true'||node.dataset.workUser!==value.actor.userId||node.dataset.workCompany!==value.actor.companyId||node.dataset.workAssignment!==value.actor.assignmentId||!node.dataset.workRevision)throw changed();
 return JSON.stringify([node.dataset.workUser,node.dataset.workCompany,node.dataset.workAssignment,node.dataset.workRevision]);
}
/** The original owner returns the recommendation, never the new order as id.
 * Only approval creates mfg-order-<the exact original command key>. */
function receipt(value:unknown,input:PlanningReviewRequest,key:string):value is PlanningReviewReceipt{
 return object(value)&&value.id===input.id&&value.version===input.expectedVersion+1&&value.status===(input.action==='approve'?'approved':'rejected')&&value.orderId===(input.action==='approve'?`mfg-order-${key}`:null);
}
/** One closed recommendation-review owner. No replacement body or key is
 * admitted until its exact native acknowledgement is explicitly consumed. */
export function createPlanningReviewCommands(send:typeof workFetch=workFetch,newKey:()=>string=()=>crypto.randomUUID()){
 let read:()=>PlanningReviewCurrent|null=()=>null,pending:Attempt|null=null,busy=false;
 function captureCurrent(){
  const source=read();if(!source)throw changed();const assertWork=captureWorkCommandContext();assertWork();work(source);
  const sourceImage=image(source),href=window.location.href;
  return()=>{assertWork();const current=read();if(!current||image(current)!==sourceImage||window.location.href!==href)throw changed();work(current);};
 }
 function visible(){const current=read();return !!pending&&!!current&&owner(current)===pending.owner;}
 async function execute(selected:Attempt,replay:boolean){
  if(busy)throw Error('The original Planning review is still running.');
  const current=read(),assertCurrent=captureCurrent();if(!current||owner(current)!==selected.owner||window.location.href!==selected.href||work(current)!==selected.work)throw changed();
  const input=JSON.parse(selected.body) as PlanningReviewRequest,row=current.rows.map(planningReviewRow).find(value=>value?.id===selected.row.id),purchase=current.recordPurchases?.[selected.row.id]??current.purchase;
  if(!row||row.companyId!==current.actor.companyId||row.companyId!==selected.row.companyId||row.branchId!==selected.row.branchId||row.scenarioId!==selected.row.scenarioId||current.actor.branchId!==null&&row.branchId!==current.actor.branchId)throw changed();
  if(current.blocked||!current.canReview||(!replay&&(row.createdBy===current.actor.userId||row.status!=='proposed'||row.version!==input.expectedVersion))||(replay?purchase?.history===false:purchase?.[input.action==='approve'?'newWork':'settleExisting']===false))throw Error('The current Planning role, purchase or source does not permit this review. The original request is retained.');
  assertCurrent();busy=true;pending=selected;
  try{
   const response=await send(`/api/v1${current.base}/production`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':selected.key},body:selected.body},selected.completion(),'caller');
   const original:unknown=await response.json();assertCurrent();
   // Generic command recovery may report a typed4xx after an ambiguous COMMIT
   // acknowledgement. This owner has no status-wide first-response release.
   if(!response.ok){const error=object(original)&&object(original.error)?original.error:{};throw Error(typeof error.message==='string'?error.message:'Recover the original Planning review before changing its fields.');}
   if(!object(original)||!receipt(original.data,input,selected.key))throw Error('The native Planning review acknowledgement is incomplete. Retry the original request.');
   const data=original.data;let consumed=false;
   return{data,assertCurrent,completion:selected.completion,unchanged:()=>{const completion=selected.completion();return read()?.draftVersion===selected.draftVersion&&(!completion||completion.form.isConnected&&completion.form.getAttribute('data-erpx-edit-version')===completion.formEditVersion);},acknowledge:()=>{assertCurrent();if(consumed||pending!==selected)return false;consumed=true;pending=null;return true;}};
  }finally{busy=false;}
 }
 return{
  bind:(source:()=>PlanningReviewCurrent|null)=>{read=source;},captureCurrent,
  send:(input:PlanningReviewRequest,completion?:WorkCommandCompletion)=>{
   if(pending||busy)return Promise.reject(Error('Recover the original Planning review before submitting another action. Your newer draft is retained.'));
   const current=read();if(!current)return Promise.reject(changed());
   if(input.entity!=='recommendation'||!['approve','reject'].includes(input.action)||typeof input.id!=='string'||!input.id||!Number.isSafeInteger(input.expectedVersion)||input.expectedVersion<1||typeof input.reason!=='string'||input.reason.trim().length<3||input.reason.trim().length>3000)return Promise.reject(Error('Review the recommendation, current version and an explicit reason before submitting.'));
   const row=current.rows.map(planningReviewRow).find(value=>value?.id===input.id);if(!row)return Promise.reject(changed());
   const body=JSON.stringify({entity:'recommendation',id:input.id,action:input.action,expectedVersion:input.expectedVersion,reason:input.reason});
   return execute({owner:owner(current),href:window.location.href,work:work(current),row,body,key:newKey(),draftVersion:current.draftVersion,completion:retainWorkCommandCompletion(completion)},false);
  },
  retry:()=>pending?execute(pending,true):Promise.reject(Error('No original Planning review needs recovery.')),
  hasPending:()=>!!pending,busy:()=>busy,pendingForCurrent:()=>visible()?{request:JSON.parse(pending!.body) as PlanningReviewRequest,row:pending!.row}:null,
 };
}
