import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { Row } from "./commerce";
import type { WorkItem } from "@/lib/work-item";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { hasPermission } from "@/lib/permissions";
import { hasModuleAccess } from "@/data/module-access";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
const reason = z.string().trim().min(3).max(3000),
  version = z.number().int().positive(),
  date = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);
const courseSchema = z.object({
  title: z.string().trim().min(3).max(160),
  description: reason,
  provider: z.string().trim().min(2).max(160),
  durationMinutes: z.number().int().positive(),
  requiredFor: z.string().trim().min(2).max(120).nullable().optional(),
});
const courseAction = z.object({
  action: z.enum(["publish", "reject", "retire"]),
  expectedVersion: version,
  reason,
});
const assignSchema = z.object({
  courseId: z.string().min(1),
  courseVersion: version,
  learnerId: z.string().min(1),
  dueDate: date,
  reason,
});
const assignmentAction = z.discriminatedUnion("action", [
  z.object({ action: z.literal("start"), expectedVersion: version, reason }),
  z.object({
    action: z.literal("complete"),
    expectedVersion: version,
    attendance: z.enum(["attended", "absent"]),
    scoreBps: z.number().int().min(0).max(10000),
    evidence: reason,
    reason,
  }),
  z.object({
    action: z.enum(["waive", "cancel"]),
    expectedVersion: version,
    reason,
  }),
]);
const scope =
    "a.company_id=? AND (?::text IS NULL OR a.branch_id=?) AND (?::text IS NULL OR a.department_id=?)",
  args = (a: WorkContext) => [
    a.companyId,
    a.branchId,
    a.branchId,
    a.departmentId,
    a.departmentId,
  ];
const learnerEligible = `EXISTS(SELECT 1 FROM assignments x JOIN hr_people p ON p.user_id=x.user_id AND p.company_id=x.company_id JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE x.company_id=a.company_id AND x.user_id=a.learner_id AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=a.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=a.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active')`;
type Assignment = Row & {
  id: string;
  version: number;
  status: string;
  learner_id: string;
  created_by: string;
  updated_at: number;
  learner_eligible: number;
};
function access(a: WorkContext, write = false) {
  requirePermission(a, write ? "records.write" : "records.read");
  if (!hasModuleAccess(a.modules, "learning"))
    throw new ApiError(
      403,
      "MODULE_FORBIDDEN",
      "Your current role cannot access Learning.",
    );
}
export async function learningWorkspace(a: WorkContext) {
  access(a);
  const { write } = await holdingResources(a.holdingId),
    courses = (
      await write
        .prepare(
          "SELECT * FROM learning_courses WHERE company_id=? ORDER BY updated_at DESC",
        )
        .bind(a.companyId)
        .all<Row>()
    ).results,
    assignments = (
      await write
        .prepare(
          `SELECT a.*,c.title course_title,c.required_for,(SELECT legal_name FROM hr_people WHERE company_id=a.company_id AND user_id=a.learner_id AND merged_into_id IS NULL LIMIT 1) learner_name,CASE WHEN ${learnerEligible} THEN 1 ELSE 0 END learner_eligible FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id WHERE ${scope} ORDER BY a.updated_at DESC`,
        )
        .bind(...args(a))
        .all<Assignment>()
    ).results,
    people = (
      await write
        .prepare(
          "SELECT p.user_id id,p.legal_name name,e.branch_id,e.department_id FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' ORDER BY p.legal_name",
        )
        .bind(a.companyId)
        .all<Row>()
    ).results;
  return {
    courses,
    assignments,
    people,
    canWrite: hasPermission(a, "records.write"),
    canReview: hasPermission(a, "records.review"),
    actorId: a.userId,
  };
}
export async function createLearningCourse(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const input = courseSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    id = `learning-course-${key}`,
    op = "learning.course.create",
    prior = await replayCommand(write, a, key, op, id, input);
  if (prior) return prior;
  const now = Date.now();
  return executeCommand(
    write,
    a,
    key,
    op,
    input,
    id,
    1,
    { id, version: 1, status: "draft" },
    [
      write
        .prepare(
          "INSERT INTO learning_courses(id,company_id,title,description,provider,duration_minutes,required_for,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          a.companyId,
          input.title,
          input.description,
          input.provider,
          input.durationMinutes,
          input.requiredFor ?? null,
          a.userId,
          now,
          now,
        ),
      write
        .prepare(
          "INSERT INTO learning_course_events VALUES(?,?,1,'create',?,'Course drafted for independent review','draft',?)",
        )
        .bind(`learning-course-event-${key}`, id, a.userId, now),
    ],
  );
}
export async function changeLearningCourse(
  a: WorkContext,
  id: string,
  raw: unknown,
  key: string,
) {
  access(a, true);
  requirePermission(a, "records.review");
  const input = courseAction.parse(raw),
    { write } = await holdingResources(a.holdingId),
    row = await write
      .prepare("SELECT * FROM learning_courses WHERE id=? AND company_id=?")
      .bind(id, a.companyId)
      .first<Row>();
  if (!row)
    throw new ApiError(404, "LEARNING_COURSE_NOT_FOUND", "Course not found.");
  const expected = input.action === "retire" ? "published" : "draft",
    status =
      input.action === "publish"
        ? "published"
        : input.action === "reject"
          ? "rejected"
          : "retired",
    op = `learning.course.${input.action}`,
    prior = await replayCommand(write, a, key, op, id, input);
  if (prior) return prior;
  const now = Date.now();
  return executeCommand(
    write,
    a,
    key,
    op,
    input,
    id,
    input.expectedVersion + 1,
    { id, version: input.expectedVersion + 1, status },
    [
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
    ],
  );
}
export async function createLearningAssignment(
  a: WorkContext,
  raw: unknown,
  key: string,
) {
  access(a, true);
  requirePermission(a, "records.review");
  const input = assignSchema.parse(raw),
    { write } = await holdingResources(a.holdingId),
    person = await write
      .prepare(
        "SELECT p.id,p.company_id,e.branch_id,e.department_id FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=? AND p.user_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active'",
      )
      .bind(a.companyId, input.learnerId)
      .first<Row>();
  if (!person)
    throw new ApiError(
      422,
      "LEARNING_LEARNER_INELIGIBLE",
      "Choose an active worker.",
    );
  const id = `learning-assignment-${key}`,
    op = "learning.assignment.create",
    prior = await replayCommand(write, a, key, op, id, input);
  if (prior) return prior;
  const now = Date.now();
  return executeCommand(
    write,
    a,
    key,
    op,
    input,
    id,
    1,
    { id, version: 1, status: "assigned" },
    [
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
      write
        .prepare(
          "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,learner_id,'learning_assignment',id,version,'New learning assignment: review course and due date',updated_at FROM learning_assignments WHERE id=?",
        )
        .bind(`learning-assignment-notice-${key}`, id),
    ],
  );
}
export async function learningAssignmentDetail(a: WorkContext, id: string) {
  const data = await learningWorkspace(a),
    assignment = data.assignments.find((x) => x.id === id);
  if (!assignment)
    throw new ApiError(
      404,
      "LEARNING_ASSIGNMENT_NOT_FOUND",
      "Learning assignment not found.",
    );
  const { write } = await holdingResources(a.holdingId);
  return {
    ...data,
    assignment,
    history: (
      await write
        .prepare(
          "SELECT * FROM learning_assignment_events WHERE assignment_id=? ORDER BY version",
        )
        .bind(id)
        .all<Row>()
    ).results,
    handoff: await write
      .prepare(
        "SELECT * FROM learning_eligibility_handoffs WHERE assignment_id=?",
      )
      .bind(id)
      .first<Row>(),
  };
}
export async function changeLearningAssignment(
  a: WorkContext,
  id: string,
  raw: unknown,
  key: string,
) {
  access(a, true);
  const input = assignmentAction.parse(raw),
    { write } = await holdingResources(a.holdingId),
    row = await write
      .prepare(
        `SELECT a.*,c.required_for,CASE WHEN ${learnerEligible} THEN 1 ELSE 0 END learner_eligible FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id WHERE a.id=? AND ${scope}`,
      )
      .bind(id, ...args(a))
      .first<Assignment & { required_for: string | null }>();
  if (!row)
    throw new ApiError(
      404,
      "LEARNING_ASSIGNMENT_NOT_FOUND",
      "Learning assignment not found.",
    );
  const own = row.learner_id === a.userId && Number(row.learner_eligible) > 0,
    review = hasPermission(a, "records.review");
  if (["start", "complete"].includes(input.action) && !own)
    throw new ApiError(
      403,
      "LEARNING_LEARNER_REQUIRED",
      "The assigned learner must record this action.",
    );
  if (["waive", "cancel"].includes(input.action) && !review)
    throw new ApiError(
      403,
      "LEARNING_REVIEW_REQUIRED",
      "A reviewer must waive or cancel learning.",
    );
  const allowed = {
    start: ["assigned"],
    complete: ["in_progress"],
    waive: ["assigned", "in_progress"],
    cancel: ["assigned", "in_progress"],
  } as Record<string, string[]>;
  if (!allowed[input.action].includes(row.status))
    throw new ApiError(
      409,
      "LEARNING_STATE_CHANGED",
      "Refresh the assignment.",
    );
  const status =
      input.action === "start"
        ? "in_progress"
        : input.action === "complete"
          ? "completed"
          : input.action === "waive"
            ? "waived"
            : "cancelled",
    op = `learning.assignment.${input.action}`,
    prior = await replayCommand(write, a, key, op, id, input);
  if (prior) return prior;
  const now = Date.now(),
    attendance =
      input.action === "complete" ? input.attendance : row.attendance,
    score = input.action === "complete" ? input.scoreBps : row.score_bps,
    evidence =
      input.action === "complete" ? input.evidence : row.result_evidence,
    completion = ["complete", "waive"].includes(input.action)
      ? input.reason
      : null,
    steps = [
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
    ];
  if (["complete", "waive"].includes(input.action) && row.required_for)
    steps.push(
      write
        .prepare(
          "INSERT INTO learning_eligibility_handoffs(id,assignment_id,company_id,person_id,eligibility_code,source_assignment_version,created_at) SELECT ?,a.id,a.company_id,p.id,?,?,? FROM learning_assignments a JOIN hr_people p ON p.company_id=a.company_id AND p.user_id=a.learner_id AND p.merged_into_id IS NULL WHERE a.id=?",
        )
        .bind(
          `learning-eligibility-${key}`,
          row.required_for,
          input.expectedVersion + 1,
          now,
          id,
        ),
    );
  steps.push(
    write
      .prepare(
        "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,learner_id,'learning_assignment',id,version,'Learning assignment updated: review current status',updated_at FROM learning_assignments WHERE id=?",
      )
      .bind(`learning-assignment-notice-${key}`, id),
  );
  return executeCommand(
    write,
    a,
    key,
    op,
    input,
    id,
    input.expectedVersion + 1,
    { id, version: input.expectedVersion + 1, status },
    steps,
  );
}
export async function learningWork(a: WorkContext): Promise<WorkItem[]> {
  if (
    !hasPermission(a, "records.read") ||
    !hasModuleAccess(a.modules, "learning")
  )
    return [];
  const data = await learningWorkspace(a);
  return data.assignments.map((x) => {
    const terminal = ["completed", "waived", "cancelled"].includes(x.status),
      own = x.learner_id === a.userId && Number(x.learner_eligible) > 0,
      review = hasPermission(a, "records.review");
    return {
      id: String(x.id),
      type: "learning_assignment" as const,
      title: `Learning · ${x.course_title}`,
      detail: `${x.learner_name} · due ${x.due_date}`,
      href: `/learning/${x.id}`,
      status: String(x.status),
      updatedAt: Number(x.updated_at),
      dueAt: Date.parse(String(x.due_date) + "T00:00:00Z"),
      queue: terminal
        ? ("completed" as const)
        : own || review
          ? ("todo" as const)
          : ("waiting" as const),
      canAct: !terminal && (own || review),
      responsibleQueue: "Assigned learner",
      blocker: Number(x.learner_eligible)
        ? null
        : "Assigned learner is no longer eligible; People reviewer must waive or cancel",
      relationships: [
        own
          ? ("assigned" as const)
          : x.created_by === a.userId
            ? ("requested" as const)
            : ("team" as const),
      ],
    };
  });
}
