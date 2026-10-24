import "server-only";
import {randomUUID} from "node:crypto";
import type {StaffActor} from "@erpx/server-operations/public/staff-policy";
import type {TaskWorkState} from "@erpx/server-operations/public/staff-tasks";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {projectsPackageEnforcementMode,readPurchasedAccess} from "@erpx/server-commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import {workContextGuards} from '@erpx/server-workflow/public/command-guards';
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {deferProjectOwnerCheck,inProjectOwnerUnit,projectPurchaseControls,requireProjectPurchasedRead,withProjectPurchasedCommand} from "@erpx/server-projects/adapters/purchases";

type TaskSource={id:string;company_id:string;branch_id:string|null;department_id:string|null;creator_user_id:string;assignee_user_id:string;project_id:string|null;template_id:string|null;schedule_id:string|null;state:string;version:number};
const taskCommands=new WeakMap<SqlDatabase,{id:string;userId:string}>();
export function projectTaskOwnsAttachmentAction(database:SqlDatabase,actor:StaffActor,id:string){const command=taskCommands.get(database);return command?.id===id&&command.userId===actor.userId;}
async function taskSource(database:SqlDatabase,actor:StaffActor,id:string,lock=false){
 return database.prepare(`SELECT id,company_id,branch_id,department_id,creator_user_id,assignee_user_id,project_id,template_id,schedule_id,state,version FROM staff_tasks WHERE id=? AND company_id=?${lock?' FOR UPDATE':''}`)
  .bind(id,actor.companyId).first<TaskSource>();
}
/** Actual task.project_id chooses this foundation. Ordinary Work/routine tasks
 * keep their owner; a requested project label never licenses an existing task. */
export async function projectTaskPurchaseAccess(database:SqlDatabase,actor:StaffActor,id:string){
 const mode=await projectsPackageEnforcementMode();if(mode==='disabled')return;
 const row=await taskSource(database,actor,id);if(!row?.project_id)return;
 await requireProjectPurchasedRead(database,actor,row.project_id,'projects.tasks',false);
 const access=await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'projects.tasks'});
 return {newWork:access.newWork,settleExisting:access.settleExisting,history:access.history,revision:access.revision};
}
export async function projectTaskReadScope(database:SqlDatabase,actor:StaffActor){
 const mode=await projectsPackageEnforcementMode();if(mode==='disabled')return {where:'TRUE'};
 await projectPurchaseControls(database,actor);
 return {where:mode==='enforced'?`(t.project_id IS NULL OR EXISTS(WITH project_task_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT 1 FROM staff_projects project_task CROSS JOIN project_task_clock WHERE project_task.id=t.project_id AND project_task.company_id=t.company_id AND ${purchasedCapabilityPredicate('project_task.company_id','projects.tasks','history','project_task_clock.now_ms')}))`:'TRUE'};
}
export async function projectTaskWorkState(database:SqlDatabase,actor:StaffActor,id:string,state:TaskWorkState){
 const access=await projectTaskPurchaseAccess(database,actor,id);if(!access)return state;
 const permittedActions=state.permittedActions.filter(action=>['edit','revise','submit'].includes(action)?access.newWork:access.settleExisting);
 return {...state,permittedActions,blockedReason:state.blockedReason??(permittedActions.length===0&&state.permittedActions.length>0?'This retained Project task is available for history.':null)};
}

/** The complete Operations task/approval/File/receipt effects compose through
 * a named Project unit, including tasks that are deliberately moved out of a
 * project. Final source classification cannot be changed by deferred effects. */
export async function withProjectTaskPurchasedCommand<Result>(owner:SqlDatabase,actor:StaffActor,
 command:{key:string;operations:readonly string[];id:string;purpose:CapabilityPurpose;configure?:boolean;projectId?:string|null;assigneeUserId?:string},run:(database:SqlDatabase)=>Promise<Result>){
 if(await projectsPackageEnforcementMode()==='disabled')return run(owner);
 return inProjectOwnerUnit(owner,actor.companyId,async database=>{
  const before=await taskSource(database,actor,command.id,true),receipt=await database.prepare("SELECT operation FROM command_receipts WHERE command_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?")
   .bind(command.key,actor.userId,actor.companyId).first<{operation:string}>(),replay=!!receipt&&command.operations.includes(receipt.operation);
  const expectedProject=replay?before?.project_id??null:command.configure?command.projectId??null:before?.project_id??null;
  const expectedAssignee=replay||!command.configure?before?.assignee_user_id:command.assigneeUserId;
  const projects=[...new Set([before?.project_id,expectedProject].filter((id):id is string=>!!id))];
  let acknowledgement:Record<string,unknown>|undefined,completedImage:string|null=null;
  const taskImage=()=>database.prepare("SELECT (to_jsonb(t)-'updated_at')::text image FROM staff_tasks t WHERE id=? AND company_id=?").bind(command.id,actor.companyId).first<string>('image');
  const key=`project-task-source:${randomUUID()}`;
  deferProjectOwnerCheck(database,actor.companyId,async()=>{
   const after=await taskSource(database,actor,command.id);
   if(!after||after.project_id!==expectedProject||after.assignee_user_id!==expectedAssignee||before&&(['id','company_id','branch_id','department_id','creator_user_id','template_id','schedule_id'] as const).some(field=>before[field]!==after[field])||!before&&after.creator_user_id!==actor.userId)
    throw new ApiError(409,'PROJECT_TASK_SOURCE_CHANGED','The original Project task workspace or owner changed.');
   if(!replay&&acknowledgement?.id===command.id&&(after.version!==acknowledgement.version||typeof acknowledgement.state==='string'&&after.state!==acknowledgement.state))throw new ApiError(409,'PROJECT_TASK_SOURCE_CHANGED','The completed Project task changed after its owner receipt.');
   if(!completedImage||await taskImage()!==completedImage)throw new ApiError(409,'PROJECT_TASK_SOURCE_CHANGED','The completed Project task changed during deferred effects.');
   await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]);
  });
  const invoke=async(index:number):Promise<Result>=>index===projects.length?run(database):withProjectPurchasedCommand(database,actor,{key:command.key,operations:command.operations,projectId:projects[index],capability:'projects.tasks',purpose:command.purpose,existing:true,member:false},()=>invoke(index+1));
  if(projects.length)taskCommands.set(database,{id:command.id,userId:actor.userId});
  try{const result=await invoke(0);if(result&&typeof result==='object')acknowledgement=result as Record<string,unknown>;completedImage=await taskImage();return result;}finally{taskCommands.delete(database);}
 });
}

/** Named Operations composition keeps Work and actual Project task foundations
 * in the original complete unit; source locks run before its first SELECT. */
export function withOperationsTaskPurchaseUnit<Result>(owner:SqlDatabase,actor:StaffActor,sourceLocks:(database:SqlDatabase)=>Promise<void>,run:(database:SqlDatabase)=>Promise<Result>){return inProjectOwnerUnit(owner,actor.companyId,run,sourceLocks);}
export function deferOperationsTaskPurchaseCheck(database:SqlDatabase,actor:StaffActor,check:()=>Promise<void>){return deferProjectOwnerCheck(database,actor.companyId,check);}
