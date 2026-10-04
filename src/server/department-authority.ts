import { z } from "zod";
import type { SqlDatabase } from "./runtime/types";
import type { WorkContext } from "./authorization";
import { ApiError } from "./http";
import { assertion, executeCommand, replayCommand } from "./commands";
import { activeStaff, staffPeople, reportingChain, eligibleRoleUsers, type ApprovalRoute, type RouteStep } from "./staff-policy";

const id = z.string().min(1).max(180);
const reason = z.string().trim().min(3).max(1000);
export const appointmentInput = z.object({
  kind: z.enum(["ceo", "hr", "head"]),
  departmentId: id.nullable().default(null),
  positionId: id.nullable(),
  personId: id.nullable(),
  expectedVersion: z.number().int().min(0),
  effectiveAt: z.number().int().positive().optional(),
  reason,
});
export const departmentFlowInput = z.object({
  departmentId: id,
  expectedVersion: z.number().int().min(0),
  steps: z.array(z.discriminatedUnion("kind", [
    z.object({ kind: z.literal("person"), personId: id }),
    z.object({ kind: z.literal("position"), positionId: id }),
  ])).min(1).max(20),
  reason,
});
export type FlowSelector = z.infer<typeof departmentFlowInput>["steps"][number];
type Seat = { company_id:string; kind:"ceo"|"hr"|"head"; scope_id:string; version:number; position_id:string|null; person_id:string|null; effective_at:number|null };
type Policy = { id:string; version:number; steps_json:FlowSelector[]|string };

async function seat(db:SqlDatabase, companyId:string, kind:Seat["kind"], scopeId:string) {
  return db.prepare("SELECT * FROM authority_seats WHERE company_id=? AND kind=? AND scope_id=?")
    .bind(companyId,kind,scopeId).first<Seat>();
}
async function personForActor(db:SqlDatabase, actor:Pick<WorkContext,"companyId"|"userId">) {
  return db.prepare("SELECT id FROM hr_people WHERE company_id=? AND iam_same_person(user_id,?,company_id) AND status='active' AND merged_into_id IS NULL")
    .bind(actor.companyId,actor.userId).first<{id:string}>();
}
async function appointmentPower(db:SqlDatabase, actor:WorkContext, kind:Seat["kind"]) {
  if(actor.role==="holding_owner" && actor.assignmentScope==="holding")return "owner";
  const person=await personForActor(db,actor);
  if(!person)return null;
  const current=await db.prepare(`SELECT DISTINCT s.kind FROM authority_seats s JOIN hr_engagements e ON e.person_id=s.person_id AND e.company_id=s.company_id AND e.status='active' WHERE s.company_id=? AND s.scope_id=? AND s.person_id=? AND s.effective_at<=? AND s.kind IN ('ceo','hr')`)
    .bind(actor.companyId,actor.companyId,person.id,Date.now()).all<{kind:string}>();
  if(current.results.some(s=>s.kind==="ceo") && kind!=="ceo")return "ceo";
  if(current.results.some(s=>s.kind==="hr") && kind==="head")return "hr";
  return null;
}
function powerGuard(db:SqlDatabase,key:string,actor:WorkContext,power:string){
  if(power==="owner")return assertion(db,key,"owner-appointment-power",`EXISTS(SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role='holding_owner' AND scope_kind='holding' AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint))`,[actor.assignmentId,actor.userId,actor.companyId]);
  return assertion(db,key,"appointment-power",`EXISTS(SELECT 1 FROM authority_seats s JOIN hr_people p ON p.id=s.person_id AND p.company_id=s.company_id JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active' WHERE s.company_id=? AND s.scope_id=? AND s.kind=? AND iam_same_person(p.user_id,?,s.company_id) AND s.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND p.status='active' AND p.merged_into_id IS NULL)`,[actor.companyId,actor.companyId,power,actor.userId]);
}
export async function appointmentOverview(db:SqlDatabase,actor:WorkContext) {
  const canAppointKinds=(await Promise.all((["ceo","hr","head"] as const).map(async kind=>[kind,!!await appointmentPower(db,actor,kind)] as const)))
    .filter(([,allowed])=>allowed).map(([kind])=>kind);
  const seats=(await db.prepare(`SELECT s.*,p.legal_name person_name,pos.title position_title,n.display_name department_name FROM authority_seats s
    LEFT JOIN hr_people p ON p.id=s.person_id LEFT JOIN organization_positions pos ON pos.id=s.position_id
    LEFT JOIN organization_nodes n ON n.id=s.scope_id AND s.kind='head'
    WHERE s.company_id=? ORDER BY CASE s.kind WHEN 'ceo' THEN 0 WHEN 'hr' THEN 1 ELSE 2 END,n.display_name`)
    .bind(actor.companyId).all()).results;
  const history=(await db.prepare("SELECT * FROM authority_appointments WHERE company_id=? ORDER BY created_at DESC,id DESC LIMIT 200")
    .bind(actor.companyId).all()).results;
  const policies=(await db.prepare(`SELECT DISTINCT ON (department_id) department_id,version,steps_json,published_by,reason,created_at
    FROM department_flow_versions WHERE company_id=? ORDER BY department_id,version DESC`)
    .bind(actor.companyId).all()).results;
  const departments=(await db.prepare("SELECT id,display_name FROM organization_nodes WHERE company_id=? AND kind='department' AND status='active' ORDER BY display_name").bind(actor.companyId).all<{id:string;display_name:string}>()).results;
  // Reuse the seats already loaded for this overview. Checking each department
  // with canPublishFlow repeats actor and seat queries for every row and can
  // exhaust the small web pool on large organizations.
  const publisherPersonId=(await personForActor(db,actor))?.id;
  const activePublisher=publisherPersonId ? !!await activeStaff(db,actor.companyId,actor.userId).catch(()=>null) : false;
  const currentSeats=seats as Seat[];
  const headSeats=new Map(currentSeats.filter(row=>row.kind==="head").map(row=>[row.scope_id,row]));
  const ceoPersonId=currentSeats.find(row=>row.kind==="ceo" && row.scope_id===actor.companyId)?.person_id;
  const canPublishDepartmentIds=activePublisher && publisherPersonId
    ? departments.filter(department=>{
      const headPersonId=headSeats.get(department.id)?.person_id;
      return headPersonId===publisherPersonId || (!headPersonId && ceoPersonId===publisherPersonId);
    }).map(department=>department.id)
    : [];
  if(canAppointKinds.length)return {seats,history,policies,departments,canAppointKinds,canPublishDepartmentIds};
  const visible=new Set(canPublishDepartmentIds);
  return {
    seats:seats.filter(row=>row.kind==="head" && visible.has(String(row.scope_id))),
    history:history.filter(row=>row.kind==="head" && visible.has(String(row.scope_id))),
    policies:policies.filter(row=>visible.has(String(row.department_id))),
    departments:departments.filter(row=>visible.has(row.id)),
    canAppointKinds,canPublishDepartmentIds,
  };
}
export async function changeAppointment(db:SqlDatabase,actor:WorkContext,raw:unknown,key:string){
  const input=appointmentInput.parse(raw),scopeId=input.kind==="head"?input.departmentId:actor.companyId;
  if(!scopeId || input.kind!=="head" && input.departmentId)throw new ApiError(422,"APPOINTMENT_SCOPE","Choose a department only for its head.");
  const operation="authority.appointment.change",recordId=`${input.kind}:${scopeId}`;
  const prior=await replayCommand(db,actor,key,operation,recordId,input);if(prior)return prior;
  const power=await appointmentPower(db,actor,input.kind);
  if(!power)throw new ApiError(403,"APPOINTMENT_AUTHORITY","The current owner, CEO or HR appointee must make this appointment.");
  const current=await seat(db,actor.companyId,input.kind,scopeId);
  if((current?.version??0)!==input.expectedVersion)throw new ApiError(409,"VERSION_CONFLICT","The appointment changed. Reload it before editing.");
  if(input.kind==="head"){
    const department=await db.prepare("SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='department' AND status='active'").bind(scopeId,actor.companyId).first();
    if(!department)throw new ApiError(404,"DEPARTMENT_NOT_FOUND","Active department not found.");
  }
  if(input.kind==="ceo" && !input.positionId)throw new ApiError(422,"CEO_POSITION_REQUIRED","Choose the CEO position.");
  if(input.positionId){
    const position=await db.prepare("SELECT id FROM organization_positions WHERE id=? AND company_id=? AND active=1 AND (?<>'ceo' OR is_ceo=1) AND (?<>'head' OR department_id=?)").bind(input.positionId,actor.companyId,input.kind,input.kind,scopeId).first();
    if(!position)throw new ApiError(422,"POSITION_INVALID","Choose an active position in this company and department.");
  }
  const selfCeo=input.kind==="ceo" && power==="owner" && !!input.personId && (await personForActor(db,actor))?.id===input.personId;
  if(input.personId){
    const person=await db.prepare(`SELECT p.user_id FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
      WHERE p.id=? AND p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND (?::text IS NULL OR e.position_id=? OR ?)
      AND (?<>'head' OR e.department_id=?)`).bind(input.personId,actor.companyId,input.positionId,input.positionId,selfCeo,input.kind,scopeId).first<{user_id:string}>();
    if(!person?.user_id)throw new ApiError(422,"APPOINTEE_UNAVAILABLE","Choose an active staff member occupying that position.");
    await activeStaff(db,actor.companyId,person.user_id);
  }
  const now=Date.now(),effectiveAt=input.effectiveAt??now;
  if(effectiveAt>now || effectiveAt<now-300000)throw new ApiError(422,"EFFECTIVE_AT_INVALID","Appointments take effect now; use a recent effective time.");
  const version=input.expectedVersion+1;
  return executeCommand(db,actor,key,operation,input,recordId,version,{kind:input.kind,scopeId,version,personId:input.personId,positionId:input.positionId},[
    powerGuard(db,key,actor,power),
    assertion(db,key,"appointment-version",`(?=0 AND NOT EXISTS(SELECT 1 FROM authority_seats WHERE company_id=? AND kind=? AND scope_id=?)) OR EXISTS(SELECT 1 FROM authority_seats WHERE company_id=? AND kind=? AND scope_id=? AND version=?)`,[input.expectedVersion,actor.companyId,input.kind,scopeId,actor.companyId,input.kind,scopeId,input.expectedVersion]),
    db.prepare(`INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at) VALUES(?,?,?,?,?,?,?)
      ON CONFLICT(company_id,kind,scope_id) DO UPDATE SET version=excluded.version,position_id=excluded.position_id,person_id=excluded.person_id,effective_at=excluded.effective_at`)
      .bind(actor.companyId,input.kind,scopeId,version,input.positionId,input.personId,effectiveAt),
    db.prepare(`INSERT INTO authority_appointments(id,company_id,kind,scope_id,version,position_id,person_id,effective_at,appointed_by,reason,provenance,command_id,created_at)
      VALUES(?,?,?,?,?,?,?,?,?,?,'explicit',?,?)`).bind(`appointment-${key}`,actor.companyId,input.kind,scopeId,version,input.positionId,input.personId,effectiveAt,actor.userId,input.reason,key,now),
    ...(selfCeo?[db.prepare(`UPDATE hr_engagements e SET position_id=p.id,level_id=p.level_id,manager_person_id=NULL,branch_id=NULL,department_id=NULL,job_title=p.title,version=e.version+1,updated_at=?
      FROM organization_positions p WHERE p.id=? AND p.company_id=e.company_id AND e.company_id=? AND e.person_id=? AND e.status='active'`).bind(now,input.positionId,actor.companyId,input.personId)]:[]),
    ...(input.kind==="head" && input.personId ? [db.prepare(`INSERT INTO department_heads(department_id,company_id,person_id,version,updated_by,updated_at)
      VALUES(?,?,?,?,?,?) ON CONFLICT(department_id) DO UPDATE SET person_id=excluded.person_id,version=department_heads.version+1,updated_by=excluded.updated_by,updated_at=excluded.updated_at`)
      .bind(scopeId,actor.companyId,input.personId,version,actor.userId,now)]:[]),
  ],{companyId:actor.companyId,branchId:null,departmentId:input.kind==="head"?scopeId:null});
}

async function canPublishFlow(db:SqlDatabase,actor:WorkContext,departmentId:string){
  const person=await personForActor(db,actor);
  const head=await seat(db,actor.companyId,"head",departmentId);
  if(person && head?.person_id===person.id && await activeStaff(db,actor.companyId,actor.userId).catch(()=>null))return "head";
  if(!head?.person_id){
    const ceo=await seat(db,actor.companyId,"ceo",actor.companyId);
    if(person && ceo?.person_id===person.id && await activeStaff(db,actor.companyId,actor.userId).catch(()=>null))return "ceo";
  }
  return null;
}
export async function publishDepartmentFlow(db:SqlDatabase,actor:WorkContext,raw:unknown,key:string){
  const input=departmentFlowInput.parse(raw),operation="department.flow.publish",recordId=input.departmentId;
  const prior=await replayCommand(db,actor,key,operation,recordId,input);if(prior)return prior;
  const authority=await canPublishFlow(db,actor,input.departmentId);
  if(!authority)throw new ApiError(403,"DEPARTMENT_HEAD_REQUIRED","The appointed head, or CEO while the head seat is vacant, publishes this flow.");
  const node=await db.prepare("SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='department' AND status='active'").bind(input.departmentId,actor.companyId).first();
  if(!node)throw new ApiError(404,"DEPARTMENT_NOT_FOUND","Active department not found.");
  const previous=await db.prepare("SELECT COALESCE(MAX(version),0) version FROM department_flow_versions WHERE company_id=? AND department_id=?").bind(actor.companyId,input.departmentId).first<{version:number}>();
  if(previous?.version!==input.expectedVersion)throw new ApiError(409,"VERSION_CONFLICT","The flow changed. Reload it before publishing.");
  if(new Set(input.steps.map(s=>s.kind+":"+(s.kind==="person"?s.personId:s.positionId))).size!==input.steps.length)throw new ApiError(422,"DUPLICATE_APPROVER","Choose each person or position once.");
  for(const step of input.steps){
    if(step.kind==="person"){
      const p=await db.prepare("SELECT user_id FROM hr_people WHERE id=? AND company_id=? AND status='active' AND merged_into_id IS NULL").bind(step.personId,actor.companyId).first<{user_id:string}>();
      if(!p?.user_id)throw new ApiError(422,"APPROVER_UNAVAILABLE","Choose a person with an active account.");
      await activeStaff(db,actor.companyId,p.user_id);
    } else {
      const p=await db.prepare("SELECT 1 FROM organization_positions WHERE id=? AND company_id=? AND active=1").bind(step.positionId,actor.companyId).first();
      if(!p)throw new ApiError(422,"POSITION_INVALID","Choose an active position.");
    }
  }
  const version=input.expectedVersion+1;
  return executeCommand(db,actor,key,operation,input,recordId,version,{departmentId:recordId,version,steps:input.steps},[
    assertion(db,key,"flow-publisher",`EXISTS(SELECT 1 FROM authority_seats s JOIN hr_people p ON p.id=s.person_id AND p.company_id=s.company_id JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active' WHERE s.company_id=? AND s.kind=? AND s.scope_id=? AND iam_same_person(p.user_id,?,s.company_id) AND s.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`,[actor.companyId,authority,authority==="head"?input.departmentId:actor.companyId,actor.userId]),
    assertion(db,key,"flow-version","(SELECT COALESCE(MAX(version),0) FROM department_flow_versions WHERE company_id=? AND department_id=?)=?",[actor.companyId,input.departmentId,input.expectedVersion]),
    db.prepare("INSERT INTO department_flow_versions(id,company_id,department_id,version,steps_json,published_by,reason,command_id,created_at) VALUES(?,?,?,?,?::jsonb,?,?,?,?)")
      .bind(`department-flow-${key}`,actor.companyId,input.departmentId,version,JSON.stringify(input.steps),actor.userId,input.reason,key,Date.now()),
  ],{companyId:actor.companyId,branchId:null,departmentId:input.departmentId});
}

function parsedSteps(value:Policy["steps_json"]):FlowSelector[]{return typeof value==="string"?JSON.parse(value) as FlowSelector[]:value;}
export async function departmentRoute(db:SqlDatabase,companyId:string,userId:string,excluded:string[]=[],proposed?:z.infer<typeof departmentFlowInput>["steps"]):Promise<ApprovalRoute>{
  const requester=await activeStaff(db,companyId,userId);
  const scope={company_id:companyId,branch_id:requester.branch_id,department_id:requester.department_id};
  if(!requester.department_id)return {scope,steps:[],blockedReason:"Assign the requester to a department."};
  const policy=await db.prepare("SELECT id,version,steps_json FROM department_flow_versions WHERE company_id=? AND department_id=? ORDER BY version DESC LIMIT 1")
    .bind(companyId,requester.department_id).first<Policy>();
  const recordedHead=await seat(db,companyId,"head",requester.department_id);
  const legacyHead=!recordedHead?await db.prepare("SELECT person_id FROM department_heads WHERE company_id=? AND department_id=?").bind(companyId,requester.department_id).first<{person_id:string}>():null;
  const head=recordedHead??(legacyHead?{person_id:legacyHead.person_id}:null);
  const selectors=proposed??(policy?parsedSteps(policy.steps_json):head?.person_id?[{kind:"person" as const,personId:head.person_id}]:[]);
  if(!selectors.length)return {scope,steps:[],blockedReason:"Appoint a department head or publish an acting flow."};
  const people=await staffPeople(db,companyId);
  const forbidden=new Set([userId,...excluded]);
  const steps:RouteStep[]=[];
  const checks:NonNullable<ApprovalRoute["checks"]>=[];
  if(policy)checks.push({query:"EXISTS(SELECT 1 FROM department_flow_versions WHERE id=? AND company_id=? AND version=?)",args:[policy.id,companyId,policy.version]});
  try{
    for(const [ordinal,selector] of selectors.entries()){
      const matches=selector.kind==="person"?people.filter(p=>p.person_id===selector.personId):people.filter(p=>p.position_id===selector.positionId);
      if(matches.length!==1)throw new ApiError(409,"APPROVER_UNAVAILABLE",`Step ${ordinal+1} needs exactly one active position holder and account.`);
      let chosen=matches[0];
      if(forbidden.has(chosen.user_id) || chosen.person_id===requester.person_id){
        const chain=reportingChain(people,chosen.person_id);
        const above=chain.find(p=>p.level_number!=null && chosen.level_number!=null && p.level_number<chosen.level_number && !forbidden.has(p.user_id));
        if(above)chosen=above;
        else {
          const ceo=await seat(db,companyId,"ceo",companyId);
          const ceoPerson=people.find(p=>p.person_id===ceo?.person_id && !forbidden.has(p.user_id));
          if(!ceoPerson)throw new ApiError(409,"INDEPENDENT_APPROVER_REQUIRED","Appoint a higher reporting manager or independent CEO for this request.");
          chosen=ceoPerson;
        }
      }
      if(forbidden.has(chosen.user_id))throw new ApiError(409,"SELF_APPROVAL","The requester or preparer cannot approve this request.");
      forbidden.add(chosen.user_id);
      steps.push({userId:chosen.user_id,authority:"department",candidateUserIds:[chosen.user_id],stage:"department_approval",snapshot:{departmentId:requester.department_id,departmentPolicyId:policy?.id??null,departmentPolicyVersion:policy?.version??0,selector,selectedPersonId:chosen.person_id,selectedPositionId:chosen.position_id,originalPersonId:matches[0].person_id}});
      checks.push({query:"EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND status='active')",args:[chosen.engagement_id,companyId,chosen.engagement_version]});
    }
    return {scope,steps,checks,blockedReason:null};
  }catch(error){if(!(error instanceof ApiError))throw error;return {scope,steps:[],blockedReason:error.message};}
}

export async function departmentStepEligible(db:SqlDatabase,companyId:string,userId:string,snapshot:Record<string,unknown>){
  const person=await activeStaff(db,companyId,userId).catch(()=>null);
  if(!person || person.person_id!==snapshot.selectedPersonId)return false;
  if(snapshot.selector && typeof snapshot.selector==="object" && (snapshot.selector as FlowSelector).kind==="position")
    return person.position_id===(snapshot.selector as {positionId:string}).positionId;
  if(!snapshot.rerouted && !snapshot.departmentPolicyId && snapshot.originalPersonId===snapshot.selectedPersonId){
    const current=await seat(db,companyId,"head",String(snapshot.departmentId));
    const legacy=!current?await db.prepare("SELECT person_id FROM department_heads WHERE company_id=? AND department_id=?").bind(companyId,String(snapshot.departmentId)).first<{person_id:string}>():null;
    if((current?.person_id??legacy?.person_id)!==person.person_id)return false;
  }
  return true;
}
export async function appointedHrEligible(db:SqlDatabase,companyId:string,userId:string,personId:unknown){
  if(typeof personId!=="string")return false;
  const current=await seat(db,companyId,"hr",companyId);
  if(current?.person_id!==personId || !current.effective_at || current.effective_at>Date.now())return false;
  const person=await activeStaff(db,companyId,userId).catch(()=>null);
  return person?.person_id===personId;
}

/** New submissions use the department policy before independent HR review. */
export async function peopleRequestRoute(db:SqlDatabase,companyId:string,userId:string,excluded:string[]=[]):Promise<ApprovalRoute>{
  const department=await departmentRoute(db,companyId,userId,excluded);
  if(department.blockedReason)return department;
  const forbidden=new Set([userId,...excluded,...department.steps.map(s=>s.userId)]);
  const appointed=await seat(db,companyId,"hr",companyId);
  const hrPerson=appointed?.person_id?(await staffPeople(db,companyId)).find(p=>p.person_id===appointed.person_id):null;
  const legacy=await eligibleRoleUsers(db,department.scope,"hr_admin");
  const reviewer=hrPerson?.user_id && !forbidden.has(hrPerson.user_id)?hrPerson.user_id:legacy.find(id=>!forbidden.has(id));
  if(!reviewer)return {...department,steps:[],blockedReason:"Appoint an independent HR reviewer for this request."};
  const hrStep:RouteStep={userId:reviewer,authority:"hr_admin",stage:"hr_approval",snapshot:{appointmentKind:"hr",appointedPersonId:hrPerson?.user_id===reviewer?hrPerson.person_id:null}};
  return {...department,steps:[...department.steps,hrStep]};
}

export async function previewDepartmentFlow(db:SqlDatabase,actor:WorkContext,raw:unknown){
 const input=departmentFlowInput.parse(raw);
 if(!await canPublishFlow(db,actor,input.departmentId))throw new ApiError(403,"DEPARTMENT_HEAD_REQUIRED","The appointed head, or CEO while vacant, publishes this flow.");
 const latest=await db.prepare("SELECT COALESCE(MAX(version),0) version FROM department_flow_versions WHERE company_id=? AND department_id=?").bind(actor.companyId,input.departmentId).first<{version:number}>();
 if(Number(latest?.version)!==input.expectedVersion)throw new ApiError(409,"VERSION_CONFLICT","The flow changed. Reload before reviewing.");
 const people=await staffPeople(db,actor.companyId),steps=[],blockers:string[]=[];
 for(const [i,selector] of input.steps.entries()){
 const matches=people.filter(p=>selector.kind==="person"?p.person_id===selector.personId:p.position_id===selector.positionId);
 if(matches.length!==1){blockers.push(`Step ${i+1} requires one active holder with an account.`);continue;}
 const person=matches[0];
 steps.push({name:person.legal_name,selector});}
 const simulations=[];
 for(const requester of people.filter(p=>p.department_id===input.departmentId)){
  const route=await departmentRoute(db,actor.companyId,requester.user_id,[],input.steps);
  simulations.push({requester:requester.legal_name,reviewers:route.steps.map(step=>people.find(p=>p.user_id===step.userId)?.legal_name??"Recorded reviewer"),blocker:route.blockedReason});
 }
 return {steps,blockers,simulations,previousVersion:input.expectedVersion,newVersion:input.expectedVersion+1,previousSteps:(await db.prepare("SELECT steps_json FROM department_flow_versions WHERE company_id=? AND department_id=? ORDER BY version DESC LIMIT 1").bind(actor.companyId,input.departmentId).first<{steps_json:unknown}>())?.steps_json??[],effect:"New requests only. Self-review escalation is resolved again for each requester on submission."};
}
