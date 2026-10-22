import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {learningAssignmentNotice} from "@erpx/server-collaboration/public/learning-notification-effects";
import {learningTalentSnapshotSources as identityaccessSnapshotSources} from '@erpx/server-identity_access/public/learning-talent-snapshot-sources';
import {learningTalentSnapshotSources as peoplepayrollSnapshotSources} from '@erpx/server-people_payroll/public/learning-talent-snapshot-sources';
import {learningWorkerChoices,learningWorkerPlacement,learningWorkerReferenceWitness} from '@erpx/server-people_payroll/public/learning-worker-references';
const identityaccessSources=identityaccessSnapshotSources();
const peoplepayrollSources=peoplepayrollSnapshotSources();
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { Row } from "@erpx/server-commerce/public/commerce";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { ApiError } from "@erpx/server-runtime/public/http";
import { prepareAttachmentBinding } from "@erpx/server-files/public/attachments/service";
import { createLearningWorkspace, createChangeLearningCourse, createLearningAssignment as createLearningAssignmentApplication, createChangeLearningAssignment, createLearningDetail, createLearningExecutiveEvidence, createLearningWork, type LearningCommandPorts, type LearningCommand, type LearningMutation, type LearningExecutiveEvidenceMetric } from "@erpx/server-learning/application/learning";
import type { LearningAssignment as Assignment } from "@erpx/server-learning/domain/learning";
import {courseActionSchema,learningAssignSchema,learningActionSchema} from "@erpx/server-learning/domain/learning";
import {withLearningPurchasedCommand,withLearningPurchasedRead,requireLearningPurchasedRead,type LearningCapability,type LearningPurchaseControls} from "@erpx/server-learning/adapters/purchases";
export type { LearningExecutiveEvidenceMetric } from "@erpx/server-learning/application/learning";
const scope =
    "a.company_id=? AND (?::text IS NULL OR a.branch_id=?) AND (?::text IS NULL OR a.department_id=?)",
  args = (a: WorkContext) => [
    a.companyId,
    a.branchId,
    a.branchId,
    a.departmentId,
    a.departmentId,
  ];
const learnerEligible = `EXISTS(SELECT 1 ${identityaccessSources.learning_adapters_learning_assignments_x_from} ${peoplepayrollSources.learning_adapters_learning_hr_people_p_join} ON p.user_id=x.user_id AND p.company_id=x.company_id ${peoplepayrollSources.learning_adapters_learning_hr_engagements_e_join} ON e.person_id=p.id AND e.company_id=p.company_id WHERE x.company_id=a.company_id AND x.user_id=a.learner_id AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=a.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=a.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active')`;
function access(a: WorkContext, write = false) {
  requirePermission(a, write ? "records.write" : "records.read");
  if (!hasModuleAccess(a.modules, "learning"))
    throw new ApiError(
      403,
      "MODULE_FORBIDDEN",
      "Your current role cannot access Learning.",
    );
}
type DatabaseSource = SqlDatabase | (() => Promise<SqlDatabase>);
const databaseSource = (source: DatabaseSource) => typeof source === "function" ? source : async () => source;
function lazyDatabase(a: WorkContext) { let resource: Promise<SqlDatabase> | null = null; return () => resource ??= holdingResources(a.holdingId).then(resources => resources.write); }
function commandPorts(database: () => Promise<SqlDatabase>): LearningCommandPorts {
 return { authorize: access, reviewer: a => { requirePermission(a, "records.review"); }, error: violation => new ApiError(violation.status, violation.code, violation.message), now: Date.now, today: () => new Date().toISOString().slice(0, 10),
 replay: async (a, key, op, id, input) => replayCommand(await database(), a, key, op, id, input),
 commit: async (command: LearningCommand, mutation: LearningMutation) => executeCommand(await database(), command.actor, command.key, command.operation, command.input, command.id, command.version, command.result, mutation.native as Parameters<typeof executeCommand>[8]),
 };
}
export function nativeLearningWorkspace(source: DatabaseSource) {
 const database = databaseSource(source);
 const workspace=(database:()=>Promise<SqlDatabase>,purchases:LearningPurchaseControls|undefined)=>createLearningWorkspace({ authorize: access,
 async courses(a) { if(purchases&&!purchases["learn.courses"]?.history)return [];const write = await database(); return (
      await write
        .prepare(
          "SELECT * FROM learning_courses WHERE company_id=? ORDER BY updated_at DESC",
        )
        .bind(a.companyId)
        .all<Row>()
    ).results; },
 async assignments(a, personal) { const write = await database(); return (
      await write
        .prepare(
          `SELECT ${!purchases||purchases["learn.results"]?.history?"a.*":"a.id,a.company_id,a.branch_id,a.department_id,a.course_id,a.course_version,a.learner_id,a.due_date,a.status,a.created_by,a.version,a.created_at,a.updated_at,NULL::text attendance,NULL::bigint score_bps,NULL::text result_evidence,NULL::text completion_reason"},c.title course_title,c.required_for,(SELECT legal_name ${peoplepayrollSources.learning_adapters_learning_hr_people_from} WHERE company_id=a.company_id AND user_id=a.learner_id AND merged_into_id IS NULL LIMIT 1) learner_name,CASE WHEN ${learnerEligible} THEN 1 ELSE 0 END learner_eligible FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id WHERE ${scope} AND (NOT ?::boolean OR a.learner_id=?) ORDER BY a.updated_at DESC`,
        )
        .bind(...args(a),personal,a.userId)
        .all<Assignment>()
    ).results; },
 async people(a) { if(purchases&&!purchases["people.learner-reference"]?.history)return [];return learningWorkerChoices(await database(),a); },
 });
 return async(a:WorkContext,personal=false)=>{access(a);return withLearningPurchasedRead(await database(),a,async write=>{const purchases=await requireLearningPurchasedRead(write,a);return {...await workspace(async()=>write,purchases)(a,personal),...(purchases?{purchases}:{})};});};
}
export async function learningWorkspace(a: WorkContext, personal = false) { return nativeLearningWorkspace(lazyDatabase(a))(a, personal); }
export { createLearningCourse } from "@erpx/server-learning/adapters/course";
function learningCourseChanges(source: DatabaseSource) {
 const database = databaseSource(source); return createChangeLearningCourse({ ...commandPorts(database),
 async course(a,id) { const write = await database(); return await write
      .prepare("SELECT * FROM learning_courses WHERE id=? AND company_id=?")
      .bind(id, a.companyId)
      .first<Row>(); },
 async prepare(command, input, expected, status) { const write = await database(), { actor: a, key, id, now } = command; return { native: [
      assertion(
        write,
        key,
        "course",
        "EXISTS(SELECT 1 FROM learning_courses WHERE id=? AND company_id=? AND version=? AND status=? AND created_by<>?)",
        [id, a.companyId, input.expectedVersion, expected, a.userId],
      ),
      write
        .prepare(
          "UPDATE learning_courses SET status=?,reviewed_by=?,reviewed_at=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(status, a.userId, now, now, id),
      write
        .prepare("INSERT INTO learning_course_events VALUES(?,?,?,?,?,?,?,?)")
        .bind(
          `learning-course-event-${key}`,
          id,
          input.expectedVersion + 1,
          input.action,
          a.userId,
          input.reason,
          status,
          now,
        ),
    ] }; },
 });
}
export function nativeLearningCourseChanges(source:DatabaseSource){return async(a:WorkContext,id:string,raw:unknown,key:string)=>{
 access(a,true);const input=courseActionSchema.parse(raw);
 return withLearningPurchasedCommand(await databaseSource(source)(),a,{key,operation:`learning.course.${input.action}`,target:{kind:"course",id},purpose:input.action==="publish"?"new-work":"settle-existing",capabilities:["learn.courses"],permission:"records.review",existing:true},write=>learningCourseChanges(write)(a,id,input,key));
};}
export async function changeLearningCourse(a: WorkContext, id: string, raw: unknown, key: string) { return nativeLearningCourseChanges(lazyDatabase(a))(a,id,raw,key); }
function learningAssignments(source: DatabaseSource) {
 const database=databaseSource(source); return createLearningAssignmentApplication({ ...commandPorts(database),
 async person(a,learnerId) { return learningWorkerPlacement(await database(),a,learnerId); },
 async prepare(command,input,person) { const write=await database(), { actor:a,key,id,now }=command; return { native:[
      assertion(
        write,
        key,
        "course",
        "EXISTS(SELECT 1 FROM learning_courses WHERE id=? AND company_id=? AND version=? AND status='published')",
        [input.courseId, a.companyId, input.courseVersion],
      ),
      write
        .prepare(
          "INSERT INTO learning_assignments(id,company_id,branch_id,department_id,course_id,course_version,learner_id,due_date,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          a.companyId,
          person.branch_id,
          person.department_id,
          input.courseId,
          input.courseVersion,
          input.learnerId,
          input.dueDate,
          a.userId,
          now,
          now,
        ),
      assertion(
        write,
        key,
        "learner",
        `EXISTS(SELECT 1 FROM learning_assignments a WHERE a.id=? AND ${learnerEligible})`,
        [id],
      ),
      write
        .prepare(
          "INSERT INTO learning_assignment_events VALUES(?,?,1,'assign',?,?, 'assigned',?)",
        )
        .bind(
          `learning-assignment-event-${key}`,
          id,
          a.userId,
          input.reason,
          now,
        ),
      learningAssignmentNotice(write,"assigned",`learning-assignment-notice-${key}`,id),
    ] }; },
 });
}
function assignmentWitness(actor:WorkContext,id:string){return async(write:SqlDatabase)=>{
 const row=await write.prepare(`SELECT a.course_id,a.course_version,c.company_id course_company,c.required_for,
 (SELECT jsonb_agg(to_jsonb(p)-'updated_at' ORDER BY p.id) ${peoplepayrollSources.learning_adapters_learning_hr_people_p_from} WHERE p.company_id=a.company_id AND p.user_id=a.learner_id) people,
 (SELECT jsonb_agg(to_jsonb(e)-'updated_at' ORDER BY e.id) ${peoplepayrollSources.learning_adapters_learning_hr_people_p_from} ${peoplepayrollSources.learning_adapters_learning_hr_engagements_e_join} ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=a.company_id AND p.user_id=a.learner_id) engagements,
 (SELECT jsonb_agg(to_jsonb(h) ORDER BY h.id) FROM learning_eligibility_handoffs h WHERE h.assignment_id=a.id AND h.company_id=a.company_id) handoffs,
 (SELECT jsonb_agg(to_jsonb(certificate) ORDER BY certificate.id) FROM learning_certificates certificate WHERE certificate.assignment_id=a.id AND certificate.company_id=a.company_id) certificates,
 CASE WHEN ${learnerEligible} THEN 1 ELSE 0 END learner_eligible
 FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id WHERE a.id=? AND a.company_id=?`)
  .bind(id,actor.companyId).first<Record<string,unknown>>();return JSON.stringify(row);
};}
function learnerReferenceWitness(actor:WorkContext,learnerId:string,courseId:string){return async(write:SqlDatabase)=>JSON.stringify({
 people:await learningWorkerReferenceWitness(write,actor.companyId,learnerId),
 course:await write.prepare('SELECT to_jsonb(c) image FROM learning_courses c WHERE c.id=? AND c.company_id=?').bind(courseId,actor.companyId).first('image'),
 assignments:await write.prepare(`SELECT jsonb_agg(to_jsonb(x) ORDER BY x.id) image ${identityaccessSources.learning_adapters_learning_assignments_x_from} WHERE x.company_id=? AND x.user_id=?`).bind(actor.companyId,learnerId).first('image'),
 });}
function assignmentReferenceWitness(actor:WorkContext,id:string){return async(write:SqlDatabase)=>{
 const source=await write.prepare('SELECT learner_id,course_id FROM learning_assignments WHERE id=? AND company_id=?').bind(id,actor.companyId).first<{learner_id:string;course_id:string}>();
 return source?learnerReferenceWitness(actor,source.learner_id,source.course_id)(write):'null';
};}
export function nativeLearningAssignments(source:DatabaseSource){return async(a:WorkContext,raw:unknown,key:string)=>{
 access(a,true);const input=learningAssignSchema.parse(raw),id=`learning-assignment-${key}`;
 return withLearningPurchasedCommand(await databaseSource(source)(),a,{key,operation:"learning.assignment.create",target:{kind:"assignment",id},purpose:"new-work",capabilities:["learn.assignments","people.learner-reference"],permission:"records.review",sourceWitness:assignmentWitness(a,id),readOnlySourceWitness:learnerReferenceWitness(a,input.learnerId,input.courseId)},write=>learningAssignments(write)(a,input,key));
};}
export async function createLearningAssignment(a: WorkContext, raw: unknown, key: string) { return nativeLearningAssignments(lazyDatabase(a))(a,raw,key); }
function learningAssignmentChanges(source: DatabaseSource) {
 const database=databaseSource(source); return createChangeLearningAssignment({ ...commandPorts(database),
 async assignment(a,id) { const write=await database(); return await write
      .prepare(
        `SELECT a.*,c.required_for,CASE WHEN ${learnerEligible} THEN 1 ELSE 0 END learner_eligible FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id WHERE a.id=? AND ${scope}`,
      )
      .bind(id, ...args(a))
      .first<Assignment & { required_for: string | null }>(); },
 async attachment(a,id,key,input) { const write=await database(); return await prepareAttachmentBinding(write, a, {
          attachmentSetIds: input.attachmentSetIds,
          bindingContract: {
            targetType: "learning_assignment",
            bindingType: "learning_completion",
            contexts: [{ purposeKey: "certificate", componentKind: "action", componentRef: "completion" }],
          },
          targetId: id,
          expectedTargetVersion: input.expectedVersion,
          commandId: key,
          bindingId: id,
        }); },
 async prepare(command,input,facts) { const write=await database(), {actor:a,key,id,now}=command, {row,status,attendance,score,evidence,completion,certificateFrom,certificateEnd}=facts;
 const attachmentBinding=facts.attachment as Awaited<ReturnType<typeof prepareAttachmentBinding>> | null;
 const steps=[
      ...(attachmentBinding?.guards ?? []),
      assertion(
        write,
        key,
        "learning-record",
        `EXISTS(SELECT 1 FROM learning_assignments a WHERE a.id=? AND a.version=? AND a.status=? AND ${scope})`,
        [id, input.expectedVersion, row.status, ...args(a)],
      ),
      write
        .prepare(
          "UPDATE learning_assignments SET status=?,attendance=?,score_bps=?,result_evidence=?,completion_reason=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(status, attendance, score, evidence, completion, now, id),
      write
        .prepare(
          "INSERT INTO learning_assignment_events VALUES(?,?,?,?,?,?,?,?)",
        )
        .bind(
          `learning-assignment-event-${key}`,
          id,
          input.expectedVersion + 1,
          input.action,
          a.userId,
          input.reason,
          status,
          now,
        ),
      ...(attachmentBinding?.steps ?? []),
    ];
 if (facts.issueCertificate)
    steps.push(
      write
        .prepare(
          `INSERT INTO learning_eligibility_handoffs(id,assignment_id,company_id,person_id,eligibility_code,source_assignment_version,created_at) SELECT ?,a.id,a.company_id,p.id,?,?,? FROM learning_assignments a ${peoplepayrollSources.learning_adapters_learning_hr_people_p_join} ON p.company_id=a.company_id AND p.user_id=a.learner_id AND p.merged_into_id IS NULL WHERE a.id=?`,
        )
        .bind(
          `learning-eligibility-${key}`,
          row.required_for,
          input.expectedVersion + 1,
          now,
          id,
        ),
      write.prepare("INSERT INTO learning_certificates(id,handoff_id,company_id,person_id,assignment_id,source_assignment_version,eligibility_code,valid_from,expires_on,completion_kind,evidence_ref,issued_by,created_at) SELECT ?,h.id,h.company_id,h.person_id,h.assignment_id,h.source_assignment_version,h.eligibility_code,?,?,?,?,?,? FROM learning_eligibility_handoffs h WHERE h.assignment_id=?")
        .bind(`learning-certificate-${key}`,certificateFrom,certificateEnd,status,input.action==="complete"?input.evidence:input.reason,a.userId,now,id),
    );
steps.push(
    learningAssignmentNotice(write,"changed",`learning-assignment-notice-${key}`,id),
  );
 return {native:steps};
 },
 });
}
export function nativeLearningAssignmentChanges(source:DatabaseSource){return async(a:WorkContext,id:string,raw:unknown,key:string)=>{
 access(a,true);const input=learningActionSchema.parse(raw);
 return withLearningPurchasedCommand(await databaseSource(source)(),a,{key,operation:`learning.assignment.${input.action}`,target:{kind:"assignment",id},purpose:"settle-existing",capabilities:["learn.assignments"],permission:input.action==="waive"||input.action==="cancel"?"records.review":"records.write",existing:true,sourceWitness:assignmentWitness(a,id),readOnlySourceWitness:assignmentReferenceWitness(a,id),
 additionalCapabilities:async write=>{
  if(input.action!=="complete"&&input.action!=="waive")return [];
  const row=await write.prepare("SELECT c.required_for FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id WHERE a.id=? AND a.company_id=?").bind(id,a.companyId).first<{required_for:string|null}>();
  const caps:LearningCapability[]=["learn.results"];if(row?.required_for)caps.push("learn.certificates");return caps;
 }},write=>learningAssignmentChanges(write)(a,id,input,key));
};}
export async function changeLearningAssignment(a:WorkContext,id:string,raw:unknown,key:string){return nativeLearningAssignmentChanges(lazyDatabase(a))(a,id,raw,key);}
export function nativeLearningDetail(source: DatabaseSource) {
 const database=databaseSource(source); return createLearningDetail({workspace:nativeLearningWorkspace(database),error:violation=>new ApiError(violation.status,violation.code,violation.message),
 async history(id,includeResults=true){const write=await database();return (
      await write
        .prepare(
          "SELECT * FROM learning_assignment_events WHERE assignment_id=? AND (?::boolean OR action NOT IN ('complete','waive')) ORDER BY version",
        )
        .bind(id,includeResults)
        .all<Row>()
    ).results;},
 async handoff(id){const write=await database();return await write
      .prepare(
        "SELECT * FROM learning_eligibility_handoffs WHERE assignment_id=?",
      )
      .bind(id)
      .first<Row>();},
 async certificate(id){const write=await database();return await write.prepare("SELECT c.*,r.id revocation_id FROM learning_certificates c LEFT JOIN learning_certificate_revocations r ON r.certificate_id=c.id WHERE c.assignment_id=?").bind(id).first<Row>();},
 });
}
export async function learningAssignmentDetail(a:WorkContext,id:string){return nativeLearningDetail(lazyDatabase(a))(a,id);}
export function nativeLearningExecutiveEvidence(db:SqlDatabase){return createLearningExecutiveEvidence({authorize:access,error:violation=>new ApiError(violation.status,violation.code,violation.message),
 completions:(a,id,range)=>db.prepare(`SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,
        c.title course_title
      FROM learning_assignment_events e
      JOIN learning_assignments a ON a.id=e.assignment_id
      JOIN learning_courses c ON c.id=a.course_id
      WHERE e.id=? AND a.company_id=? AND (?::text IS NULL OR a.branch_id=?)
        AND (?::text IS NULL OR a.department_id=?)
        AND e.action='complete' AND e.status='completed' AND e.created_at>=? AND e.created_at<?
      LIMIT 1`)
      .bind(id, a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId, range.from, range.toExclusive)
      .first<Row>(),overdue:(a,id)=>db.prepare(`SELECT a.id,a.version,a.status,a.due_date,a.updated_at,c.title course_title,c.required_for
    FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id
    WHERE a.id=? AND a.company_id=? AND (?::text IS NULL OR a.branch_id=?)
      AND (?::text IS NULL OR a.department_id=?) AND c.required_for IS NOT NULL
      AND a.due_date<to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')
      AND a.status IN ('assigned','in_progress')
    LIMIT 1`)
    .bind(id, a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId)
    .first<Row>(),
 });}
export async function learningExecutiveEvidenceRow(db:SqlDatabase,a:WorkContext,metric:LearningExecutiveEvidenceMetric,recordId:string,range?:{from:number;toExclusive:number}){await requireLearningPurchasedRead(db,a,metric==="learning-completions"?"learn.results":"learn.assignments");return nativeLearningExecutiveEvidence(db)(a,metric,recordId,range);}
export async function learningWork(a:WorkContext):Promise<import("@erpx/feature-work_projection/lib/work-item").WorkItem[]>{
 if(!hasModuleAccess(a.modules,"learning"))return [];
 const database=lazyDatabase(a);let workspace:Awaited<ReturnType<ReturnType<typeof nativeLearningWorkspace>>>;
 try{workspace=await nativeLearningWorkspace(database)(a);}catch(error){if(error instanceof ApiError&&error.code==="PACKAGE_REQUIRED")return [];throw error;}
 const purchases=workspace.purchases,items=await createLearningWork({workspace:async()=>workspace})(a),itemsById=new Map(workspace.assignments.map(row=>[row.id,row]));if(!purchases)return items;
 return items.map(item=>{const assignment=itemsById.get(item.id),resultAction=assignment?.status==="in_progress",canAct=item.canAct&&hasPermission(a,"records.write")&&!!purchases["learn.assignments"]?.settleExisting&&(!resultAction||hasPermission(a,"records.review")||!!purchases["learn.results"]?.settleExisting&&(!assignment?.required_for||!!purchases["learn.certificates"]?.settleExisting));return {...item,canAct,queue:item.queue==="todo"&&!canAct?"waiting" as const:item.queue};});
}
