import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { env, applyMigrations, type TestMigration } from "../../support/runtime-env";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import type { WorkSnapshot } from "../../../src/server/modules/work_projection/public/work-items";
import { dailyPlanSaveSchema, readDailyPlan, saveDailyPlan } from "../../../src/server/modules/work_projection/public/daily-plan";

const db = env.H001_DB;
const key = () => crypto.randomUUID();
let actor: WorkContext;

function snapshot(updatedAt = 100, visible = true): WorkSnapshot {
  return { unavailableSources: [], items: visible ? [{
    type: "staff_task", id: "task-1", title: "Prepare meeting room",
    detail: "Check supplies", href: "/staff-tasks/task-1", status: "ready",
    queue: "todo", updatedAt, dueAt: null, canAct: true,
  }] : [] };
}

beforeAll(async () => {
  await applyMigrations(db, (env as typeof env & { STAFF_TEST_MIGRATIONS: TestMigration[] }).STAFF_TEST_MIGRATIONS);
});

beforeEach(async () => {
  const companyId = `daily-${key()}`;
  const userId = `user-${key()}`;
  const assignmentId = `assignment-${key()}`;
  await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,?,?)")
    .bind(companyId, companyId, "Daily plan test", companyId).run();
  await db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,'employee','[]',?,0,9999999999999,'holding')")
    .bind(assignmentId, userId, companyId).run();
  actor = {
    userId, assignmentId, role: "employee", companyId,
    branchId: null, departmentId: null, holdingId: "h001", holdingSlug: "test",
    holdingName: "Test", name: "Planner", email: "planner@example.test",
    scope: { kind: "holding", holdingId: "h001", holdingSlug: "test", holdingName: "Test", companyId },
    assignmentScope: "holding", inherited: false, modules: [], assignments: [], workspaces: [],
  };
});

describe("personal daily plan", () => {
  it("stores order and optional time allocation without changing source work", async () => {
    const date = "2026-09-26";
    expect(await readDailyPlan(db, actor, date, snapshot())).toMatchObject({
      date, version: 0, items: [], hiddenCount: 0,
    });
    const input = dailyPlanSaveSchema.parse({ date, expectedVersion: 0, items: [{
      type: "staff_task", id: "task-1", sourceUpdatedAt: 100,
      startMinute: 540, plannedMinutes: 60,
    }] });
    const commandKey = key();
    expect(await saveDailyPlan(db, actor, input, commandKey, snapshot()))
      .toEqual({ date, version: 1 });
    expect(await saveDailyPlan(db, actor, input, commandKey, snapshot()))
      .toEqual({ date, version: 1 });
    await expect(saveDailyPlan(db, actor, { ...input, items: [] }, commandKey, snapshot()))
      .rejects.toMatchObject({ code: "IDEMPOTENCY_CONFLICT" });
    const view = await readDailyPlan(db, actor, date, snapshot());
    expect(view.items).toMatchObject([{
      type: "staff_task", id: "task-1", variant: "record",
      startMinute: 540, plannedMinutes: 60,
      work: { status: "ready", dueAt: null },
    }]);
    const row = await db.prepare("SELECT items_json,version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=?")
      .bind(actor.companyId, actor.userId, date).first<{ items_json: string; version: number }>();
    expect(row?.version).toBe(1);
    expect(row?.items_json).not.toContain("Prepare meeting room");
    expect((await readDailyPlan(db, actor, "2026-09-27", snapshot())).items).toEqual([]);
  });

  it("rejects stale versions and changed or inaccessible work", async () => {
    const date = "2026-09-26";
    const item = { type: "staff_task", id: "task-1", sourceUpdatedAt: 100 };
    const first = { date, expectedVersion: 0, items: [item] };
    await expect(saveDailyPlan(db, actor, first, key(), snapshot(101)))
      .rejects.toMatchObject({ code: "PLAN_ITEM_CHANGED" });
    await expect(saveDailyPlan(db, actor, first, key(), snapshot(100, false)))
      .rejects.toMatchObject({ code: "PLAN_ITEM_UNAVAILABLE" });
    expect(await saveDailyPlan(db, actor, first, key(), snapshot()))
      .toMatchObject({ version: 1 });
    await expect(saveDailyPlan(db, actor, first, key(), snapshot()))
      .rejects.toMatchObject({ status: 409 });
    await expect(saveDailyPlan(db, actor, { ...first, expectedVersion: 1 }, key(), {
      items: [], unavailableSources: ["Tasks"],
    })).rejects.toMatchObject({ code: "PLAN_SOURCE_UNAVAILABLE" });
    const hidden = await readDailyPlan(db, actor, date, snapshot(100, false));
    expect(hidden.items).toEqual([]);
    expect(hidden.hiddenCount).toBe(1);
  });

  it("does not expose a plan across company or user boundaries", async () => {
    const date = "2026-09-26";
    await saveDailyPlan(db, actor, { date, expectedVersion: 0, items: [{
      type: "staff_task", id: "task-1", sourceUpdatedAt: 100,
    }] }, key(), snapshot());
    expect((await readDailyPlan(db, { ...actor, userId: "another-user" }, date, snapshot())).items).toEqual([]);
    expect((await readDailyPlan(db, { ...actor, companyId: "another-company" }, date, snapshot())).items).toEqual([]);
  });

  it("validates unique references and time blocks within the day", () => {
    const item = { type: "staff_task", id: "task-1", sourceUpdatedAt: 100 };
    expect(dailyPlanSaveSchema.safeParse({
      date: "2026-09-26", expectedVersion: 0, items: [item, item],
    }).success).toBe(false);
    expect(dailyPlanSaveSchema.safeParse({
      date: "2026-09-26", expectedVersion: 0,
      items: [{ ...item, startMinute: 1430, plannedMinutes: 30 }],
    }).success).toBe(false);
  });
});
