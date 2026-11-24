import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {nativeLearningAssignmentNotice} from "./native-learning-notification";
import {learningTalentSnapshotSources as identityaccessSnapshotSources} from '@erpx/server-identity_access/public/learning-talent-snapshot-sources';
import {learningTalentSnapshotSources as peoplepayrollSnapshotSources} from '@erpx/server-people_payroll/public/learning-talent-snapshot-sources';
import {learningWorkerReferenceWitness} from '@erpx/server-people_payroll/public/learning-worker-references';
const identityaccessSources=identityaccessSnapshotSources();
const peoplepayrollSources=peoplepayrollSnapshotSources();
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { ApiError } from "@erpx/server-runtime/public/http";
import { prepareAttachmentBinding } from "@erpx/server-files/public/attachments/service";
import { createChangeLearningCourse, createChangeLearningAssignment, createLearningExecutiveEvidence, createLearningWork, type LearningCommandPorts, type LearningCommand, type LearningMutation, type LearningExecutiveEvidenceMetric } from "@erpx/server-learning/application/learning";
import type { LearningAssignment as Assignment } from "@erpx/server-learning/domain/learning";
import {courseActionSchema,learningActionSchema} from "@erpx/server-learning/domain/learning";
import {withLearningPurchasedCommand,withLearningPurchasedRead,requireLearningPurchasedRead,type LearningCapability} from "@erpx/server-learning/adapters/purchases";
import {overdueEvidenceColumns} from "./learning-read-sql";
import {completionEvidenceColumns} from "./learning-completion-evidence";
export type { LearningExecutiveEvidenceMetric } from "@erpx/server-learning/application/learning";
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
/* Native learning workspace/detail reads are owned by the mounted axum port
 * (@erpx/server-learning/public/native-learning). The TypeScript workspace and
 * detail readers were read-only and are retired; no TS fallback remains. */
/* r5 read-tail family: the four assignment-child images below share byte-identical
 * subquery framing and differ only by aggregate, source tail and output label.
 * One parameterized source replaces four inlined statements; r3/r4 helper files
 * and every lock statement stay untouched. */
const assignmentChildImage = (aggregate: string, source: string, label: string) => `(SELECT ${aggregate} ${source}) ${label}`;
function assignmentWitness(actor:WorkContext,id:string){return async(write:SqlDatabase)=>{
 const row=await write.prepare(`SELECT a.course_id,a.course_version,c.company_id course_company,c.required_for,
 ${assignmentChildImage("jsonb_agg(to_jsonb(p)-'updated_at' ORDER BY p.id)", `${peoplepayrollSources.learning_adapters_learning_hr_people_p_from} WHERE p.company_id=a.company_id AND p.user_id=a.learner_id`, "people")},
 ${assignmentChildImage("jsonb_agg(to_jsonb(e)-'updated_at' ORDER BY e.id)", `${peoplepayrollSources.learning_adapters_learning_hr_people_p_from} ${peoplepayrollSources.learning_adapters_learning_hr_engagements_e_join} ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=a.company_id AND p.user_id=a.learner_id`, "engagements")},
 ${assignmentChildImage("jsonb_agg(to_jsonb(h) ORDER BY h.id)", "FROM learning_eligibility_handoffs h WHERE h.assignment_id=a.id AND h.company_id=a.company_id", "handoffs")},
 ${assignmentChildImage("jsonb_agg(to_jsonb(certificate) ORDER BY certificate.id)", "FROM learning_certificates certificate WHERE certificate.assignment_id=a.id AND certificate.company_id=a.company_id", "certificates")},
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
// Wave 3: assignment writes execute only in the native axum Learning route
// (learning_http ChangeAssignment). No TypeScript SQL writer remains for this
// capability; callers must use the native command client and fail closed.
function learningAssignmentChanges(_source:DatabaseSource){
  return (_a:WorkContext,_id:string,_input:unknown,_key:string):Promise<never>=>Promise.reject(new ApiError(503,'NATIVE_LEARNING_UNAVAILABLE','The native Learning assignment command is unavailable.'));
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
/* Native learning assignment detail (history/handoff/certificate) is owned by
 * the mounted axum port; the TS detail reader and its export are retired. */
export function nativeLearningExecutiveEvidence(db:SqlDatabase){return createLearningExecutiveEvidence({authorize:access,error:violation=>new ApiError(violation.status,violation.code,violation.message),
 completions:(a,id,range)=>db.prepare(`${completionEvidenceColumns}
      FROM learning_assignment_events e
      JOIN learning_assignments a ON a.id=e.assignment_id
      JOIN learning_courses c ON c.id=a.course_id
      WHERE e.id=? AND a.company_id=? AND (?::text IS NULL OR a.branch_id=?)
        AND (?::text IS NULL OR a.department_id=?)
        AND e.action='complete' AND e.status='completed' AND e.created_at>=? AND e.created_at<?
      LIMIT 1`)
      .bind(id, a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId, range.from, range.toExclusive)
      .first<Row>(),overdue:(a,id)=>db.prepare(`${overdueEvidenceColumns}
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
/* Native learning work read. The mounted axum workspace route owns the scoped
 * assignments + eligibility SQL and applies results-history redaction before
 * the rows leave the owner; the TS work projection consumes that payload. The
 * learnerEligible predicate kept at module scope remains for the transactional
 * assignmentWitness (R3) and is not part of this read. */
export async function nativeLearningWork(a:WorkContext):Promise<import("@erpx/feature-work_projection/lib/work-item").WorkItem[]>{
 if(!hasModuleAccess(a.modules,"learning"))return [];
 let page:Awaited<ReturnType<typeof import("@erpx/workplace/lib/native-learning").nativeLearningWorkspacePage>>;
 try{
  page=await (await import("@erpx/workplace/lib/native-learning")).nativeLearningWorkspacePage(a);
 }catch(error){if(error instanceof ApiError&&error.code==="PACKAGE_REQUIRED")return [];throw error;}
 const assignments=page.assignments as unknown as Assignment[];
 const workspace={courses:[],assignments,people:[],canWrite:page.canWrite,canReview:page.canReview,actorId:page.actorId,purchases:page.purchases};
 const items=await createLearningWork({workspace:async()=>workspace})(a),itemsById=new Map(assignments.map(row=>[row.id,row]));if(!page.purchases)return items;
 const p=page.purchases;
 return items.map(item=>{const assignment=itemsById.get(item.id),resultAction=assignment?.status==="in_progress",canAct=item.canAct&&hasPermission(a,"records.write")&&!!p["learn.assignments"]?.settleExisting&&(!resultAction||hasPermission(a,"records.review")||!!p["learn.results"]?.settleExisting&&(!assignment?.required_for||!!p["learn.certificates"]?.settleExisting));return {...item,canAct,queue:item.queue==="todo"&&!canAct?"waiting" as const:item.queue};});
}
/** Thin native-backed alias retained for record-purchases.filterLearningWork (R1). */
export const learningWork = nativeLearningWork;
