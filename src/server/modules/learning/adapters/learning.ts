import {learningAssignmentNotice} from "../../collaboration/public/learning-notification-effects";
import {learningTalentSnapshotSources as identityaccessSnapshotSources} from '../../identity_access/public/learning-talent-snapshot-sources';
import {learningTalentSnapshotSources as peoplepayrollSnapshotSources} from '../../people_payroll/public/learning-talent-snapshot-sources';
const identityaccessSources=identityaccessSnapshotSources();
const peoplepayrollSources=peoplepayrollSnapshotSources();
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { Row } from "@/server/modules/commerce/public/commerce";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { hasModuleAccess } from "@/data/module-access";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { ApiError } from "@/server/modules/runtime/public/http";
import { prepareAttachmentBinding } from "@/server/modules/files/public/attachments/service";
import { createLearningWorkspace, createChangeLearningCourse, createLearningAssignment as createLearningAssignmentApplication, createChangeLearningAssignment, createLearningDetail, createLearningExecutiveEvidence, createLearningWork, type LearningCommandPorts, type LearningCommand, type LearningMutation, type LearningExecutiveEvidenceMetric } from "../application/learning";
import type { LearningAssignment as Assignment } from "../domain/learning";
export type { LearningExecutiveEvidenceMetric } from "../application/learning";
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
 return createLearningWorkspace({ authorize: access,
 async courses(a) { const write = await database(); return (
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
          `SELECT a.*,c.title course_title,c.required_for,(SELECT legal_name ${peoplepayrollSources.learning_adapters_learning_hr_people_from} WHERE company_id=a.company_id AND user_id=a.learner_id AND merged_into_id IS NULL LIMIT 1) learner_name,CASE WHEN ${learnerEligible} THEN 1 ELSE 0 END learner_eligible FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id WHERE ${scope} AND (NOT ?::boolean OR a.learner_id=?) ORDER BY a.updated_at DESC`,
        )
        .bind(...args(a),personal,a.userId)
        .all<Assignment>()
    ).results; },
 async people(a) { const write = await database(); return (
      await write
        .prepare(
          `SELECT p.user_id id,p.legal_name name,e.branch_id,e.department_id ${peoplepayrollSources.learning_adapters_learning_hr_people_p_from} ${peoplepayrollSources.learning_adapters_learning_hr_engagements_e_join} ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' ORDER BY p.legal_name`,
        )
        .bind(a.companyId)
        .all<Row>()
    ).results; },
 });
}
export async function learningWorkspace(a: WorkContext, personal = false) { return nativeLearningWorkspace(lazyDatabase(a))(a, personal); }
export { createLearningCourse } from "./course";
export function nativeLearningCourseChanges(source: DatabaseSource) {
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
export async function changeLearningCourse(a: WorkContext, id: string, raw: unknown, key: string) { return nativeLearningCourseChanges(lazyDatabase(a))(a,id,raw,key); }
export function nativeLearningAssignments(source: DatabaseSource) {
 const database=databaseSource(source); return createLearningAssignmentApplication({ ...commandPorts(database),
 async person(a,learnerId) { const write=await database(), input={learnerId}; return await write
      .prepare(
        `SELECT p.id,p.company_id,e.branch_id,e.department_id ${peoplepayrollSources.learning_adapters_learning_hr_people_p_from} ${peoplepayrollSources.learning_adapters_learning_hr_engagements_e_join} ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=? AND p.user_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active'`,
      )
      .bind(a.companyId, input.learnerId)
      .first<Row>(); },
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
export async function createLearningAssignment(a: WorkContext, raw: unknown, key: string) { return nativeLearningAssignments(lazyDatabase(a))(a,raw,key); }
export function nativeLearningAssignmentChanges(source: DatabaseSource) {
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
export async function changeLearningAssignment(a:WorkContext,id:string,raw:unknown,key:string){return nativeLearningAssignmentChanges(lazyDatabase(a))(a,id,raw,key);}
export function nativeLearningDetail(source: DatabaseSource) {
 const database=databaseSource(source); return createLearningDetail({workspace:nativeLearningWorkspace(database),error:violation=>new ApiError(violation.status,violation.code,violation.message),
 async history(id){const write=await database();return (
      await write
        .prepare(
          "SELECT * FROM learning_assignment_events WHERE assignment_id=? ORDER BY version",
        )
        .bind(id)
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
export async function learningExecutiveEvidenceRow(db:SqlDatabase,a:WorkContext,metric:LearningExecutiveEvidenceMetric,recordId:string,range?:{from:number;toExclusive:number}){return nativeLearningExecutiveEvidence(db)(a,metric,recordId,range);}
export async function learningWork(a:WorkContext):Promise<import("@/lib/work-item").WorkItem[]>{return createLearningWork({workspace:nativeLearningWorkspace(lazyDatabase(a))})(a);}
