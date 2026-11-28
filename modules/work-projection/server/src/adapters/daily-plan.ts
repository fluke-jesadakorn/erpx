import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/workflow-server/public/commands";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { WorkSnapshot } from "@erpx/work-projection-server/domain/work-items";
import { createDailyPlan, type DailyPlanUnit, type DailyPlanResult } from "@erpx/work-projection-server/application/daily-plan";
import type { PlanRow } from "@erpx/work-projection-server/domain/daily-plan";
import type { SqlStatement } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeOwnerWrite, nativeOwnerWriteRow } from "./native-owner-write";
import { dailyPlanSql, noopSql } from "./read-tail-sql";
const service = createDailyPlan<SqlStatement>({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message) });
export function dailyPlanUnit(db: SqlDatabase): DailyPlanUnit<SqlStatement> {
    return {
        // The owner-authored read executes in the mounted native owner-read
        // route; no TypeScript SQL or fallback remains for this read.
        read: (companyId, userId, date) => nativeOwnerWriteRow<PlanRow>('wp_daily_plan_read', [companyId, userId, date], `wp-daily-plan-read:${companyId}:${userId}:${date}`, { companyId }),
        lock: (companyId, userId, date) => db.prepare("SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE").bind(companyId, userId, date),
        version: (key, companyId, userId, date, version) => assertion(db, key, "daily-plan-version", `COALESCE((${dailyPlanSql('version')}),0)=?`, [companyId, userId, date, version]),
        // Native axum owner write executes the plan upsert and its ledger.
        write: async (companyId, userId, date, items, version, key, now, expected) => nativeOwnerWrite('wp_daily_plan_write',[companyId,userId,date,items,version,key,now,expected],`wp-daily-plan:${key}`,{companyId,now},()=>db.prepare(noopSql)),
        written: (key, companyId, userId, date, version) => assertion(db, key, "daily-plan-written", `EXISTS(${dailyPlanSql('1', ' AND version=? AND last_command_id=?')})`, [companyId, userId, date, version, key]),
        replay: (actor, key, id, input) => replayCommand<DailyPlanResult>(db, actor, key, "daily.plan.save", id, input),
        execute: (actor, key, input, id, result, effects) => executeCommand(db, actor, key, "daily.plan.save", input, id, result.version, result, effects)
    };
}
export function readDailyPlan(db: SqlDatabase, actor: Pick<WorkContext, "companyId" | "userId">, date: string, snapshot: WorkSnapshot) { return service.readDailyPlan(dailyPlanUnit(db), actor, date, snapshot); }
export function saveDailyPlan(db: SqlDatabase, actor: WorkContext, raw: unknown, commandKey: string, snapshot: WorkSnapshot) { return service.saveDailyPlan(dailyPlanUnit(db), actor, raw, commandKey, snapshot); }
export { dailyPlanSaveSchema, dailyPlanReferenceKey } from "@erpx/work-projection-server/domain/daily-plan";
export type { DailyPlanSave, DailyPlanReference, DailyPlanWork, DailyPlanView } from "@erpx/work-projection-server/domain/daily-plan";
