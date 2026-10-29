import {workFetch,type WorkCommandCompletion} from "@erpx/feature-work_projection/lib/work-fetch";
import {captureWorkCommandContext} from "@erpx/feature-work_projection/lib/command-context";
import {retainWorkCommandCompletion} from "@erpx/feature-work_projection/lib/command-completion";
import {projectPurchased,projectStructurePurchased,type ProjectPurchaseControls} from "@erpx/feature-projects/lib/purchase-controls";
import type {ProjectCostAllocationRow,ProjectWorkLogRow} from "@erpx/server-projects/public/project-actuals";
import type {ProjectWorkerDisplay} from "@erpx/feature-projects/lib/worker-display";
import type {ProjectGraph,ProjectHandoffView} from "@erpx/server-projects/public/project-structure";

export type ProjectStructureCurrent={base:string;graph:ProjectGraph;handoffs:readonly ProjectHandoffView[];actorId:string;canWriteCosts:boolean;people:readonly ProjectWorkerDisplay[];members:readonly string[];workLogs:readonly ProjectWorkLogRow[];costAllocations:readonly ProjectCostAllocationRow[];purchases?:ProjectPurchaseControls;draftVersion:number};
export type ProjectStructureAcknowledgement=Record<string,unknown>&{id:string;version:number;status:string};
type Attempt={base:string;projectId:string;companyId:string;actorId:string;workOwner:string;body:string;key:string;draftVersion:number;uncertain:boolean;hadForms:number;handoffHash:string|null;completions:ReturnType<typeof retainWorkCommandCompletion>[]};
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==="object"&&!Array.isArray(value);
const actions=new Set(["assign-task","unassign-task","link-actual","prepare-handoff","accept-handoff","reject-handoff","cancel-handoff","reopen"]);
function changed(){return Object.assign(Error("Your Project source, Company or work role changed. Return to this owner and recover the original request; your newer draft is retained."),{code:"WORK_CONTEXT_CHANGED"});}
function workOwner(){const node=typeof document==="undefined"?null:document.querySelector<HTMLElement>("[data-work-revision]");return node?JSON.stringify([node.dataset.workUser,node.dataset.workCompany,node.dataset.workAssignment,node.dataset.workAssignmentVersion]):"server";}
function stamp(source:ProjectStructureCurrent){return JSON.stringify([source.base,source.graph,source.handoffs,source.actorId,source.canWriteCosts,source.people,source.members,source.workLogs,source.costAllocations,source.purchases]);}
function allowed(source:ProjectStructureCurrent,input:Record<string,unknown>,replay:boolean){
 const action=String(input.action),kind=typeof input.sourceKind==="string"?input.sourceKind:undefined;
 if(!actions.has(action))return false;
 const capability=action==="link-actual"?(kind==="cost_allocation"?"projects.costs":"projects.time"):action.includes("handoff")||action==="reopen"?"projects.handover":"projects.wbs";
 if(replay?!projectPurchased(source.purchases,capability,"history"):!projectStructurePurchased(source.purchases,action,kind))return false;
 if(!replay&&input.expectedVersion!==source.graph.project.version)return false;
 if(["assign-task","unassign-task","prepare-handoff"].includes(action))return source.graph.isProjectOwner&&(replay||source.graph.project.lifecycle_status==="active");
 if(action==="link-actual")return (kind==="work_log"?source.graph.isProjectOwner:kind==="cost_allocation"&&source.graph.canReadCosts&&source.canWriteCosts)&&(replay||source.graph.project.lifecycle_status==="active");
 const handoff=source.handoffs.find(row=>row.id===input.handoffId);if(!handoff)return false;
 if(action==="cancel-handoff")return handoff.prepared_by===source.actorId;
 return handoff.recipient_id===source.actorId&&handoff.prepared_by!==source.actorId;
}
function acknowledgement(selected:Attempt,input:Record<string,unknown>,value:unknown):value is ProjectStructureAcknowledgement{
 if(!object(value)||value.id!==selected.projectId||value.version!==Number(input.expectedVersion)+1)return false;
 const action=String(input.action),status=action==="prepare-handoff"?"handoff_pending":action==="accept-handoff"?"completed":"active";
 if(value.status!==status)return false;
 if(action==="assign-task"||action==="unassign-task")return value.taskId===input.taskId&&value.bindingVersion===(action==="assign-task"?Number(input.bindingVersion)+1:0);
 if(action==="link-actual")return value.actualId===`project-package-actual-${selected.key}`;
 if(action==="prepare-handoff")return value.handoffId===`project-handoff-${selected.key}`&&typeof value.snapshotSha256==="string"&&/^[a-f0-9]{64}$/.test(value.snapshotSha256);
 return value.handoffId===input.handoffId&&value.snapshotSha256===selected.handoffHash;
}
/** This closed WBS transport cannot replace an unknown mutation with a newer
 * form. Exact native replay uses fresh history, not the old mutable graph. */
export function createProjectStructureCommands(send:typeof workFetch=workFetch,newKey:()=>string=()=>crypto.randomUUID()){
 let read:()=>ProjectStructureCurrent|null=()=>null,attempt:Attempt|null=null,busy=false;
 function fence(){const captured=read(),work=captureWorkCommandContext();if(!captured)throw changed();work();const node=typeof document==="undefined"?null:document.querySelector<HTMLElement>("[data-work-revision]");if(node&&(node.dataset.workUser!==captured.actorId||node.dataset.workCompany!==captured.graph.project.company_id))throw changed();const source=stamp(captured);return {captured,assertCurrent:()=>{work();const now=read();if(!now||stamp(now)!==source)throw changed();}};}
 async function execute(selected:Attempt,replay:boolean){
  if(busy)throw Error("The original Project request is still running.");
  const {captured,assertCurrent}=fence(),input:unknown=JSON.parse(selected.body);
  if(!object(input)||captured.base!==selected.base||captured.graph.project.id!==selected.projectId||captured.graph.project.company_id!==selected.companyId||captured.actorId!==selected.actorId||selected.workOwner!==workOwner())throw changed();
  if(!allowed(captured,input,replay))throw Error("The current Project role or purchase does not permit this operation. The original request is retained.");
  const wasUncertain=selected.uncertain;busy=true;attempt=selected;selected.uncertain=true;
  try{
   const completions=selected.completions.map(get=>get()).filter((value):value is WorkCommandCompletion=>!!value),response=await send(`/api/v1${selected.base}/projects/${encodeURIComponent(selected.projectId)}/structure`,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":selected.key},body:selected.body},completions[0],"caller");
   const envelope:unknown=await response.json();assertCurrent();
   if(!response.ok){
    const error=object(envelope)&&object(envelope.error)?envelope.error:{};
    // The sole native occurrence is before prepareHandoff/commands.commit in
    // application/structure. A prior unknown can never use this release.
    if(!wasUncertain&&input.action==="prepare-handoff"&&response.status===409&&error.code==="PROJECT_HANDOFF_INCOMPLETE"){assertCurrent();attempt=null;}
    throw Error(typeof error.message==="string"?error.message:"Unable to save the Project. Recover the original request.");
   }
   if(!object(envelope)||!acknowledgement(selected,input,envelope.data))throw Error("The native Project structure acknowledgement is incomplete. Recover the original request.");
   assertCurrent();let acknowledged=false;
   return {data:envelope.data,assertCurrent,completions:()=>selected.completions.map(get=>get()).filter((value):value is WorkCommandCompletion=>!!value),unchanged:()=>{const forms=selected.completions.map(get=>get());return read()?.draftVersion===selected.draftVersion&&forms.length===selected.hadForms&&forms.every(form=>!!form&&form.form.isConnected&&form.form.getAttribute("data-erpx-edit-version")===form.formEditVersion);},acknowledge:()=>{assertCurrent();if(acknowledged||attempt!==selected)return false;acknowledged=true;attempt=null;return true;}};
  }finally{busy=false;}
 }
 return {
  bind:(current:()=>ProjectStructureCurrent|null)=>{read=current;},
  send:(input:unknown,completions:readonly WorkCommandCompletion[]=[])=>{
   if(attempt||busy)return Promise.reject(Error("Recover the original Project request before submitting another action. Your newer draft is retained."));
   const source=read();if(!source) return Promise.reject(changed());
   if(!object(input)||!actions.has(String(input.action))||!Number.isSafeInteger(input.expectedVersion)||Number(input.expectedVersion)<1)return Promise.reject(Error("Review the original Project structure action before submitting."));
   const body=JSON.stringify(input);if(!body)return Promise.reject(Error("Review the Project request before submitting."));
   const selected:Attempt={base:source.base,projectId:source.graph.project.id,companyId:source.graph.project.company_id,actorId:source.actorId,workOwner:workOwner(),body,key:newKey(),draftVersion:source.draftVersion,uncertain:false,hadForms:completions.length,handoffHash:source.handoffs.find(row=>row.id===input.handoffId)?.snapshot_sha256??null,completions:completions.map(completion=>retainWorkCommandCompletion(completion))};return execute(selected,false);
  },
  retry:()=>attempt?execute(attempt,true):Promise.reject(Error("No original Project request needs recovery.")),
  hasPending:()=>!!attempt,busy:()=>busy,pendingAction:()=>attempt?String(JSON.parse(attempt.body).action):null,
 };
}
