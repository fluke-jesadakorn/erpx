import { OPEN_PERIOD_GUARD_SQL, OPEN_PERIOD_IDS_SQL } from './finance-read-tail-r16-sql';
import { withFinancePurchasedCommand, withFinancePurchasedRead } from "@erpx/server-finance/adapters/purchase-boundary";
import { nativeFinanceAccruals } from '@erpx/server-collaboration/adapters/native-finance-accruals';
import { createAccruals } from '@erpx/server-finance/application/accruals';
import { accrualScheduleSchema, accrualScheduleDecisionSchema, accrualRunPostSchema, type AccrualRow } from '@erpx/server-finance/domain/accruals';
import "server-only";
import { z } from "zod";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { ApiError } from "@erpx/server-runtime/public/http";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export { accrualScheduleSchema, accrualScheduleDecisionSchema, accrualRunPostSchema } from '@erpx/server-finance/domain/accruals';
async function hash(value: unknown) { const bytes = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(JSON.stringify(value))); return Array.from(new Uint8Array(bytes), byte => byte.toString(16).padStart(2, "0")).join(""); }
function accountSql(write: SqlDatabase, actor: WorkContext, input: z.infer<typeof accrualScheduleSchema>) { return write.prepare(`SELECT a.id,a.chart_id,a.currency,a.status,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.id IN (?,?) AND a.company_id=? AND a.status='active'`).bind(input.debitAccountId, input.creditAccountId, actor.companyId); }
export async function accrualSchedulePage(actor: WorkContext) { return withFinancePurchasedRead(actor,db=>accrualService(db).accrualSchedulePage(actor)); }
export async function createAccrualSchedule(actor: WorkContext, input: z.infer<typeof accrualScheduleSchema>, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.accrual.create",purpose:"new-work"},tx=>accrualService(tx).createAccrualSchedule(actor, input, key)); }
export async function decideAccrualSchedule(actor: WorkContext, scheduleId: string, input: z.infer<typeof accrualScheduleDecisionSchema>, action: "approve" | "cancel", key: string) { return withFinancePurchasedCommand(actor,{key,operation:`finance.accrual.${action}`,purpose:"settle-existing"},tx=>accrualService(tx).decideAccrualSchedule(actor, scheduleId, input, action, key)); }
export async function prepareAccrualRun(actor: WorkContext, scheduleId: string, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.accrual.prepare",purpose:"settle-existing"},tx=>accrualService(tx).prepareAccrualRun(actor, scheduleId, key)); }
export async function postAccrualRun(actor: WorkContext, runId: string, input: z.infer<typeof accrualRunPostSchema>, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.accrual.post",purpose:"settle-existing"},tx=>accrualService(tx).postAccrualRun(actor, runId, input, key)); }
/** Read by ID independently of the register's page limits. */
export async function accrualRecordDetail(actor: WorkContext, kind: "schedule" | "run", id: string) { return withFinancePurchasedRead(actor,db=>accrualService(db).accrualRecordDetail(actor, kind, id)); }
function accrualService(provided?:SqlDatabase) {
    let pending: Promise<SqlDatabase> | undefined;
    const db = (actor: WorkContext) => provided ? Promise.resolve(provided) : pending ??= holdingResources(actor.holdingId).then(r => r.write);
    return createAccruals({ error: (...args) => new ApiError(...args), permission: requirePermission, ready: async (actor) => { await db(actor); }, now: Date.now, hash, replay: async (actor, key, operation, id, input) => replayCommand(await db(actor), actor, key, operation, id, input), schedules: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare(`SELECT s.*,da.code debit_code,da.name debit_name,ca.code credit_code,ca.name credit_name FROM finance_accrual_schedules s JOIN finance_accounts da ON da.id=s.debit_account_id JOIN finance_accounts ca ON ca.id=s.credit_account_id WHERE s.company_id=? ORDER BY s.created_at DESC LIMIT 100`).bind(actor.companyId).all<AccrualRow>()).results; }, runs: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare(`SELECT r.*,s.description FROM finance_accrual_schedule_runs r JOIN finance_accrual_schedules s ON s.id=r.schedule_id WHERE r.company_id=? ORDER BY r.period_start DESC,r.id DESC LIMIT 200`).bind(actor.companyId).all<AccrualRow>()).results; }, activeAccounts: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT id,code,name,currency FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY code").bind(actor.companyId).all<AccrualRow>()).results; }, accounts: async (facts) => { const { actor, input } = facts; const write = await db(actor); return (await accountSql(write, actor, input).all<{
            id: string;
            chart_id: string;
            currency: string;
            status: string;
            base_currency: string;
        }>()).results; }, commitCreateAccrualSchedule: async (facts) => {
            const { actor, key, input, id, result, total, now, count } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, "finance.accrual.create", input, id, 1, result, [
                await nativeFinanceAccruals(write, actor.companyId).scheduleInsert([id, actor.companyId, input.description, input.debitAccountId, input.creditAccountId, input.currency, total, input.startDate, input.endDate, input.startDate, input.frequency, actor.userId, now, now]),
                await nativeFinanceAccruals(write, actor.companyId).eventCreated([`accrual-event-${key}`, id, actor.companyId, "created", actor.userId, "Schedule prepared for independent approval", JSON.stringify({ totalMinor: total, periodCount: count, frequency: input.frequency }), now]),
            ]);
        }, scheduleDecisionRow: async (facts) => { const { scheduleId, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_accrual_schedules WHERE id=? AND company_id=?").bind(scheduleId, actor.companyId).first<Record<string, string | number | null>>(); }, commitDecideAccrualSchedule: async (facts) => {
            const { actor, key, action, input, scheduleId, version, status, row, now } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, `finance.accrual.${action}`, input, scheduleId, version, { id: scheduleId, status, version }, [
                assertion(write, key, "schedule-version", "EXISTS(SELECT 1 FROM finance_accrual_schedules WHERE id=? AND company_id=? AND version=? AND status=?)", [scheduleId, actor.companyId, input.expectedVersion, row.status]),
                await nativeFinanceAccruals(write, actor.companyId).scheduleDecision([status, status, actor.userId, now, scheduleId]),
                await nativeFinanceAccruals(write, actor.companyId).eventDecision([`accrual-event-${key}`, scheduleId, actor.companyId, action === "approve" ? "approved" : "cancelled", actor.userId, version, input.reason, JSON.stringify({ status }), now]),
            ]);
        }, schedule: async (facts) => { const { scheduleId, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_accrual_schedules WHERE id=? AND company_id=?").bind(scheduleId, actor.companyId).first<Record<string, string | number | null>>(); }, openPeriod: async (facts) => { const { actor, periodStart, periodEndValue } = facts; const write = await db(actor); return write.prepare(OPEN_PERIOD_IDS_SQL).bind(actor.companyId, periodStart, periodEndValue).first<{
            id: string;
        }>(); }, existingRun: async (facts) => { const { scheduleId, periodStart, actor } = facts; const write = await db(actor); return write.prepare("SELECT id FROM finance_accrual_schedule_runs WHERE schedule_id=? AND period_start=?").bind(scheduleId, periodStart).first<AccrualRow>(); }, chart: async (facts) => { const { schedule, actor } = facts; const write = await db(actor); return write.prepare("SELECT chart_id FROM finance_accounts WHERE id=? AND company_id=? AND status='active'").bind(schedule.debit_account_id, actor.companyId).first<{
            chart_id: string;
        }>(); }, commitPrepareAccrualRun: async (facts) => {
            const { actor, key, scheduleId, runId, result, schedule, journalId, chart, period, number, periodEndValue, periodStart, now, amount, sourceHash } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, "finance.accrual.prepare", { scheduleId }, runId, 1, result, [
                assertion(write, key, "schedule-version", "EXISTS(SELECT 1 FROM finance_accrual_schedules WHERE id=? AND company_id=? AND status='active' AND version=?)", [scheduleId, actor.companyId, schedule.version]),
                await nativeFinanceAccruals(write, actor.companyId).journalInsert([journalId, actor.companyId, chart.chart_id, period.id, number, periodEndValue, schedule.currency, 1000000, "accrual_schedule", runId, `Accrual · ${schedule.description} · ${periodStart} to ${periodEndValue}`, actor.userId, now, now]),
                await nativeFinanceAccruals(write, actor.companyId).journalLinesInsert([`${journalId}-0`, journalId, schedule.debit_account_id, schedule.description, amount, 0, amount, 0, "{}", `${journalId}-1`, journalId, schedule.credit_account_id, schedule.description, 0, amount, 0, amount, "{}"]),
                await nativeFinanceAccruals(write, actor.companyId).runInsert([runId, scheduleId, actor.companyId, periodStart, periodEndValue, amount, schedule.currency, journalId, Number(schedule.version), sourceHash, actor.userId, now, now]),
                await nativeFinanceAccruals(write, actor.companyId).eventRun([`accrual-event-${key}`, scheduleId, runId, actor.companyId, "prepared", actor.userId, Number(schedule.version), 1, "Prepare period accrual", JSON.stringify({ periodStart, periodEnd: periodEndValue, amountMinor: amount, sourceHash }), now]),
            ]);
        }, run: async (facts) => { const { runId, actor } = facts; const write = await db(actor); return write.prepare(`SELECT r.*,s.description,s.frequency,s.start_date,s.end_date,s.next_run_date,s.version schedule_current_version,s.status schedule_status,j.chart_id,j.period_id,j.entry_date,j.status journal_status,j.journal_number FROM finance_accrual_schedule_runs r JOIN finance_accrual_schedules s ON s.id=r.schedule_id JOIN finance_journals j ON j.id=r.journal_id WHERE r.id=? AND r.company_id=?`).bind(runId, actor.companyId).first<Record<string, string | number | null>>(); }, commitPostAccrualRun: async (facts) => {
            const { actor, key, input, runId, version, row, now, next, complete } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, "finance.accrual.post", input, runId, version, { id: runId, status: "posted", version, journalId: row.journal_id }, [
                assertion(write, key, "run-version", "EXISTS(SELECT 1 FROM finance_accrual_schedule_runs WHERE id=? AND company_id=? AND status='prepared' AND version=? AND schedule_version=?)", [runId, actor.companyId, input.expectedVersion, row.schedule_version]),
                assertion(write, key, "period-open", "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open')", [row.period_id, actor.companyId]),
                await nativeFinanceAccruals(write, actor.companyId).journalPost([actor.userId, now, now, row.journal_id]),
                await nativeFinanceAccruals(write, actor.companyId).runPost([actor.userId, now, runId]),
                await nativeFinanceAccruals(write, actor.companyId).schedulePost([next, complete, now, row.schedule_id, row.schedule_version]),
                await nativeFinanceAccruals(write, actor.companyId).eventRun([`accrual-event-${key}`, row.schedule_id, runId, actor.companyId, "posted", actor.userId, Number(row.schedule_version) + 1, version, input.reason, JSON.stringify({ journalId: row.journal_id, periodStart: row.period_start, periodEnd: row.period_end, amountMinor: row.amount_minor }), now]),
            ]);
        }, scheduleDetail: async (facts) => { const { id, actor } = facts; const write = await db(actor); return write.prepare("SELECT s.*,da.code debit_code,da.name debit_name,ca.code credit_code,ca.name credit_name FROM finance_accrual_schedules s JOIN finance_accounts da ON da.id=s.debit_account_id JOIN finance_accounts ca ON ca.id=s.credit_account_id WHERE s.id=? AND s.company_id=?").bind(id, actor.companyId).first<AccrualRow>(); }, runDetail: async (facts) => { const { id, actor } = facts; const write = await db(actor); return write.prepare("SELECT r.*,s.description FROM finance_accrual_schedule_runs r JOIN finance_accrual_schedules s ON s.id=r.schedule_id WHERE r.id=? AND r.company_id=?").bind(id, actor.companyId).first<AccrualRow>(); } });
}
