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
