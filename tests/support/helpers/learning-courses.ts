import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

export type LearningCourseDraft = {
  title: string;
  description: string;
  provider: string;
  durationMinutes: number;
  requiredFor?: string | null;
};

/** Native-shape course seeding for integration fixtures. The production
 * create command is served by the native Learning router. */
export async function createCourseSeed(
  database: SqlDatabase,
  actor: { companyId: string; userId: string },
  input: LearningCourseDraft,
  commandKey: string,
) {
  const id = `learning-course-${commandKey}`,
    now = Date.now();
  await database.batch([
    database
      .prepare(
        "INSERT INTO learning_courses(id,company_id,title,description,provider,duration_minutes,required_for,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)",
      )
      .bind(
        id,
        actor.companyId,
        input.title,
        input.description,
        input.provider,
        input.durationMinutes,
        input.requiredFor ?? null,
        actor.userId,
        now,
        now,
      ),
    database
      .prepare(
        "INSERT INTO learning_course_events VALUES(?,?,1,'create',?,'Course drafted for independent review','draft',?)",
      )
      .bind(`learning-course-event-${commandKey}`, id, actor.userId, now),
  ]);
  return { id, status: "draft", version: 1 };
}

export async function publishCourseSeed(
  database: SqlDatabase,
  courseId: string,
  reviewerId: string,
  expectedVersion = 1,
) {
  const now = Date.now(),
    version = expectedVersion + 1;
  await database.batch([
    database
      .prepare(
        "UPDATE learning_courses SET status='published',reviewed_by=?,reviewed_at=?,version=?,updated_at=? WHERE id=?",
      )
      .bind(reviewerId, now, version, now, courseId),
    database
      .prepare("INSERT INTO learning_course_events VALUES(?,?,?,?,?,?,?,?)")
      .bind(
        `learning-course-event-publish-${courseId}`,
        courseId,
        version,
        "publish",
        reviewerId,
        "Course published by fixture",
        "published",
        now,
      ),
  ]);
}

export type LearningAssignmentDraft = {
  courseId: string;
  courseVersion: number;
  learnerId: string;
  dueDate: string;
  reason: string;
};

/** Native-shape assignment seeding for integration fixtures. The production
 * assignment-create command is served by the native Learning router, which
 * resolves the learner's active placement and emits the assignment notice. */
export async function createAssignmentSeed(
  database: SqlDatabase,
  actor: {
    companyId: string;
    userId: string;
    branchId?: string | null;
    departmentId?: string | null;
  },
  input: LearningAssignmentDraft,
  commandKey: string,
) {
  const id = `learning-assignment-${commandKey}`,
    now = Date.now(),
    person = await database
      .prepare(
        `SELECT p.id,e.branch_id,e.department_id FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
         WHERE p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active'
           AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)
           AND p.user_id=? ORDER BY p.id,e.id LIMIT 1`,
      )
      .bind(
        actor.companyId,
        actor.branchId ?? null,
        actor.branchId ?? null,
        actor.departmentId ?? null,
        actor.departmentId ?? null,
        input.learnerId,
      )
      .first<{ id: string; branch_id: string; department_id: string | null }>();
  if (!person?.branch_id)
    throw new Error("LEARNING_ASSIGNMENT_SEED_PLACEMENT_REQUIRED");
  await database.batch([
    database
      .prepare(
        "INSERT INTO learning_assignments(id,company_id,branch_id,department_id,course_id,course_version,learner_id,due_date,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
      )
      .bind(
        id,
        actor.companyId,
        person.branch_id,
        person.department_id,
        input.courseId,
        input.courseVersion,
        input.learnerId,
        input.dueDate,
        actor.userId,
        now,
        now,
      ),
    database
      .prepare(
        "INSERT INTO learning_assignment_events VALUES(?,?,1,'assign',?,?, 'assigned',?)",
      )
      .bind(`learning-assignment-event-${commandKey}`, id, actor.userId, input.reason, now),
    database
      .prepare(
        "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,learner_id,'learning_assignment',id,version,'New learning assignment: review course and due date',updated_at FROM learning_assignments WHERE id=?",
      )
      .bind(`learning-assignment-notice-${commandKey}`, id),
  ]);
  return { id, status: "assigned" as const, version: 1 };
}

export async function retireCourseSeed(
  database: SqlDatabase,
  courseId: string,
  reviewerId: string,
) {
  const now = Date.now();
  await database.batch([
    database
      .prepare(
        "UPDATE learning_courses SET status='retired',reviewed_by=?,reviewed_at=?,version=version+1,updated_at=? WHERE id=?",
      )
      .bind(reviewerId, now, now, courseId),
    database
      .prepare(
        "INSERT INTO learning_course_events SELECT ?,?,version,'retire',?,?,'retired',? FROM learning_courses WHERE id=?",
      )
      .bind(
        `learning-course-event-retire-${courseId}`,
        courseId,
        reviewerId,
        "Course retired by fixture",
        now,
        courseId,
      ),
  ]);
}
