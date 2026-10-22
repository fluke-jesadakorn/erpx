import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkSnapshot } from "@erpx/server-work_projection/domain/work-items";
import { createDailyPlan, type DailyPlanUnit, type DailyPlanResult } from "@erpx/server-work_projection/application/daily-plan";
import type { PlanRow } from "@erpx/server-work_projection/domain/daily-plan";
import type { SqlStatement } from "@erpx/server-runtime/public/runtime/types";
const service = createDailyPlan<SqlStatement>({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message) });
export function dailyPlanUnit(db: SqlDatabase): DailyPlanUnit<SqlStatement> {
    return {
        read: (companyId, userId, date) => db.prepare("SELECT items_json,version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=?").bind(companyId, userId, date).first<PlanRow>(),
        lock: (companyId, userId, date) => db.prepare("SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE").bind(companyId, userId, date),
        version: (key, companyId, userId, date, version) => assertion(db, key, "daily-plan-version", "COALESCE((SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=?),0)=?", [companyId, userId, date, version]),
        write: (companyId, userId, date, items, version, key, now, expected) => db.prepare(`INSERT INTO staff_daily_plans(company_id,user_id,plan_date,items_json,version,last_command_id,updated_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(company_id,user_id,plan_date) DO UPDATE SET items_json=excluded.items_json,version=excluded.version,last_command_id=excluded.last_command_id,updated_at=excluded.updated_at WHERE staff_daily_plans.version=?`).bind(companyId, userId, date, items, version, key, now, expected),
        written: (key, companyId, userId, date, version) => assertion(db, key, "daily-plan-written", "EXISTS(SELECT 1 FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? AND version=? AND last_command_id=?)", [companyId, userId, date, version, key]),
        replay: (actor, key, id, input) => replayCommand<DailyPlanResult>(db, actor, key, "daily.plan.save", id, input),
        execute: (actor, key, input, id, result, effects) => executeCommand(db, actor, key, "daily.plan.save", input, id, result.version, result, effects)
    };
}
export function readDailyPlan(db: SqlDatabase, actor: Pick<WorkContext, "companyId" | "userId">, date: string, snapshot: WorkSnapshot) { return service.readDailyPlan(dailyPlanUnit(db), actor, date, snapshot); }
export function saveDailyPlan(db: SqlDatabase, actor: WorkContext, raw: unknown, commandKey: string, snapshot: WorkSnapshot) { return service.saveDailyPlan(dailyPlanUnit(db), actor, raw, commandKey, snapshot); }
export { dailyPlanSaveSchema, dailyPlanReferenceKey } from "@erpx/server-work_projection/domain/daily-plan";
export type { DailyPlanSave, DailyPlanReference, DailyPlanWork, DailyPlanView } from "@erpx/server-work_projection/domain/daily-plan";
