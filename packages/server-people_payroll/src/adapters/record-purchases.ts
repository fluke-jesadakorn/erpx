import {expenseSourceReadScope,requireExpenseWorkspaceRecordRead} from '@erpx/server-people_payroll/adapters/expense-record-purchases';
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {peoplePackageEnforcementMode,checkPurchasedCapability,readPurchasedAccess} from "@erpx/server-commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import {ApiError} from "@erpx/server-runtime/public/http";

type SourceKind="run"|"correction"|"advance"|"final-pay"|"payment";
const tables={run:"hr_payroll_runs",correction:"hr_payroll_corrections",advance:"hr_payroll_advances","final-pay":"hr_payroll_final_pay_preparations",payment:"hr_payment_batches"} as const;
type Fragment={where:string;args:unknown[]};
/** Notification self-service keeps its original employee-owned policy. A
 * different assignment held by the same account cannot elevate this context. */
function authority(actor:WorkContext,company:string,branch:string="NULL::text",department:string="NULL::text",permission?:string):Fragment{
 if(permission&&!hasPermission(actor,permission))return {where:"FALSE",args:[]};
 const scope=`(SELECT ${company} company_id,${branch} branch_id,${department} department_id) payroll_actor_scope`;
 const assignment=permission?assignmentReadSql("payroll_actor_scope",permission):`EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=payroll_actor_scope.company_id AND iam_assignment_active(g.id,${packageClockSql}) AND iam_principal_active(g.user_id,g.company_id,${packageClockSql}) AND EXISTS(SELECT 1 FROM iam_roles role WHERE role.id=g.role AND role.company_id IN ('*',g.company_id) AND role.status='active') AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=payroll_actor_scope.branch_id OR g.scope_kind='department' AND g.scope_node_id=payroll_actor_scope.department_id))`;
 return {where:`(SELECT ${assignment} FROM ${scope}) AND (?::text IS NULL OR ${branch}=?) AND (?::text IS NULL OR ${department}=?)`,args:[actor.assignmentId,actor.userId,actor.role,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId]};
}
function companyAuthority(actor:WorkContext,company:string,permission:string):Fragment{
 return actor.branchId||actor.departmentId?{where:"FALSE",args:[]}:authority(actor,company,"NULL::text","NULL::text",permission);
}
function sourceQuery(actor:WorkContext,kind:SourceKind,id:string,company:string,purchased:boolean):{sql:string;args:unknown[]}{
 const purchase=purchased?purchasedCapabilityPredicate("s.company_id","people.payroll","history","payroll_source_clock.now_ms"):"TRUE";
 let from=`${tables[kind]} s`,target="s.id",policy:Fragment;
 if(kind==="advance"){
  from+=" JOIN hr_engagements payroll_engagement ON payroll_engagement.id=s.engagement_id AND payroll_engagement.company_id=s.company_id JOIN hr_people p ON p.id=s.person_id AND p.id=payroll_engagement.person_id AND p.company_id=s.company_id";
  const self=authority(actor,"s.company_id","payroll_engagement.branch_id","payroll_engagement.department_id"),privileged=companyAuthority(actor,"s.company_id","payroll.read");
  policy={where:`((p.user_id=? AND ${self.where}) OR (${privileged.where}))`,args:[actor.userId,...self.args,...privileged.args]};
 }else if(kind==="final-pay"){
  from+=" JOIN people_preparation_tasks t ON t.id=s.task_id AND t.company_id=s.company_id JOIN hr_engagements e ON e.id=s.engagement_id AND e.company_id=s.company_id JOIN hr_payroll_runs r ON r.id=s.run_id AND r.company_id=s.company_id";
  const documents=companyAuthority(actor,"s.company_id","payroll.documents.read"),preparer=companyAuthority(actor,"s.company_id","payroll.prepare");
  policy={where:`t.clearance_type='payroll' AND (t.assignee_id=? OR t.completed_by=?) AND ${documents.where} AND ${preparer.where}`,args:[actor.userId,actor.userId,...documents.args,...preparer.args]};
 }else{
  if(kind==="correction"){from+=" JOIN hr_payroll_runs r ON r.id=s.run_id AND r.company_id=s.company_id";target="s.run_id";}
  if(kind==="payment")from+=" JOIN hr_payroll_runs r ON r.id=s.source_id AND r.company_id=s.company_id";
  policy=companyAuthority(actor,"s.company_id",kind==="payment"?"payroll.payment.read":"payroll.read");
 }
 return {sql:`SELECT ${target} target_id FROM ${from} CROSS JOIN payroll_source_clock WHERE s.id=${id} AND s.company_id=${company}${kind==="payment"?" AND s.source_type='payroll'":""} AND (${policy.where}) AND ${purchase}`,args:policy.args};
}
function visible(actor:WorkContext,kinds:readonly SourceKind[],id:string,company:string,purchased:boolean){
 const queries=kinds.map(kind=>sourceQuery(actor,kind,id,company,purchased));
 return {sql:`WITH payroll_source_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) ${queries.map(query=>query.sql).join(" UNION ALL ")}`,args:queries.flatMap(query=>query.args)};
}
const allKinds=Object.keys(tables) as SourceKind[];
const actualSource=(id:string)=>allKinds.map(kind=>`EXISTS(SELECT 1 FROM ${tables[kind]} s WHERE s.id=${id}${kind==="payment"?" AND s.source_type='payroll'":""})`).join(" OR ");
/** A generic People record only intersects Payroll when its ID resolves to an
 * actual Payroll owner. Directory, learning/project foundations and contractor
 * payment sources retain their own original owner policies. */
async function payrollSourceReadScopeBase(database:SqlDatabase,actor:WorkContext,surface:"records"|"staff"|"inbox"){
 const mode=await peoplePackageEnforcementMode();if(mode!=="enforced"){
  if(mode==="shadow")await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:"people.payroll"});
  return {where:"TRUE",args:[] as unknown[]};
 }
 if(surface==="records"){
  const query=visible(actor,allKinds,"workspace_records.id","workspace_records.company_id",true);
  return {where:`(NOT (module_id='people_payroll' AND (${actualSource("workspace_records.id")})) OR EXISTS(${query.sql}))`,args:query.args};
 }
 const type=surface==="staff"?"staff_notifications.record_type":"e.source_type",id=surface==="staff"?"staff_notifications.record_id":"e.source_id",company=surface==="staff"?"staff_notifications.company_id":"e.company_id";
 const correction=visible(actor,["correction"],id,company,true),advance=visible(actor,["advance"],id,company,true);
 return {where:`(${type} NOT IN ('payroll_correction','payroll_advance') OR (${type}='payroll_correction' AND EXISTS(${correction.sql})) OR (${type}='payroll_advance' AND EXISTS(${advance.sql})))`,args:[...correction.args,...advance.args]};
}
export async function payrollNotificationTarget(database:SqlDatabase,actor:WorkContext,kind:"correction"|"advance",id:string){
 if(await peoplePackageEnforcementMode()!=="enforced")return undefined;
 const query=visible(actor,[kind],"payroll_input.id","payroll_input.company_id",true);
 return (await database.prepare(`SELECT authorized.target_id target FROM (SELECT ?::text id,?::text company_id) payroll_input LEFT JOIN LATERAL (${query.sql} LIMIT 1) authorized ON TRUE`).bind(id,actor.companyId,...query.args).first<string>("target"))??null;
}
export async function requirePayrollWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,moduleId:string,id:string){
 await requireExpenseWorkspaceRecordRead(database,actor,moduleId,id);
 const mode=await peoplePackageEnforcementMode();if(moduleId!=="people_payroll"||mode==="disabled")return;
 const actual=await database.prepare(`SELECT 1 FROM (SELECT ?::text id) payroll_input WHERE ${actualSource("payroll_input.id")}`).bind(id).first();if(!actual)return;
 const query=visible(actor,allKinds,"payroll_input.id","payroll_input.company_id",false);
 const allowed=await database.prepare(`SELECT 1 FROM (SELECT ?::text id,?::text company_id) payroll_input WHERE EXISTS(${query.sql})`).bind(id,actor.companyId,...query.args).first();
 if(!allowed)throw new ApiError(404,"PAYROLL_RECORD_NOT_FOUND","The Payroll source was not found in this workspace.");
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:"people.payroll",purpose:"history"});
}

type PeopleSourceKind="person"|"engagement"|"compensation"|"lifecycle"|"clearance";
const peopleSourceTables={person:"hr_people",engagement:"hr_engagements",compensation:"hr_compensation_revisions",lifecycle:"people_lifecycle_cases",clearance:"people_preparation_tasks"} as const;
const baseKinds=Object.keys(peopleSourceTables) as PeopleSourceKind[];
function baseActual(id:string){return baseKinds.map(kind=>`EXISTS(SELECT 1 FROM ${peopleSourceTables[kind]} s WHERE s.id=${id}${kind==="clearance"?" AND s.clearance_type IS NOT NULL":""})`).join(" OR ");}
function baseSourceQuery(actor:WorkContext,kind:PeopleSourceKind,id:string,company:string,purchased:boolean,work=false){
 const capability=kind==="lifecycle"||kind==="clearance"?"people.lifecycle":"people.directory";let from=`${peopleSourceTables[kind]} s`,branch="NULL::text",department="NULL::text",policy:Fragment;
 if(kind==="person")from+=" LEFT JOIN LATERAL(SELECT x.branch_id,x.department_id FROM hr_engagements x WHERE x.person_id=s.id AND x.company_id=s.company_id ORDER BY CASE WHEN x.status IN ('active','onboarding','suspended') THEN 0 ELSE 1 END,x.start_date DESC,x.id LIMIT 1) people_engagement ON TRUE";
 else if(kind==="compensation")from+=" JOIN hr_engagements people_engagement ON people_engagement.id=s.engagement_id";
 else if(kind==="lifecycle")from+=" JOIN hr_engagements people_engagement ON people_engagement.id=s.engagement_id AND people_engagement.company_id=s.company_id";
 else if(kind==="clearance")from+=" JOIN people_lifecycle_cases c ON c.id=s.case_id AND c.company_id=s.company_id JOIN hr_engagements people_engagement ON people_engagement.id=c.engagement_id AND people_engagement.company_id=c.company_id";
 if(kind==="engagement"){branch="s.branch_id";department="s.department_id";}else{branch="people_engagement.branch_id";department="people_engagement.department_id";}
 const ownerCompany=kind==="compensation"?"people_engagement.company_id":"s.company_id";
 if(kind==="compensation"){
  const reader=authority(actor,ownerCompany,branch,department,"people.compensation.read"),payroll=authority(actor,ownerCompany,branch,department,"people.compensation.payroll");policy={where:`((${reader.where}) OR (${payroll.where}))`,args:[...reader.args,...payroll.args]};
 }else if(kind==="clearance"){
  const required:Record<string,string|undefined>={hr:"people.manage",handover:undefined,equipment:undefined,directory:"iam.directory.manage",payroll:"payroll.prepare",finance:"finance.write"};const scopes=Object.entries(required).map(([kind,permission])=>{const access=["payroll","finance","directory"].includes(kind)&&Boolean(actor.branchId||actor.departmentId)?{where:"FALSE",args:[]}:authority(actor,ownerCompany,branch,department,permission);return {where:`(s.clearance_type='${kind}' AND ${access.where})`,args:access.args};});const hr=authority(actor,ownerCompany,branch,department,"people.manage");policy=work?{where:`s.clearance_type IS NOT NULL AND ((s.assignee_id=? AND (${scopes.map(s=>s.where).join(" OR ")})) OR (${hr.where}) OR (${scopes.filter((_s,index)=>Object.values(required)[index]!==undefined).map(s=>s.where).join(" OR ")}))`,args:[actor.userId,...scopes.flatMap(s=>s.args),...hr.args,...scopes.filter((_s,index)=>Object.values(required)[index]!==undefined).flatMap(s=>s.args)]}:{where:`s.clearance_type IS NOT NULL AND (s.assignee_id=? OR s.completed_by=?) AND (${scopes.map(s=>s.where).join(" OR ")})`,args:[actor.userId,actor.userId,...scopes.flatMap(s=>s.args)]};
 }else if(work&&kind==="lifecycle"){
  const hr=authority(actor,ownerCompany,branch,department,"people.manage"),it=authority(actor,ownerCompany,branch,department,"iam.read");policy={where:`((${hr.where}) OR (${it.where}))`,args:[...hr.args,...it.args]};
 }else policy=authority(actor,ownerCompany,branch,department,"people.manage");
 const purchase=purchased?purchasedCapabilityPredicate(ownerCompany,capability,"history","people_source_clock.now_ms"):"TRUE";
 return {sql:`SELECT s.id target_id FROM ${from} CROSS JOIN people_source_clock WHERE s.id=${id} AND ${ownerCompany}=${company} AND (${policy.where}) AND ${purchase}`,args:policy.args};
}
function baseVisible(actor:WorkContext,kinds:readonly PeopleSourceKind[],id:string,company:string,purchased:boolean,work=false){const queries=kinds.map(kind=>baseSourceQuery(actor,kind,id,company,purchased,work));return {sql:`WITH people_source_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) ${queries.map(q=>q.sql).join(" UNION ALL ")}`,args:queries.flatMap(q=>q.args)};}
/** Actual People identities only. Generic imported records and Organization,
 * Projects worker and Learning learner foundation references retain owners. */
export async function peopleSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:"records"|"staff"|"inbox"){
 const mode=await peoplePackageEnforcementMode();if(mode!=="enforced"){if(mode==="shadow")for(const capability of ["people.directory","people.lifecycle"])await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability});return {where:"TRUE",args:[] as unknown[]};}
 if(surface==="records"){const visible=baseVisible(actor,baseKinds,"workspace_records.id","workspace_records.company_id",true);return {where:`(NOT(module_id='people_payroll' AND (${baseActual("workspace_records.id")})) OR EXISTS(${visible.sql}))`,args:visible.args};}
 const type=surface==="staff"?"staff_notifications.record_type":"e.source_type",id=surface==="staff"?"staff_notifications.record_id":"e.source_id",company=surface==="staff"?"staff_notifications.company_id":"e.company_id",visible=baseVisible(actor,["clearance"],id,company,true);
 return {where:`(${type}<>'retirement_clearance' OR EXISTS(${visible.sql}))`,args:visible.args};
}
export async function requirePeopleWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,moduleId:string,id:string){
 const mode=await peoplePackageEnforcementMode();if(moduleId!=="people_payroll"||mode==="disabled")return;
 const actual=await database.prepare(`SELECT 1 FROM(SELECT ?::text id) people_input WHERE ${baseActual("people_input.id")}`).bind(id).first();if(!actual)return;
 const query=baseVisible(actor,baseKinds,"people_input.id","people_input.company_id",mode==="enforced");if(!await database.prepare(`SELECT 1 FROM(SELECT ?::text id,?::text company_id)people_input WHERE EXISTS(${query.sql})`).bind(id,actor.companyId,...query.args).first())throw new ApiError(404,"PEOPLE_RECORD_NOT_FOUND","The actual People source was not found in this workspace.");
}
export async function peopleClearanceNotificationTarget(database:SqlDatabase,actor:WorkContext,id:string){
 if(await peoplePackageEnforcementMode()!=="enforced")return undefined;const query=baseVisible(actor,["clearance"],"people_input.id","people_input.company_id",true);return Boolean(await database.prepare(`SELECT 1 FROM(SELECT ?::text id,?::text company_id) people_input WHERE EXISTS(${query.sql})`).bind(id,actor.companyId,...query.args).first());
}

/** One actual-source query serves owner Work and the shared post-Flow pass.
 * No gate infers authority from a Work title, type label or caller package. */
export async function peopleWorkSourceAccess(database:SqlDatabase,actor:WorkContext,ids:readonly string[]){
 const mode=await peoplePackageEnforcementMode();if(mode==="disabled"||!ids.length)return null;
 if(mode==="shadow"){for(const capability of ["people.directory","people.lifecycle","foundation.identity"])await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability});return null;}
 const now="people_source_clock.now_ms",lifecycle=baseSourceQuery(actor,"lifecycle","people_work_input.id","people_work_input.company_id",true,true),clearance=baseSourceQuery(actor,"clearance","people_work_input.id","people_work_input.company_id",true,true);
 lifecycle.sql=lifecycle.sql.replace("SELECT s.id target_id",`SELECT s.id target_id,CASE WHEN s.kind IN ('offboarding','suspend') THEN ${purchasedCapabilityPredicate("s.company_id","people.lifecycle","settle-existing",now)} ELSE ${purchasedCapabilityPredicate("s.company_id","people.lifecycle","new-work",now)} END can_act`);
 clearance.sql=clearance.sql.replace("SELECT s.id target_id",`SELECT s.id target_id,${purchasedCapabilityPredicate("s.company_id","people.lifecycle","settle-existing",now)} can_act`);
 const foundation=authority(actor,"s.company_id","COALESCE(p.branch_id,ce.branch_id)","COALESCE(p.department_id,ce.department_id)","iam.read"),hr=authority(actor,"s.company_id","COALESCE(p.branch_id,ce.branch_id)","COALESCE(p.department_id,ce.department_id)","people.manage"),organization=authority(actor,"s.company_id","COALESCE(p.branch_id,ce.branch_id)","COALESCE(p.department_id,ce.department_id)","organization.manage"),review=companyAuthority(actor,"s.company_id","iam.review"),directory=companyAuthority(actor,"s.company_id","iam.read");
 const foundationHistory=purchasedCapabilityPredicate("s.company_id","foundation.identity","history",now),foundationSettle=purchasedCapabilityPredicate("s.company_id","foundation.identity","settle-existing",now),lifecycleHistory=purchasedCapabilityPredicate("s.company_id","people.lifecycle","history",now),lifecycleNew=purchasedCapabilityPredicate("s.company_id","people.lifecycle","new-work",now);
 const preparation=`SELECT s.id target_id,CASE WHEN s.case_id IS NULL THEN ${foundationSettle} WHEN c.kind IN ('offboarding','suspend') THEN ${purchasedCapabilityPredicate("s.company_id","people.lifecycle","settle-existing",now)} ELSE ${lifecycleNew} END can_act FROM people_preparation_tasks s LEFT JOIN people_lifecycle_cases c ON c.id=s.case_id AND c.company_id=s.company_id LEFT JOIN organization_positions p ON p.id=s.position_id AND p.company_id=s.company_id LEFT JOIN hr_engagements ce ON ce.id=c.engagement_id AND ce.company_id=c.company_id CROSS JOIN people_source_clock WHERE s.id=people_work_input.id AND s.company_id=people_work_input.company_id AND s.clearance_type IS NULL AND ((s.case_id IS NULL AND ${foundationHistory}) OR (c.id IS NOT NULL AND ${lifecycleHistory})) AND ((${foundation.where}) OR (${hr.where}) OR (${organization.where}))`;
 const reviews=`SELECT s.id target_id,CASE WHEN c.kind IN ('offboarding','suspend') THEN ${purchasedCapabilityPredicate("s.company_id","people.lifecycle","settle-existing",now)} ELSE ${lifecycleNew} END can_act FROM iam_reviews s JOIN people_lifecycle_cases c ON c.id=s.payload->>'caseId' AND c.company_id=s.company_id CROSS JOIN people_source_clock WHERE s.id=people_work_input.id AND s.company_id=people_work_input.company_id AND s.kind LIKE 'people.lifecycle.%' AND ${review.where} AND ${lifecycleHistory}`;
 const operations=`SELECT s.id target_id,${foundationSettle} can_act FROM people_directory_operations s CROSS JOIN people_source_clock WHERE s.id=people_work_input.id AND s.company_id=people_work_input.company_id AND ${directory.where} AND ${foundationHistory}`;
 const rows=await database.prepare(`WITH people_source_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms),people_work_input AS(SELECT jsonb_array_elements_text(?::jsonb) id,?::text company_id) SELECT permitted.target_id,bool_or(permitted.can_act) can_act FROM people_work_input JOIN LATERAL(${[lifecycle.sql,clearance.sql,preparation,reviews,operations].join(" UNION ALL ")})permitted ON TRUE GROUP BY permitted.target_id`).bind(JSON.stringify([...new Set(ids)]),actor.companyId,...lifecycle.args,...clearance.args,...foundation.args,...hr.args,...organization.args,...review.args,...directory.args).all<{target_id:string;can_act:boolean}>();
 return new Map(rows.results.map(row=>[row.target_id,{history:true,canAct:row.can_act}]));
}
export async function filterPeopleLifecycleWork(database:SqlDatabase,actor:WorkContext,items:import("@erpx/feature-work_projection/lib/work-item").WorkItem[]){
 const access=await peopleWorkSourceAccess(database,actor,items.filter(item=>item.type==="people_lifecycle").map(item=>item.id));if(access===null)return items;
 return items.flatMap(item=>{if(item.type!=="people_lifecycle")return [item];const source=access.get(item.id);if(!source?.history)return [];if(source.canAct)return [item];return [{...item,canAct:false,actionKind:"work" as const,queue:item.queue==="completed"?"completed" as const:"waiting" as const,nextAction:"Review retained People work",flow:item.flow?{...item.flow,canAct:false,canManage:false}:undefined,relationships:(item.relationships??[]).filter(value=>!["assigned","approval"].includes(value))}];});
}

/** Actual Expense sources intersect every existing shared Payroll/People scope. */
export async function payrollSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:"records"|"staff"|"inbox"){
 const payroll=await payrollSourceReadScopeBase(database,actor,surface),expenses=await expenseSourceReadScope(database,actor,surface);return {where:`(${payroll.where}) AND (${expenses.where})`,args:[...payroll.args,...expenses.args]};
}
