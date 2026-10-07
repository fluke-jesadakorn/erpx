import {hasPermission} from '@/lib/permissions';import {clearancePermissions,type ClearanceKind,type ClearanceReference} from '@/lib/retirement';import type {WorkContext} from '../../identity_access/public/authorization';import type {PreparationTask} from '@/lib/people-lifecycle';import type {WorkItem} from '@/lib/work-item';import {BusinessRuleError,type Clock} from '../../shared';import {canCompleteClearance,canReadClearanceEvidence} from '../domain/retirement-clearance';export interface RetirementClearanceRepository{eligibleOwner(companyId:string,userId:string,now:number,permission:string | null,permission2:string | null,kind:"hr" | "handover" | "equipment" | "directory" | "payroll" | "finance"):Promise<Record<string,unknown>|null>;
tasks(companyId:string,branchId:string | null,branchId2:string | null,departmentId:string | null,departmentId2:string | null):Promise<{results:(PreparationTask & {legal_name:string;case_status:string})[]}>;
caseRow(id:string,companyId:string):Promise<{id:string;status:string;effective_at:number;definition:string;legal_name:string}|null>;
caseTasks(id:string,companyId:string):Promise<{results:PreparationTask[]}>;
finalPays(companyId:string,id:string):Promise<{results:{task_id:string;preparation_id:string;run_id:string;status:string;version:number;pay_group_id:string;currency:string;period_start:string;period_end:string;pay_date:string;run_status:string;run_version:number}[]}>;
finalPayOptions(companyId:string,id:string):Promise<{results:{task_id:string;pay_group_id:string|null;currency:string;last_working_date:string|null}[]}>;
source(companyId:string,id:string,kind:ClearanceReference['kind']):Promise<{status:string;label:string;path:string}>;
accessEnded(companyId:string,id:string,now:number):Promise<Record<string,unknown>|null>;
directoryEvidence(id:string,id2:string,companyId:string):Promise<{total:number;verified:number}|null>;
countryPacks():Promise<{results:{id:string;version:string;effective_from:string;effective_to:string|null}[]}>;
owners(companyId:string,now:number):Promise<{results:{id:string;name:string;kind:ClearanceKind}[]}>;}export interface RetirementClearancePorts{repository:RetirementClearanceRepository;clock:Clock;requireSourceAccess(actor:WorkContext,kind:ClearanceReference['kind'],id:string):Promise<unknown>;}export function createRetirementClearanceApplication(ports:RetirementClearancePorts){const repository=ports.repository;const denied = (message:string):never => {throw new BusinessRuleError(403,"CLEARANCE_AUTHORITY",message);};async function eligibleClearanceOwner(companyId:string,userId:string,kind:ClearanceKind) {
 const permission=clearancePermissions[kind];
 return !!await repository.eligibleOwner(companyId,userId,ports.clock.now(),permission,permission,kind);
}
async function requireClearanceCompletion(actor:WorkContext,task:PreparationTask,input:{status?:string;reason?:string;references?:ClearanceReference[]}) {
 if(!canCompleteClearance(actor,task) || !await eligibleClearanceOwner(actor.companyId,actor.userId,task.clearance_type!))denied("Only the assigned colleague with current department authority can close this clearance task.");
 if((input.reason??"").trim().length<3)throw new BusinessRuleError(422,"CLEARANCE_REASON","Record the review or explain why this task is not required.");
 if(input.status!=="not_required" && ["finance","payroll"].includes(task.clearance_type!) && !input.references?.length)throw new BusinessRuleError(422,"CLEARANCE_EVIDENCE","Link the source documents reviewed, or mark not required with a reason.");
 for(const reference of input.references??[]) {
  if(task.clearance_type === "payroll" && reference.kind !== "payroll")throw new BusinessRuleError(422,"CLEARANCE_EVIDENCE","Final payroll must reference a payroll run.");
  await ports.requireSourceAccess(actor,reference.kind,reference.id);
 }
}
async function requireClearancePlan(actor:WorkContext,task:PreparationTask,assigneeId:string|null) {
 const kind=task.clearance_type!;
 if(!hasPermission(actor,"people.manage") && !(clearancePermissions[kind] && hasPermission(actor,clearancePermissions[kind]!)))denied("HR or the responsible department must assign this task.");
 if(assigneeId && !await eligibleClearanceOwner(actor.companyId,assigneeId,kind))throw new BusinessRuleError(422,"CLEARANCE_ASSIGNEE","Choose an eligible colleague in the responsible department.");
}
async function retirementWork(actor:WorkContext):Promise<WorkItem[]> {
 const rows=(await repository.tasks(actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId)).results;
 return rows.filter(t => t.assignee_id===actor.userId || hasPermission(actor,"people.manage") || (clearancePermissions[t.clearance_type!] && hasPermission(actor,clearancePermissions[t.clearance_type!]!))).map(t => ({id:t.id,type:"people_lifecycle",title:`${t.legal_name} · ${t.title}`,detail:t.assignee_id ? `${t.clearance_type} clearance` : `Assign ${t.clearance_type} owner`,href:`/people/clearance/${t.case_id}`,status:t.status,updatedAt:t.updated_at,queue:["completed","not_required","cancelled"].includes(t.status)?"completed":canCompleteClearance(actor,t)?"todo":"waiting",canAct:t.status==="pending" && canCompleteClearance(actor,t),category:t.assignee_id?"operational":"setup",setupReason:t.assignee_id?null:`Assign ${t.clearance_type} owner`,responsibleQueue:t.responsibility || t.clearance_type!,dueAt:t.due_date?Date.parse(`${t.due_date}T00:00:00+07:00`):null,relationships:t.assignee_id===actor.userId?["assigned"]:["team"]}));
}
async function clearanceCase(actor:WorkContext,id:string) {
 const visible=await retirementWork(actor);
 if(!visible.some(t=>t.href===`/people/clearance/${id}`))throw new BusinessRuleError(404,"NOT_FOUND","Clearance case not found in this workspace.");
 const row=await repository.caseRow(id,actor.companyId);
 if(!row)throw new BusinessRuleError(404,"NOT_FOUND","Case not found.");
 const tasks=(await repository.caseTasks(id,actor.companyId)).results;
 const finalPays=(await repository.finalPays(actor.companyId,id)).results;
 const finalPayOptions=(await repository.finalPayOptions(actor.companyId,id)).results;
 const detailed=await Promise.all(tasks.map(async t=>{
  const evidenceReadable=canReadClearanceEvidence(actor,t);
  const refs=typeof t.document_references === "string" ? JSON.parse(t.document_references) as ClearanceReference[] : t.document_references??[];
  const evidence=evidenceReadable?await Promise.all(refs.map(async ref=>{
   try {
    await ports.requireSourceAccess(actor,ref.kind,ref.id);
    const source=await repository.source(actor.companyId,ref.id,ref.kind);
    return {label:source?.label??ref.kind,status:source?.status??"Unavailable",href:`${source.path}${ref.id}`};
   } catch(e){if(!(e instanceof BusinessRuleError)||![403,404].includes(e.status))throw e; return {label:"Restricted source document",status:"Read permission required",href:null};}
  })):[];
  return {...t,evidenceReadable,completion_reason:!evidenceReadable || ["finance","payroll"].includes(t.clearance_type ?? "") && (!hasPermission(actor,t.clearance_type === "payroll" ? "payroll.read" : "finance.read") || evidence.some(e=>!e.href)) ? null : t.completion_reason,document_references:undefined,evidence,finalPay:evidenceReadable?finalPays.find(fp=>fp.task_id===t.id)??null:null,finalPayOptions:evidenceReadable?finalPayOptions.find(fp=>fp.task_id===t.id)??null:null,canComplete:t.status==="pending" && canCompleteClearance(actor,t),canPlan:t.status==="pending" && (hasPermission(actor,"people.manage") || !!clearancePermissions[t.clearance_type!] && hasPermission(actor,clearancePermissions[t.clearance_type!]!))};
 }));
 const definition = typeof row.definition === "string" ? JSON.parse(row.definition) : row.definition;
 const accessEnded = !!await repository.accessEnded(actor.companyId,id,ports.clock.now());
 const directory = await repository.directoryEvidence(id,id,actor.companyId);
 const countryPacks=hasPermission(actor,"payroll.prepare")?(await repository.countryPacks()).results:[];
 return {case:{id:row.id,status:row.status,effectiveAt:row.effective_at,name:row.legal_name,timezone:String(definition.timezone??"Asia/Bangkok"),lastWorkingDate:String(definition.lastWorkingDate??""),retirementEffective:["scheduled","completed"].includes(row.status)&&row.effective_at<=ports.clock.now(),accessEnded,directory:!directory?.total?"Not required":directory.total===directory.verified?"Verified":"Pending"},tasks:detailed,countryPacks};
}
async function clearanceOwners(actor:WorkContext) {
 const result=await repository.owners(actor.companyId,ports.clock.now());
 const owners=new Map<string,{id:string;name:string;kinds:ClearanceKind[]}>();
 for(const row of result.results){const owner=owners.get(row.id)??{id:row.id,name:row.name,kinds:[]};owner.kinds.push(row.kind);owners.set(row.id,owner);}
 return [...owners.values()];
}return {eligibleClearanceOwner,canCompleteClearance,canReadClearanceEvidence,requireClearanceCompletion,requireClearancePlan,retirementWork,clearanceCase,clearanceOwners};}
