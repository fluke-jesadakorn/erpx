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
