import "server-only";
import { hasPermission } from "@/lib/permissions";
import { clearancePermissions, type ClearanceKind, type ClearanceReference } from "@/lib/retirement";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import type { PreparationTask } from "@/lib/people-lifecycle";
import type { WorkItem } from "@/lib/work-item";
import { ApiError } from "./http";
import { requireFlowAccess } from "./flow-access";
const denied = (message:string):never => {throw new ApiError(403,"CLEARANCE_AUTHORITY",message);};
export async function eligibleClearanceOwner(db:SqlDatabase,companyId:string,userId:string,kind:ClearanceKind) {
 const permission=clearancePermissions[kind];
 return !!await db.prepare(`SELECT 1 FROM assignments a WHERE a.company_id=? AND a.user_id=? AND iam_assignment_active(a.id,?) AND (?::text IS NULL OR iam_role_allows(a.role,a.company_id,?)) AND (? NOT IN ('payroll','finance','directory') OR a.scope_kind='holding') LIMIT 1`).bind(companyId,userId,Date.now(),permission,permission,kind).first();
}
export function canCompleteClearance(actor:WorkContext,task:Pick<PreparationTask,"assignee_id"|"clearance_type">) {
 const kind=task.clearance_type;
 if(!kind || task.assignee_id!==actor.userId)return false;
 return (!clearancePermissions[kind] || hasPermission(actor,clearancePermissions[kind]!)) && (!["payroll","finance","directory"].includes(kind) || !actor.branchId && !actor.departmentId);
}
export function canReadClearanceEvidence(actor:WorkContext,task:Pick<PreparationTask,"assignee_id"|"clearance_type"|"completed_by">) {
 const kind=task.clearance_type;if(!kind)return false;
 const permission=clearancePermissions[kind];
 if(permission&&!hasPermission(actor,permission))return false;
 if(["payroll","finance","directory"].includes(kind)&&(actor.branchId||actor.departmentId))return false;
 // Clearance evidence belongs to one responsibility, not the whole People or
 // department audience.  A current permission is necessary but never enough:
 // only the assigned worker (or the worker whose completed decision is now
 // part of the immutable history) may read that task's evidence.
 return task.assignee_id===actor.userId || task.completed_by===actor.userId;
}
export async function requireClearanceCompletion(db:SqlDatabase,actor:WorkContext,task:PreparationTask,input:{status?:string;reason?:string;references?:ClearanceReference[]}) {
 if(!canCompleteClearance(actor,task) || !await eligibleClearanceOwner(db,actor.companyId,actor.userId,task.clearance_type!))denied("Only the assigned colleague with current department authority can close this clearance task.");
 if((input.reason??"").trim().length<3)throw new ApiError(422,"CLEARANCE_REASON","Record the review or explain why this task is not required.");
 if(input.status!=="not_required" && ["finance","payroll"].includes(task.clearance_type!) && !input.references?.length)throw new ApiError(422,"CLEARANCE_EVIDENCE","Link the source documents reviewed, or mark not required with a reason.");
 for(const reference of input.references??[]) {
  if(task.clearance_type === "payroll" && reference.kind !== "payroll")throw new ApiError(422,"CLEARANCE_EVIDENCE","Final payroll must reference a payroll run.");
  await requireFlowAccess(actor,reference.kind,reference.id);
 }
}
export async function requireClearancePlan(db:SqlDatabase,actor:WorkContext,task:PreparationTask,assigneeId:string|null) {
 const kind=task.clearance_type!;
 if(!hasPermission(actor,"people.manage") && !(clearancePermissions[kind] && hasPermission(actor,clearancePermissions[kind]!)))denied("HR or the responsible department must assign this task.");
 if(assigneeId && !await eligibleClearanceOwner(db,actor.companyId,assigneeId,kind))throw new ApiError(422,"CLEARANCE_ASSIGNEE","Choose an eligible colleague in the responsible department.");
}
export async function retirementWork(db:SqlDatabase,actor:WorkContext):Promise<WorkItem[]> {
 const rows=(await db.prepare(`SELECT t.*,p.legal_name,c.status case_status,e.branch_id,e.department_id FROM people_preparation_tasks t JOIN people_lifecycle_cases c ON c.id=t.case_id AND c.company_id=t.company_id JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id JOIN hr_people p ON p.id=c.person_id AND p.company_id=c.company_id WHERE t.company_id=? AND t.clearance_type IS NOT NULL AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)`).bind(actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId).all<PreparationTask & {legal_name:string;case_status:string}>()).results;
 return rows.filter(t => t.assignee_id===actor.userId || hasPermission(actor,"people.manage") || (clearancePermissions[t.clearance_type!] && hasPermission(actor,clearancePermissions[t.clearance_type!]!))).map(t => ({id:t.id,type:"people_lifecycle",title:`${t.legal_name} · ${t.title}`,detail:t.assignee_id ? `${t.clearance_type} clearance` : `Assign ${t.clearance_type} owner`,href:`/people/clearance/${t.case_id}`,status:t.status,updatedAt:t.updated_at,queue:["completed","not_required","cancelled"].includes(t.status)?"completed":canCompleteClearance(actor,t)?"todo":"waiting",canAct:t.status==="pending" && canCompleteClearance(actor,t),category:t.assignee_id?"operational":"setup",setupReason:t.assignee_id?null:`Assign ${t.clearance_type} owner`,responsibleQueue:t.responsibility || t.clearance_type!,dueAt:t.due_date?Date.parse(`${t.due_date}T00:00:00+07:00`):null,relationships:t.assignee_id===actor.userId?["assigned"]:["team"]}));
}
const sources={payroll:["hr_payroll_runs","status","pay_date","/people/payroll/runs/"],expense:["finance_expenses","status","merchant","/expenses/"],invoice:["finance_invoices","status","invoice_number","/finance/invoices/"],journal:["finance_journals","status","journal_number","/finance/journals/"]} as const;
export async function clearanceCase(db:SqlDatabase,actor:WorkContext,id:string) {
 const visible=await retirementWork(db,actor);
 if(!visible.some(t=>t.href===`/people/clearance/${id}`))throw new ApiError(404,"NOT_FOUND","Clearance case not found in this workspace.");
 const row=await db.prepare(`SELECT c.id,c.status,c.effective_at,c.definition,p.legal_name FROM people_lifecycle_cases c JOIN hr_people p ON p.id=c.person_id AND p.company_id=c.company_id WHERE c.id=? AND c.company_id=?`).bind(id,actor.companyId).first<{id:string;status:string;effective_at:number;definition:string;legal_name:string}>();
 if(!row)throw new ApiError(404,"NOT_FOUND","Case not found.");
 const tasks=(await db.prepare("SELECT * FROM people_preparation_tasks WHERE case_id=? AND company_id=? ORDER BY id").bind(id,actor.companyId).all<PreparationTask>()).results;
 const finalPays=(await db.prepare(`SELECT fp.task_id,fp.id preparation_id,fp.run_id,fp.status,fp.version,r.pay_group_id,r.currency,r.period_start,r.period_end,r.pay_date,r.status run_status,r.version run_version
   FROM hr_payroll_final_pay_preparations fp JOIN hr_payroll_runs r ON r.id=fp.run_id
   WHERE fp.company_id=? AND fp.case_id=? ORDER BY fp.created_at DESC`).bind(actor.companyId,id).all<{task_id:string;preparation_id:string;run_id:string;status:string;version:number;pay_group_id:string;currency:string;period_start:string;period_end:string;pay_date:string;run_status:string;run_version:number}>()).results;
 const finalPayOptions=(await db.prepare(`SELECT t.id task_id,e.pay_group_id,e.currency,COALESCE(e.end_date,c.definition->>'lastWorkingDate') last_working_date
   FROM people_preparation_tasks t JOIN people_lifecycle_cases c ON c.id=t.case_id AND c.company_id=t.company_id JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id
   WHERE t.company_id=? AND t.case_id=? AND t.clearance_type='payroll'`).bind(actor.companyId,id).all<{task_id:string;pay_group_id:string|null;currency:string;last_working_date:string|null}>()).results;
 const detailed=await Promise.all(tasks.map(async t=>{
  const evidenceReadable=canReadClearanceEvidence(actor,t);
  const refs=typeof t.document_references === "string" ? JSON.parse(t.document_references) as ClearanceReference[] : t.document_references??[];
  const evidence=evidenceReadable?await Promise.all(refs.map(async ref=>{
   try {
    await requireFlowAccess(actor,ref.kind,ref.id);
    const [table,status,label,path]=sources[ref.kind];
    const source=await db.prepare(`SELECT ${status} status,${label} label FROM ${table} WHERE id=? AND company_id=?`).bind(ref.id,actor.companyId).first<{status:string;label:string}>();
    return {label:source?.label??ref.kind,status:source?.status??"Unavailable",href:`${path}${ref.id}`};
   } catch(e){if(!(e instanceof ApiError)||![403,404].includes(e.status))throw e; return {label:"Restricted source document",status:"Read permission required",href:null};}
  })):[];
  return {...t,evidenceReadable,completion_reason:!evidenceReadable || ["finance","payroll"].includes(t.clearance_type ?? "") && (!hasPermission(actor,t.clearance_type === "payroll" ? "payroll.read" : "finance.read") || evidence.some(e=>!e.href)) ? null : t.completion_reason,document_references:undefined,evidence,finalPay:evidenceReadable?finalPays.find(fp=>fp.task_id===t.id)??null:null,finalPayOptions:evidenceReadable?finalPayOptions.find(fp=>fp.task_id===t.id)??null:null,canComplete:t.status==="pending" && canCompleteClearance(actor,t),canPlan:t.status==="pending" && (hasPermission(actor,"people.manage") || !!clearancePermissions[t.clearance_type!] && hasPermission(actor,clearancePermissions[t.clearance_type!]!))};
 }));
 const definition = typeof row.definition === "string" ? JSON.parse(row.definition) : row.definition;
 const accessEnded = !!await db.prepare("SELECT 1 FROM people_access_cutoffs WHERE company_id=? AND case_id=? AND cancelled_at IS NULL AND effective_at<=? AND restored_at IS NULL").bind(actor.companyId,id,Date.now()).first();
 const directory = await db.prepare(`SELECT count(*) total,count(*) FILTER(WHERE EXISTS(SELECT 1 FROM people_directory_operations o WHERE o.company_id=m.company_id AND o.connection_id=i.connection_id AND o.status='completed' AND o.result->>'verified'='true' AND o.definition->'action'->>'caseId'=? AND o.definition->'action'->>'objectId'=i.external_id AND o.definition->'action'->>'kind'='account.disable')) verified FROM iam_members m JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id JOIN people_lifecycle_cases c ON c.company_id=m.company_id AND c.person_id=m.person_id WHERE c.id=? AND c.company_id=? AND m.status='active'`).bind(id,id,actor.companyId).first<{total:number;verified:number}>();
 const countryPacks=hasPermission(actor,"payroll.prepare")?(await db.prepare("SELECT id,version,effective_from,effective_to FROM hr_country_packs WHERE country_code='TH' AND status='active' AND adapter_id='thai-payroll-v1' ORDER BY effective_from DESC,id").all<{id:string;version:string;effective_from:string;effective_to:string|null}>()).results:[];
 return {case:{id:row.id,status:row.status,effectiveAt:row.effective_at,name:row.legal_name,timezone:String(definition.timezone??"Asia/Bangkok"),lastWorkingDate:String(definition.lastWorkingDate??""),retirementEffective:["scheduled","completed"].includes(row.status)&&row.effective_at<=Date.now(),accessEnded,directory:!directory?.total?"Not required":directory.total===directory.verified?"Verified":"Pending"},tasks:detailed,countryPacks};
}

export async function clearanceOwners(db:SqlDatabase,actor:WorkContext) {
 const result=await db.prepare(`SELECT DISTINCT a.user_id id,coalesce(p.legal_name,a.user_id) name,k.kind FROM assignments a LEFT JOIN hr_people p ON p.user_id=a.user_id AND p.company_id=a.company_id AND p.merged_into_id IS NULL CROSS JOIN (VALUES ('hr','people.manage'),('handover',NULL),('equipment',NULL),('directory','iam.directory.manage'),('payroll','payroll.prepare'),('finance','finance.write')) k(kind,permission) WHERE a.company_id=? AND iam_assignment_active(a.id,?) AND (k.permission IS NULL OR iam_role_allows(a.role,a.company_id,k.permission)) AND (k.kind NOT IN ('payroll','finance','directory') OR a.scope_kind='holding') ORDER BY name`).bind(actor.companyId,Date.now()).all<{id:string;name:string;kind:ClearanceKind}>();
 const owners=new Map<string,{id:string;name:string;kinds:ClearanceKind[]}>();
 for(const row of result.results){const owner=owners.get(row.id)??{id:row.id,name:row.name,kinds:[]};owner.kinds.push(row.kind);owners.set(row.id,owner);}
 return [...owners.values()];
}
