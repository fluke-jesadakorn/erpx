import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { approvalView } from "./approvals";
import { ApiError } from "./http";
import { transitionExpense } from "./expenses";
import { approveLeave, transitionLeave } from "./leave";
import { commandOvertime } from "./overtime";
import { actOnEmployeeRequest,canOverrideEmployeeRequest,requestActionSchema,type EmployeeRequest } from "./employee-requests";
import type { WorkItem } from "@/lib/work-item";

const kinds=["expense","leave","overtime","request"] as const;
const inputSchema=z.object({
  kind:z.enum(kinds),id:z.string().min(1).max(180),expectedVersion:z.number().int().positive(),
  outcome:z.enum(["approve","reject"]),reason:z.string().trim().min(3).max(2048),
});
type Kind=typeof kinds[number];
const tables={expense:"finance_expenses",leave:"leave_requests",overtime:"overtime_requests"} as const;

export async function departmentOverrideDetail(actor:WorkContext,kind:Kind,id:string){
  const {write}=await holdingResources(actor.holdingId);
  if(kind==="request"){
    const row=await write.prepare("SELECT * FROM employee_requests WHERE id=? AND company_id=?").bind(id,actor.companyId).first<EmployeeRequest>();
    if(!row || !await canOverrideEmployeeRequest(write,actor,row))throw new ApiError(404,"OVERRIDE_UNAVAILABLE","No current department override is available to this work role.");
    const saved=JSON.parse(row.approval_json) as {currentStep?:number;route?:{steps?:{userId:string}[]};decisions?:{override?:boolean;userId:string;originalApproverUserId?:string;at:number;note?:string}[]};
    const original=saved.route?.steps?.[saved.currentStep??0]?.userId??"Assigned approver";
    const name=await write.prepare("SELECT legal_name FROM hr_people WHERE company_id=? AND iam_same_person(user_id,?,company_id) AND merged_into_id IS NULL LIMIT 1").bind(actor.companyId,original).first<{legal_name:string}>();
    return {kind,id,version:row.version,step:(saved.currentStep??0)+1,originalApprover:name?.legal_name??original,
      priorOverrides:(saved.decisions??[]).filter(item=>item.override).map(item=>({actor:item.userId,originalApprover:item.originalApproverUserId??null,at:item.at,reason:item.note}))};
  }
  const approval=await approvalView(write,kind,id,actor);
  if(!approval.canOverride || !approval.currentApprover)throw new ApiError(404,"OVERRIDE_UNAVAILABLE","No current department override is available to this work role.");
  const row=await write.prepare(`SELECT version FROM ${tables[kind]} WHERE id=? AND company_id=?`)
    .bind(id,actor.companyId).first<{version:number}>();
  if(!row)throw new ApiError(404,"REQUEST_NOT_FOUND","Request not found.");
  return {kind,id,version:row.version,step:approval.currentApprover.ordinal+1,
    originalApprover:approval.currentApprover.approver_name??approval.currentApprover.approver_user_id,
    priorOverrides:approval.history.filter(item=>item.action==="override").map(item=>({
      actor:item.actor_name,originalApprover:item.previous_approver_user_id,at:item.created_at,
      ...(kind==="leave"?{}:{reason:item.reason}),
    }))};
}

export async function decideDepartmentOverride(actor:WorkContext,raw:unknown,key:string){
  const input=inputSchema.parse(raw);
  const {write}=await holdingResources(actor.holdingId);
  const receipt=await write.prepare("SELECT operation FROM command_receipts WHERE command_id=?").bind(key).first<{operation:string}>();
  if(!receipt)await departmentOverrideDetail(actor,input.kind,input.id);
  if(input.kind==="request")return actOnEmployeeRequest(write,actor,input.id,requestActionSchema.parse({action:input.outcome==="approve"?"approve":"reject",expectedVersion:input.expectedVersion,note:input.reason,override:true}),key);
  if(input.kind==="expense")return transitionExpense(actor,input.id,input.outcome,{expectedVersion:input.expectedVersion,reason:input.reason,override:true},key);
  if(input.kind==="leave")return input.outcome==="approve"
    ? approveLeave(actor.holdingId,input.id,{expectedVersion:input.expectedVersion,note:input.reason,override:true},key,actor)
    : transitionLeave(actor,input.id,{action:"reject",expectedVersion:input.expectedVersion,note:input.reason,override:true},key);
  const row=await write.prepare("SELECT state FROM overtime_requests WHERE id=? AND company_id=?").bind(input.id,actor.companyId).first<{state:string}>();
  const action=receipt?.operation.startsWith("overtime.") ? receipt.operation.slice("overtime.".length) as "authorize"|"reject"|"return"|"verify" : input.outcome==="reject"?(row?.state==="pending_authorization"?"reject":"return")
    :(row?.state==="pending_authorization"?"authorize":"verify");
  return commandOvertime(actor,input.id,{action,expectedVersion:input.expectedVersion,note:input.reason,override:true,attachmentSetIds:[]},key);
}

/** A superior sees only the pending step, with no leave reason or attachment metadata. */
export async function departmentOverrideWork(actor:WorkContext):Promise<WorkItem[]>{
  const {write}=await holdingResources(actor.holdingId);
  const pending=(await write.prepare(`SELECT i.record_type kind,i.record_id id,i.updated_at
    FROM approval_instances i JOIN approval_steps s ON s.instance_id=i.id AND s.ordinal=i.current_step
    WHERE i.company_id=? AND i.state='pending' AND s.state='pending' AND s.stage='department_approval'
      AND i.record_type IN ('expense','leave','overtime') ORDER BY i.updated_at DESC LIMIT 150`)
    .bind(actor.companyId).all<{kind:Exclude<Kind,"request">;id:string;updated_at:number}>()).results;
  const items:WorkItem[]=[];
  for(const item of pending){
    const view=await approvalView(write,item.kind,item.id,actor);
    if(!view.canOverride || !view.currentApprover)continue;
    items.push({id:item.id,type:item.kind,title:`${item.kind[0].toUpperCase()}${item.kind.slice(1)} · department override`,
      detail:`Current step ${view.currentApprover.ordinal+1}: ${view.currentApprover.approver_name}`,
      href:`/work/department-approvals/${item.kind}/${encodeURIComponent(item.id)}`,
      status:"pending",updatedAt:item.updated_at,queue:"todo",canAct:true,actionKind:"override",
      responsibleQueue:view.currentApprover.approver_name,nextAction:"Review department override",
      relationships:["approval"]});
  }
  const requests=(await write.prepare(`SELECT * FROM employee_requests WHERE company_id=? AND state='awaiting_approval'
    AND approval_json::jsonb->'route'->'steps'->COALESCE((approval_json::jsonb->>'currentStep')::integer,0)->>'stage'='department_approval'
    ORDER BY updated_at DESC LIMIT 150`).bind(actor.companyId).all<EmployeeRequest>()).results;
  for(const row of requests){
    if(!await canOverrideEmployeeRequest(write,actor,row))continue;
    const saved=JSON.parse(row.approval_json) as {currentStep?:number;route?:{steps?:{userId:string}[]}};
    const assigned=saved.route?.steps?.[saved.currentStep??0]?.userId??"Assigned approver";
    items.push({id:row.id,type:row.kind==="purchase"?"purchase_request":"equipment_request",title:"Department request override",
      detail:`Current step ${(saved.currentStep??0)+1}: ${assigned}`,href:`/work/department-approvals/request/${encodeURIComponent(row.id)}`,
      status:"pending",updatedAt:row.updated_at,queue:"todo",canAct:true,actionKind:"override",responsibleQueue:assigned,
      nextAction:"Review department override",relationships:["approval"]});
  }
  return items;
}
