import { beforeAll, beforeEach, expect, it } from "vitest";
import { applyMigrations, env, type TestMigration } from "./runtime-env";
import { escalateStaffTasks } from "../src/server/staff-task-alerts";

const db = env.H001_DB;
const key = () => crypto.randomUUID();

beforeAll(async () => {
  await applyMigrations(
    db,
    (env as typeof env & { STAFF_TEST_MIGRATIONS: TestMigration[] })
      .STAFF_TEST_MIGRATIONS,
  );
});

beforeEach(async () => {
  await db.prepare("DELETE FROM staff_notifications WHERE record_type='staff_task_alert'").run();
  await db.prepare("DELETE FROM staff_task_delegations WHERE task_id LIKE 'task-%'").run();
  await db.prepare("DELETE FROM staff_tasks WHERE id LIKE 'task-%'").run();
});

async function createTask(dueAt: number, state = "ready") {
  const company = `task-alert-${key()}`;
  const id = `task-${key()}`;
  await db
    .prepare(
      `INSERT INTO staff_tasks
        (id,company_id,title,description,creator_user_id,assignee_user_id,due_at,
         start_approvers_json,completion_approvers_json,state,created_at,updated_at)
       VALUES(?,?,?,'',?,?,?,'[]','[]',?,?,?)`,
    )
    .bind(id, company, "Review receiving exception", "user-creator", "user-assignee", dueAt, state, dueAt, dueAt)
    .run();
  return { company, id };
}

it("emits a deduplicated due-soon reminder to the assignee", async () => {
  const now = Date.now();
  const task = await createTask(now + 60 * 60 * 1000);
  expect(await escalateStaffTasks(db, now)).toEqual({ dueSoon: 1, overdue: 0 });
  expect(await escalateStaffTasks(db, now + 30 * 60 * 1000)).toEqual({ dueSoon: 0, overdue: 0 });
  const rows = await db
    .prepare("SELECT user_id,record_type,message FROM staff_notifications WHERE company_id=? AND record_id=?")
    .bind(task.company, task.id)
    .all();
  expect(rows.results).toEqual([
    {
      user_id: "user-assignee",
      record_type: "staff_task_alert",
      message: "Task due within 24 hours: Review receiving exception.",
    },
  ]);
});

it("escalates overdue work to the creator and project owner without flooding retries", async () => {
  const now = Date.now();
  const task = await createTask(now + 60 * 60 * 1000);
  const project = `project-${key()}`;
  await db
    .prepare(
      "INSERT INTO staff_projects(id,company_id,title,owner_user_id,created_at,updated_at) VALUES(?,?,?,'user-owner',?,?)",
    )
    .bind(project, task.company, "Receiving controls", now, now)
    .run();
  await db.prepare("UPDATE staff_tasks SET project_id=? WHERE id=?").bind(project, task.id).run();
  await db.prepare("INSERT INTO staff_task_delegations(id,company_id,task_id,delegator_user_id,delegate_user_id,starts_at,expires_at,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
    .bind(`delegation-${key()}`, task.company, task.id, "user-creator", "user-delegate", now - 1000, now + 24 * 60 * 60 * 1000, "Coverage", now, now).run();
  expect(await escalateStaffTasks(db, now)).toEqual({ dueSoon: 2, overdue: 0 });
  // Once the clock advances, the separate overdue escalation is emitted once.
  expect(await escalateStaffTasks(db, now + 2 * 60 * 60 * 1000)).toEqual({ dueSoon: 0, overdue: 4 });
  expect(await escalateStaffTasks(db, now + 3 * 60 * 60 * 1000)).toEqual({ dueSoon: 0, overdue: 0 });
  const rows = await db
    .prepare("SELECT user_id,message FROM staff_notifications WHERE company_id=? AND record_id=? ORDER BY user_id,message")
    .bind(task.company, task.id)
    .all();
  expect(rows.results).toHaveLength(6);
  expect(rows.results.map((row) => row.user_id)).toEqual([
    "user-assignee",
    "user-assignee",
    "user-creator",
    "user-delegate",
    "user-delegate",
    "user-owner",
  ]);
});
