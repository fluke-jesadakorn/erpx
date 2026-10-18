import {activeBenefitPerson,benefitElectionEmployeeName} from "../../people_payroll/public/benefits-owner-facts";
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { hasPermission } from "@/lib/identity_access/permissions";
import { hasModuleAccess } from "@/data/module-access";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { ApiError } from "@/server/modules/runtime/public/http";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { prepareAttachmentBinding } from "@/server/modules/files/public/attachments/service";
import { prepareLearningBenefitConsumption, validateBenefitLearning } from "@/server/modules/learning/public/learning-eligibility";
import { BusinessRuleError } from "../../shared";
import { createBenefitsApplication, type BenefitsPorts, type BenefitMutation } from "../application/benefits";
import {benefitElectionSchema,benefitTransitionSchema,type BenefitRow,type BenefitRecordKind} from "../domain/benefits";
import {withBenefitPurchasedRead,withBenefitPurchasedCommand} from "./purchases";
import {benefitPresentationContext} from "./presentation";
export { createBenefitPlan } from "./plan";
type LearningPrepared={guards:SqlStatement[];certificateId?:string;receipt?:SqlStatement};
type EvidencePrepared=Awaited<ReturnType<typeof prepareAttachmentBinding>>;
const tables={plan:"benefit_plans",window:"benefit_enrollment_windows",dependent:"benefit_dependents",election:"benefit_elections"} as const;
const events={plan:"benefit_plan_events",window:"benefit_window_events",dependent:"benefit_dependent_events",election:"benefit_election_events"} as const;
function access(a:WorkContext,write=false){requirePermission(a,write?"records.write":"records.read");if(!hasModuleAccess(a.modules,"benefits"))throw new ApiError(403,"MODULE_FORBIDDEN","Your current role cannot access Benefits.");}
function application(a:WorkContext,provided?:SqlDatabase){
 let resource:Promise<SqlDatabase>|null=null;const database=()=>provided?Promise.resolve(provided):resource??=holdingResources(a.holdingId).then(x=>x.write);
 const person=async()=>activeBenefitPerson(await database(),a.companyId,a.userId);
 const rows=async(sql:string,...bind:(string|number|boolean|null)[])=>((await database()).prepare(sql).bind(...bind).all<BenefitRow>()).then(x=>x.results);
 const ports:BenefitsPorts<LearningPrepared,EvidencePrepared>={
 actorId:a.userId,clock:{now:()=>Date.now()},canRead:hasModuleAccess(a.modules,"benefits")&&hasPermission(a,"records.read"),canReview:hasPermission(a,"benefits.review"),canWrite:hasPermission(a,"records.write"),canManageDocuments:hasPermission(a,"benefits.documents.manage"),
 async authorize(write){access(a,write);},async authorizeReview(){requirePermission(a,"benefits.review");},
 replay:async(key,op,id,input)=>replayCommand(await database(),a,key,op,id,input),person,
 plans:()=>rows("SELECT * FROM benefit_plans WHERE company_id=? ORDER BY updated_at DESC",a.companyId),
 windows:()=>rows("SELECT * FROM benefit_enrollment_windows WHERE company_id=? ORDER BY updated_at DESC",a.companyId),
 dependents:(personId)=>rows("SELECT * FROM benefit_dependents WHERE company_id=? AND person_id=? ORDER BY updated_at DESC",a.companyId,personId),
 elections:(personId,reviewer,activeOnly=false)=>rows(`SELECT e.*,p.name plan_name,(${benefitElectionEmployeeName()}) employee_name FROM benefit_elections e JOIN benefit_plans p ON p.id=e.plan_id WHERE e.company_id=? ${activeOnly?"AND e.status NOT IN ('released','cancelled')":""} AND (? OR e.person_id=?) ORDER BY e.updated_at DESC`,a.companyId,reviewer,personId),
 plan:async(id,version)=>(await database()).prepare("SELECT * FROM benefit_plans WHERE id=? AND company_id=? AND version=? AND status='published'").bind(id,a.companyId,version).first<BenefitRow>(),
 window:async(id,version)=>(await database()).prepare("SELECT * FROM benefit_enrollment_windows WHERE id=? AND company_id=? AND version=? AND status='open'").bind(id,a.companyId,version).first<BenefitRow>(),
 verifiedDependents:async(personId)=>((await database()).prepare("SELECT id FROM benefit_dependents WHERE company_id=? AND person_id=? AND status='verified'").bind(a.companyId,personId).all<{id:string}>()).then(x=>x.results.map(y=>y.id)),
 record:async(kind,id)=>(await database()).prepare(`SELECT * FROM ${tables[kind]} WHERE id=? AND company_id=?`).bind(id,a.companyId).first<BenefitRow>(),
 election:async(id,personId,reviewer)=>(await database()).prepare(`SELECT e.*,p.name plan_name,p.version plan_current_version,p.status plan_status,(${benefitElectionEmployeeName()}) employee_name FROM benefit_elections e JOIN benefit_plans p ON p.id=e.plan_id WHERE e.id=? AND e.company_id=? AND (? OR e.person_id=?)`).bind(id,a.companyId,reviewer,personId).first<BenefitRow>(),
 history:(id)=>rows("SELECT * FROM benefit_election_events WHERE election_id=? ORDER BY version",id),
 payroll:async(id)=>(await database()).prepare("SELECT * FROM benefit_payroll_inputs WHERE election_id=?").bind(id).first<BenefitRow>(),
 prepareLearning:async(input)=>prepareLearningBenefitConsumption(await database(),a,input),
 validateLearning:async(row,key)=>({guards:await validateBenefitLearning(await database(),a,{id:String(row.id),person_id:String(row.person_id),learning_certificate_id:row.learning_certificate_id?String(row.learning_certificate_id):null,effective_from:String(row.effective_from),plan_id:String(row.plan_id)},key)}),
 prepareEvidence:async(id,_kind,input,policy,key)=>prepareAttachmentBinding(await database(),a,{attachmentSetIds:input.attachmentSetIds,bindingContract:{targetType:policy.targetType,bindingType:policy.bindingType,contexts:[{purposeKey:policy.purpose,componentKind:"action",componentRef:policy.componentRef}]},targetId:id,expectedTargetVersion:input.expectedVersion,commandId:key,bindingId:id}),
 commit:async(mutation,result)=>commit(await database(),a,mutation,result),
 };return createBenefitsApplication(ports);
}
async function commit<Result>(write:SqlDatabase,a:WorkContext,m:BenefitMutation<LearningPrepared,EvidencePrepared>,result:Result):Promise<Result>{
 const {id,key,operation:op,now}=m;
 switch(m.action){
case "window":{const i=m.input;return executeCommand(write, a, key, op, i, id, 1, result, [
    write
      .prepare(
        "INSERT INTO benefit_enrollment_windows VALUES(?,?,?,?,?,?,'draft',?,NULL,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        i.name,
        i.opensOn,
        i.closesOn,
        i.effectiveOn,
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO benefit_window_events VALUES(?,?,1,'create',?,'Window drafted for independent review','draft',?)",
      )
      .bind(`benefit-window-event-${key}`, id, a.userId, now),
  ]);}
case "dependent":{const i=m.input,p=m.person;return executeCommand(write, a, key, op, i, id, 1, result, [
    write
      .prepare(
        "INSERT INTO benefit_dependents VALUES(?,?,?,?,?,?,'pending',?,NULL,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        p.id,
        i.name,
        i.relationship,
        i.birthDate,
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO benefit_dependent_events VALUES(?,?,1,'create',?,'Dependent submitted for verification','pending',?)",
      )
      .bind(`benefit-dependent-event-${key}`, id, a.userId, now),
  ]);}
case "election":{const i=m.input,p=m.person,plan=m.plan,win=m.window,eligibility=m.learning;return executeCommand(write, a, key, op, i, id, 1, result, [
    ...(eligibility?.guards??[]),
    write
      .prepare(
        "INSERT INTO benefit_elections(id,company_id,person_id,engagement_id,plan_id,plan_version,window_id,window_version,coverage_type,dependent_ids_json,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,status,created_by,version,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'elected',?,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        p.id,
        p.engagement_id,
        i.planId,
        i.planVersion,
        i.windowId,
        i.windowVersion,
        plan.coverage_type,
        JSON.stringify(i.dependentIds),
        plan.currency,
        plan.employee_deduction_minor,
        plan.employer_cost_minor,
        win.effective_on,
        plan.effective_to,
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO benefit_election_events VALUES(?,?,1,'elect',?,'Employee elected reviewed coverage','elected',?)",
      )
      .bind(`benefit-election-event-${key}`, id, a.userId, now),
    ...(eligibility?[write.prepare("UPDATE benefit_elections SET learning_certificate_id=? WHERE id=?").bind(eligibility.certificateId!,id),eligibility.receipt!]:[]),
  ]);}
case "transition":{const i=m.input,kind=m.kind,table=tables[kind],eventTable=events[kind],attachmentBinding=m.evidence;const now = m.now,
    next = i.expectedVersion + 1,
    sets = ["status=?", "version=version+1", "updated_at=?"],
    bind: (string | number | null)[] = [m.to, now];
  const columns={submittedBy:"submitted_by",providerReference:"provider_reference",acknowledgedBy:"acknowledged_by",releasedBy:"released_by",reviewedBy:"reviewed_by"} as const;
  for(const [field,column] of Object.entries(columns)){
    if(field in m.changes){sets.push(`${column}=?`);bind.push(m.changes[field as keyof typeof columns]??null);}
  }
  bind.push(id);
  const statements: SqlStatement[] = [
    ...(m.learning?.guards ?? []),
    ...(attachmentBinding?.guards ?? []),
    assertion(
      write,
      key,
      "record",
      `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=? AND status=?)`,
      [id, a.companyId, i.expectedVersion, m.from],
    ),
    write
      .prepare(`UPDATE ${table} SET ${sets.join(",")} WHERE id=?`)
      .bind(...bind),
    write
      .prepare(`INSERT INTO ${eventTable} VALUES(?,?,?,?,?,?,?,?)`)
      .bind(
        `benefit-${kind}-event-${key}`,
        id,
        next,
        i.action,
        a.userId,
        i.reason,
        m.to,
        now,
      ),
    ...(attachmentBinding?.steps ?? []),
  ];
  if (m.releasePayroll)
    statements.push(
      write
        .prepare(
          "INSERT INTO benefit_payroll_inputs(id,election_id,company_id,person_id,engagement_id,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,source_election_version,status,released_by,created_at) SELECT ?,id,company_id,person_id,engagement_id,currency,employee_deduction_minor,employer_cost_minor,effective_from,effective_to,?,'released',?,? FROM benefit_elections WHERE id=?",
        )
        .bind(`benefit-payroll-${key}`, next, a.userId, now, id),
    );
  return executeCommand(
    write,
    a,
    key,
    op,
    i,
    id,
    next,
    result,
    statements,
  );}
 }
}
async function translated<Result>(action:()=>Promise<Result>):Promise<Result>{try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function benefitsWorkspace(a:WorkContext,personal=false){return translated(()=>withBenefitPurchasedRead(a,async(database,live,purchase)=>({...await application(live,database).workspace(personal),workContext:benefitPresentationContext(live),...(purchase?{purchase}:{})})));}
export function createBenefitWindow(a:WorkContext,raw:unknown,key:string){return translated(()=>withBenefitPurchasedCommand(a,{key,operation:"benefits.window.create",target:{kind:"window",id:`benefit-window-${key}`},purpose:"new-work",review:true},database=>application(a,database).createWindow(raw,key)));}
export function createBenefitDependent(a:WorkContext,raw:unknown,key:string){return translated(()=>withBenefitPurchasedCommand(a,{key,operation:"benefits.dependent.create",target:{kind:"dependent",id:`benefit-dependent-${key}`},purpose:"new-work",employee:true},database=>application(a,database).createDependent(raw,key)));}
export function createBenefitElection(a:WorkContext,raw:unknown,key:string){return translated(()=>{const input=benefitElectionSchema.parse(raw);return withBenefitPurchasedCommand(a,{key,operation:"benefits.election.create",target:{kind:"election",id:`benefit-election-${key}`},purpose:"new-work",employee:true,references:{planId:input.planId,windowId:input.windowId,dependentIds:input.dependentIds}},database=>application(a,database).createElection(input,key));});}
export function changeBenefitRecord(a:WorkContext,kind:BenefitRecordKind,id:string,raw:unknown,key:string){return translated(()=>{const input=benefitTransitionSchema.parse(raw);return withBenefitPurchasedCommand(a,{key,operation:`benefits.${kind}.${input.action}`,target:{kind,id},purpose:["publish","open"].includes(input.action)?"new-work":"settle-existing",existing:true,review:kind!=="election"||input.action!=="submit"},database=>application(a,database).change(kind,id,input,key));});}
export function benefitDetail(a:WorkContext,id:string){return translated(()=>withBenefitPurchasedRead(a,async(database,live,purchase)=>({...await application(live,database).detail(id),canWrite:hasPermission(live,"records.write"),workContext:benefitPresentationContext(live),...(purchase?{purchase}:{})})));}
export function benefitWork(a:WorkContext){return translated(()=>withBenefitPurchasedRead(a,async(database,live,purchase)=>(await application(live,database).work()).map(item=>purchase&&!purchase.settleExisting?{...item,canAct:false,queue:"waiting" as const,relationships:item.relationships?.filter(value=>value!=="approval")}:item),()=>[]));}
