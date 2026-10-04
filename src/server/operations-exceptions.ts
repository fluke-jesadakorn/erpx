import "server-only";
import {z} from "zod";
import type {WorkContext} from "./authorization";
import type {SqlDatabase,SqlStatement} from "./runtime/types";
import type {WorkItem} from "@/lib/work-item";
import {hasPermission} from "@/lib/permissions";
import {hasModuleAccess} from "@/data/module-access";
import {samePerson} from "@/lib/iam";
import {holdingResources} from "./runtime";
import {requirePermission} from "./iam-permissions";
import {assertion,executeCommand,replayCommand} from "./commands";
import {ApiError} from "./http";
import {readSourceRecord,visibleSourceRecord,sourceRecordGuards,sourceRecordInput,sourceRecordKind,type AuthorizedSourceRecord} from "./source-record-policy";

const reason=z.string().trim().min(3).max(4000),version=z.number().int().positive(),id=z.string().min(1).max(180);
const policySchema=z.object({area:z.string().trim().regex(/^[a-z][a-z0-9_-]{1,39}$/),name:z.string().trim().min(3).max(160),sourceKinds:z.array(sourceRecordKind).min(1).max(7),targetMinutes:z.number().int().min(1).max(10080)});
const createSchema=z.object({source:sourceRecordInput.extend({version}),policyId:id,policyVersion:version,title:z.string().trim().min(3).max(160),issue:reason,priority:z.enum(["normal","high","critical"]).default("normal"),ownerId:id});
const actionSchema=z.discriminatedUnion("action",[
 z.object({action:z.enum(["acknowledge","start","reject","approve","close","reopen","cancel"]),expectedVersion:version,reason}),
 z.object({action:z.literal("assign"),expectedVersion:version,ownerId:id,reason}),
 z.object({action:z.literal("prioritize"),expectedVersion:version,priority:z.enum(["normal","high","critical"]),reason}),
 z.object({action:z.literal("escalate"),expectedVersion:version,escalationOwnerId:id,reason}),
 z.object({action:z.literal("resolve"),expectedVersion:version,resolution:reason,reason}),
]);
type Row=Record<string,string|number|null>;
type Exception={id:string;company_id:string;branch_id:string|null;department_id:string|null;area:string;source_kind:AuthorizedSourceRecord["kind"];source_id:string;source_version:number;source_reference_json:string;policy_id:string;policy_snapshot_json:string;title:string;issue:string;priority:string;status:string;owner_id:string;escalation_owner_id:string|null;resolution:string|null;created_by:string;resolved_by:string|null;reviewed_by:string|null;version:number;updated_at:number;due_at:number};
const scope="e.company_id=? AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)",args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
function access(a:WorkContext,write=false){requirePermission(a,write?"records.write":"records.read");if(!hasModuleAccess(a.modules,"operations"))throw new ApiError(403,"MODULE_FORBIDDEN","Operations is unavailable to the selected role.");}
function recipient(db:SqlDatabase,key:string,scope:Pick<Exception,"company_id"|"branch_id"|"department_id">,userId:string,permission="records.write") {
 return assertion(db,key,`recipient-${permission}-${userId}`,`EXISTS(SELECT 1 FROM assignments g JOIN hr_people p ON p.user_id=g.user_id AND p.company_id=g.company_id JOIN hr_engagements h ON h.person_id=p.id AND h.company_id=p.company_id
   WHERE g.company_id=? AND g.user_id=? AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
   AND iam_role_allows(g.role,g.company_id,?) AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=? OR g.scope_kind='department' AND g.scope_node_id=?)
   AND p.status='active' AND p.merged_into_id IS NULL AND h.status='active' AND h.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (h.end_date IS NULL OR h.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))`,[scope.company_id,userId,permission,scope.branch_id,scope.department_id]);
}
export async function draftQueuePolicy(a:WorkContext,raw:unknown,key:string){
 access(a,true);requirePermission(a,"records.review");const input=policySchema.parse(raw),{write}=await holdingResources(a.holdingId),recordId=`queue-policy-${key}`,op="operations.policy.draft",prior=await replayCommand(write,a,key,op,recordId,input);if(prior)return prior;
 const now=Date.now();return executeCommand(write,a,key,op,input,recordId,1,{id:recordId,status:"draft",version:1},[
  write.prepare("SELECT id FROM organization_nodes WHERE id=? AND kind='company' AND status='active' FOR UPDATE").bind(a.companyId),
  assertion(write,key,"company-policy-scope","EXISTS(SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND scope_kind='holding' AND iam_role_allows(role,company_id,'records.review'))",[a.assignmentId,a.userId,a.companyId]),
  write.prepare("INSERT INTO operations_queue_policies(id,company_id,area,name,revision,source_kinds_json,target_minutes,created_by,created_at,updated_at) SELECT ?,?,?,?,COALESCE(max(revision),0)+1,?,?,?,?,? FROM operations_queue_policies WHERE company_id=? AND area=?")
   .bind(recordId,a.companyId,input.area,input.name,JSON.stringify([...new Set(input.sourceKinds)].sort()),input.targetMinutes,a.userId,now,now,a.companyId,input.area),
 ]);
}
export async function reviewQueuePolicy(a:WorkContext,recordId:string,raw:unknown,key:string){
 access(a,true);requirePermission(a,"records.review");const input=z.object({action:z.enum(["publish","reject","retire"]),expectedVersion:version,reason}).parse(raw),{write}=await holdingResources(a.holdingId),op=`operations.policy.${input.action}`,prior=await replayCommand(write,a,key,op,recordId,input);if(prior)return prior;
 const now=Date.now(),status=input.action==="publish"?"published":input.action==="reject"?"rejected":"retired",steps:SqlStatement[]=[
  write.prepare("SELECT id FROM organization_nodes WHERE id=? FOR UPDATE").bind(a.companyId),
  assertion(write,key,"policy-review",`EXISTS(SELECT 1 FROM operations_queue_policies WHERE id=? AND company_id=? AND version=? AND status=? ${input.action!=="retire"?"AND NOT iam_same_person(created_by,?,company_id)":""})`,[recordId,a.companyId,input.expectedVersion,input.action==="retire"?"published":"draft",...(input.action!=="retire"?[a.userId]:[])]),
  assertion(write,key,"review-authority","EXISTS(SELECT 1 FROM assignments WHERE id=? AND company_id=? AND user_id=? AND scope_kind='holding' AND iam_role_allows(role,company_id,'records.review'))",[a.assignmentId,a.companyId,a.userId]),
 ];
 if(input.action==="publish")steps.push(write.prepare("UPDATE operations_queue_policies SET status='retired',version=version+1,updated_at=? WHERE company_id=? AND area=(SELECT area FROM operations_queue_policies WHERE id=?) AND status='published'").bind(now,a.companyId,recordId));
 steps.push(write.prepare("UPDATE operations_queue_policies SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?").bind(status,a.userId,now,recordId));
 return executeCommand(write,a,key,op,input,recordId,input.expectedVersion+1,{id:recordId,status,version:input.expectedVersion+1},steps);
}
export async function operationsWorkspace(a:WorkContext){
 access(a);const {write}=await holdingResources(a.holdingId),[candidates,policies,people]=await Promise.all([
  write.prepare(`SELECT e.* FROM operations_exceptions e WHERE ${scope} ORDER BY e.updated_at DESC LIMIT 200`).bind(...args(a)).all<Exception>(),
  write.prepare("SELECT * FROM operations_queue_policies WHERE company_id=? ORDER BY area,revision DESC").bind(a.companyId).all<Row>(),
  write.prepare("SELECT DISTINCT g.user_id id,COALESCE(p.legal_name,g.user_id) name FROM assignments g JOIN hr_people p ON p.user_id=g.user_id AND p.company_id=g.company_id WHERE g.company_id=? AND iam_assignment_active(g.id,?) AND iam_principal_active(g.user_id,g.company_id,?) AND p.status='active' AND p.merged_into_id IS NULL ORDER BY name").bind(a.companyId,Date.now(),Date.now()).all<Row>(),
 ]),exceptions:(Exception&{source:AuthorizedSourceRecord})[]=[];
 for(const row of candidates.results){const source=await visibleSourceRecord(a,{kind:row.source_kind,id:row.source_id},write);if(source)exceptions.push({...row,source});}
 const active=exceptions.filter(row=>!["closed","cancelled"].includes(row.status));
 return {exceptions,policies:policies.results,people:people.results,actorId:a.userId,canWrite:hasPermission(a,"records.write"),canReview:hasPermission(a,"records.review"),summary:{active:active.length,awaitingReview:active.filter(row=>row.status==="awaiting_review").length,escalated:active.filter(row=>row.status==="escalated").length,overdue:active.filter(row=>row.due_at<Date.now()).length,byArea:Object.fromEntries([...new Set(active.map(row=>String(row.area)))].sort().map(area=>[area,active.filter(row=>row.area===area).length]))}};
}
export async function operationsExceptionDetail(a:WorkContext,recordId:string){
 const data=await operationsWorkspace(a),row=data.exceptions.find(item=>item.id===recordId);if(!row)throw new ApiError(404,"OPERATIONS_EXCEPTION_NOT_FOUND","Exception or source is unavailable in this scope.");
 const {write}=await holdingResources(a.holdingId);return {...data,row,history:(await write.prepare("SELECT * FROM operations_exception_events WHERE exception_id=? ORDER BY version").bind(recordId).all<Row>()).results};
}
export async function openOperationsException(a:WorkContext,raw:unknown,key:string){
 access(a,true);const input=createSchema.parse(raw),{write}=await holdingResources(a.holdingId),recordId=`operations-exception-${key}`,op="operations.exception.open",prior=await replayCommand(write,a,key,op,recordId,input);if(prior){await operationsExceptionDetail(a,recordId);return prior;}
 const source=await readSourceRecord(a,input.source,write),policy=await write.prepare("SELECT * FROM operations_queue_policies WHERE id=? AND company_id=? AND version=? AND status='published'").bind(input.policyId,a.companyId,input.policyVersion).first<Row>();
 if(!policy||!(JSON.parse(String(policy.source_kinds_json)) as string[]).includes(source.kind))throw new ApiError(409,"OPERATIONS_POLICY_UNAVAILABLE","Choose a published policy that permits this source kind.");
 const now=Date.now(),dueAt=now+Number(policy.target_minutes)*60000,snapshot={id:policy.id,area:policy.area,revision:policy.revision,version:policy.version,targetMinutes:policy.target_minutes,sourceKinds:JSON.parse(String(policy.source_kinds_json))},recordScope={company_id:source.companyId,branch_id:source.branchId,department_id:source.departmentId};
 return executeCommand(write,a,key,op,input,recordId,1,{id:recordId,status:"open",version:1},[
  ...await sourceRecordGuards(write,a,key,source),
  write.prepare("SELECT id FROM operations_queue_policies WHERE id=? FOR SHARE").bind(input.policyId),
  assertion(write,key,"policy","EXISTS(SELECT 1 FROM operations_queue_policies WHERE id=? AND company_id=? AND version=? AND status='published')",[input.policyId,a.companyId,input.policyVersion]),
  recipient(write,key,recordScope,input.ownerId),
  write.prepare("INSERT INTO operations_exceptions(id,company_id,branch_id,department_id,area,source_kind,source_id,source_version,source_reference_json,policy_id,policy_snapshot_json,title,issue,priority,owner_id,due_at,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
   .bind(recordId,a.companyId,source.branchId,source.departmentId,String(policy.area),source.kind,source.id,source.version,JSON.stringify(source),String(policy.id),JSON.stringify(snapshot),input.title,input.issue,input.priority,input.ownerId,dueAt,a.userId,now,now),
  write.prepare("INSERT INTO operations_exception_events VALUES(?,?,1,'open',?,?,'open',?,?)").bind(`operations-event-${key}`,recordId,a.userId,input.issue,JSON.stringify({source,policy:snapshot,ownerId:input.ownerId}),now),
  write.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'operational_exception',?,1,'An operational exception needs acknowledgement',?)").bind(`operations-notice-${key}`,a.companyId,input.ownerId,recordId,now),
 ],{companyId:a.companyId,branchId:source.branchId,departmentId:source.departmentId});
}
export async function changeOperationsException(a:WorkContext,recordId:string,raw:unknown,key:string){
 access(a,true);const input=actionSchema.parse(raw),{write}=await holdingResources(a.holdingId),op=`operations.exception.${input.action}`,prior=await replayCommand(write,a,key,op,recordId,input);if(prior){await operationsExceptionDetail(a,recordId);return prior;}
 const {row}=await operationsExceptionDetail(a,recordId),manage=["assign","prioritize","reject","approve","reopen","cancel"].includes(input.action);
 if(manage)requirePermission(a,"records.review");else if(row.owner_id!==a.userId)throw new ApiError(403,"OPERATIONS_OWNER_REQUIRED","The assigned owner performs this step.");
 const allowed:Record<string,string[]>={acknowledge:["open"],start:["acknowledged","escalated"],resolve:["in_progress","escalated"],approve:["awaiting_review"],reject:["awaiting_review"],close:["resolved"],reopen:["closed"],assign:["open","acknowledged","in_progress","escalated","awaiting_review"],prioritize:["open","acknowledged","in_progress","escalated"],escalate:["open","acknowledged","in_progress"],cancel:["open","acknowledged","in_progress","escalated","awaiting_review"]};
 if(!allowed[input.action].includes(row.status))throw new ApiError(409,"OPERATIONS_STATE_CHANGED","Review the current exception state.");
 if(["approve","reject"].includes(input.action)&&(samePerson(a,row.owner_id)||samePerson(a,row.resolved_by)))throw new ApiError(403,"OPERATIONS_INDEPENDENT_REVIEW","Another eligible reviewer must decide on this resolution.");
 const now=Date.now(),status=input.action==="assign"&&row.status==="awaiting_review"?"in_progress":({acknowledge:"acknowledged",start:"in_progress",resolve:"awaiting_review",approve:"resolved",reject:"in_progress",close:"closed",reopen:"in_progress",escalate:"escalated",cancel:"cancelled"} as Record<string,string>)[input.action]??row.status,steps:SqlStatement[]=[
  write.prepare("SELECT id FROM operations_exceptions WHERE id=? FOR UPDATE").bind(recordId),
  assertion(write,key,"exception",`EXISTS(SELECT 1 FROM operations_exceptions e WHERE e.id=? AND e.version=? AND e.status=? AND ${scope})`,[recordId,input.expectedVersion,row.status,...args(a)]),
  ...await sourceRecordGuards(write,a,key,row.source),recipient(write,key,row,a.userId,manage?"records.review":"records.write"),
 ];
 if(input.action==="assign")steps.push(recipient(write,key,row,input.ownerId));
 if(input.action==="escalate")steps.push(recipient(write,key,row,input.escalationOwnerId,"records.review"));
 const reset=["reject","reopen","assign"].includes(input.action),owner=input.action==="assign"?input.ownerId:row.owner_id;
 steps.push(write.prepare("UPDATE operations_exceptions SET status=?,owner_id=?,priority=?,escalation_owner_id=?,resolution=?,resolved_by=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?")
  .bind(status,owner,input.action==="prioritize"?input.priority:row.priority,input.action==="escalate"?input.escalationOwnerId:row.escalation_owner_id,input.action==="resolve"?input.resolution:reset?null:row.resolution,input.action==="resolve"?a.userId:reset?null:row.resolved_by,input.action==="approve"?a.userId:reset?null:row.reviewed_by,now,recordId),
  write.prepare("INSERT INTO operations_exception_events VALUES(?,?,?,?,?,?,?,?,?)").bind(`operations-event-${key}`,recordId,input.expectedVersion+1,input.action,a.userId,input.reason,status,JSON.stringify(input),now),
  write.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'operational_exception',?,?,'Operational exception updated; review its current next step',?)").bind(`operations-notice-${key}`,a.companyId,input.action==="escalate"?input.escalationOwnerId:owner,recordId,input.expectedVersion+1,now));
 return executeCommand(write,a,key,op,input,recordId,input.expectedVersion+1,{id:recordId,status,version:input.expectedVersion+1},steps,{companyId:a.companyId,branchId:row.branch_id,departmentId:row.department_id});
}
export async function operationsExceptionWork(a:WorkContext):Promise<WorkItem[]>{
 if(!hasPermission(a,"records.read")||!hasModuleAccess(a.modules,"operations"))return [];
 const data=await operationsWorkspace(a);return data.exceptions.map(row=>{const terminal=["closed","cancelled"].includes(row.status),review=row.status==="awaiting_review",canAct=!terminal&&(review?data.canReview&&!samePerson(a,row.resolved_by)&&!samePerson(a,row.owner_id):row.owner_id===a.userId&&data.canWrite);
 return {id:row.id,type:"operational_exception",title:row.title,detail:row.issue,href:`/operations/${row.id}`,source:row.source,status:row.status,updatedAt:row.updated_at,dueAt:row.due_at,canAct,queue:terminal?"completed":canAct?"todo":"waiting",responsibleQueue:review?"Independent Operations reviewer":"Assigned exception owner",nextAction:review?"Review the proposed resolution":row.status==="open"?"Acknowledge the exception":row.status==="resolved"?"Close the reviewed exception":"Record progress and resolve the exception",relationships:[row.owner_id===a.userId?"assigned":row.created_by===a.userId?"requested":review?"approval":"team"]};});
}
