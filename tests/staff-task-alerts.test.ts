import { beforeAll, beforeEach, expect, it, vi } from "vitest";
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

async function createTask(dueAt: number | null, state = "ready") {
  const company = `task-alert-${key()}`;
  const id = `task-${key()}`;
  const createdAt = dueAt ?? Date.now();
  await db
    .prepare(
      `INSERT INTO staff_tasks
        (id,company_id,title,description,creator_user_id,assignee_user_id,due_at,
         start_approvers_json,completion_approvers_json,state,created_at,updated_at)
       VALUES(?,?,?,'',?,?,?,'[]','[]',?,?,?)`,
    )
    .bind(id, company, "Review receiving exception", "user-creator", "user-assignee", dueAt, state, createdAt, createdAt)
    .run();
  return { company, id };
}

it("does not send deadline alerts for an ordinary task without a due date", async () => {
  const now = Date.now();
  const task = await createTask(null);
  expect(await escalateStaffTasks(db, now + 7 * 86_400_000)).toEqual({ dueSoon: 0, overdue: 0 });
  const notifications = await db.prepare("SELECT COUNT(*)::int count FROM staff_notifications WHERE record_id=?")
    .bind(task.id).first<{ count: number }>();
  expect(notifications?.count).toBe(0);
});

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

it("holds task alerts until Operations is enabled for the company in production", async () => {
  const now = Date.now(), task = await createTask(now + 60 * 60 * 1000);
  vi.stubEnv("NODE_ENV", "production");
  try {
    expect(await escalateStaffTasks(db, now)).toEqual({ dueSoon: 0, overdue: 0 });
    await db.prepare("INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,version,reason,changed_by,changed_at) VALUES(?,'operations','pilot',1,'Test scheduled alerts','test',?)")
      .bind(task.company,now).run();
    expect(await escalateStaffTasks(db, now)).toEqual({ dueSoon: 1, overdue: 0 });
  } finally { vi.unstubAllEnvs(); }
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
