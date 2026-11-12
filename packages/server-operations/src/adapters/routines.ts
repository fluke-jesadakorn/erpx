import {withWorkRoutineOccurrence,workRoutineReadScope,workRoutineScheduledReadScope} from '@erpx/server-operations/adapters/work-routine-purchases';
import {operationsRoutineJournalEffects} from '@erpx/server-workflow/public/operations-routine-journal-effects';
import type { SqlDatabase, SqlStatement, SqlValue } from "@erpx/server-runtime/public/runtime/types";
import { companyAuthority, notification, type StaffActor } from "@erpx/server-operations/public/staff-policy";
import {taskRoute,openApproval} from '@erpx/server-workflow/public/approval-planning';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { assertProductionModuleEnabled, productionFeatureGuard, scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import { holdingResources as nativeHoldingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/server-identity_access/public/iam-read-snapshot';
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { type TemplateRow, type ScheduleRow } from '@erpx/server-operations/domain/routines';
import { createRoutinesApplication, type RoutinesRepository } from '@erpx/server-operations/application/routines';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { RoutinesProjectsFacts } from '@erpx/server-projects/public/operations-routines-facts';
import { RoutinesWorkflowFacts } from '@erpx/server-workflow/public/operations-routines-facts';
export { templateSchema } from '@erpx/server-operations/domain/routines';
export type { TemplateRow } from '@erpx/server-operations/domain/routines';
export type { ScheduleRow } from '@erpx/server-operations/domain/routines';
function application(provided?: SqlDatabase,readScope={where:'TRUE',args:[] as SqlValue[]}) {
    let database = provided!;
    const repository: RoutinesRepository<SqlStatement> = { readListTemplatesEffect(companyId: string, userId: string, value: 0 | 1) {
            return database.prepare(`SELECT t.* FROM routine_templates t WHERE t.company_id=? AND (t.published=1 OR t.creator_user_id=? OR ?=1) AND (${readScope.where}) ORDER BY t.updated_at DESC,t.id`)
                .bind(companyId, userId, value,...readScope.args)
                .all<TemplateRow>();
        },
        readGetTemplateRow(id: string, companyId: string) {
            return database.prepare(`SELECT t.* FROM routine_templates t WHERE t.id=? AND t.company_id=? AND (${readScope.where})`)
                .bind(id, companyId,...readScope.args)
                .first<TemplateRow>();
        },
        readOccurrenceStatementsEffect(...args) { return RoutinesProjectsFacts(database).readOccurrenceStatementsEffect(...args); },
        guard17(key: string, value: "template-version", id: string, version: number) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM routine_templates WHERE id=? AND version=? AND published=1)", [id, version]);
        },
        stageOccurrenceStatementsSteps(id: string, company_id: string, branch_id: string | null, department_id: string | null, title: string, description: string, creator: string, assignee_user_id: string, dueAt: number, project_id: string | null, start_approvers_json: string, completion_approvers_json: string, state: "draft" | "start_approval" | "ready", blocked: string | null, id2: string, version: number, value: string | null, occurrenceAt: number | null, now: number, now2: number) {
            return database.prepare("INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,description,creator_user_id,assignee_user_id,due_at,project_id,start_approvers_json,completion_approvers_json,state,blocked_reason,template_id,template_version,schedule_id,occurrence_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id, company_id, branch_id, department_id, title, description, creator, assignee_user_id, dueAt, project_id, start_approvers_json, completion_approvers_json, state, blocked, id2, version, value, occurrenceAt, now, now2);
        },
        async readGenerateOccurrencesSchedules(now: number, value: number) {
            return database.prepare(`SELECT * FROM routine_schedules s WHERE s.state='active' AND s.next_occurrence_at<=? AND ${scheduledFeaturePredicate("s.company_id", "operations")} AND (${await workRoutineScheduledReadScope()}) ORDER BY s.next_occurrence_at,s.id LIMIT ?`)
                .bind(now, value)
                .all<ScheduleRow>();
        },
        readGenerateOccurrencesPause(id: string, at: number, at2: number) {
            return database.prepare("SELECT ended_at FROM routine_schedule_pauses WHERE schedule_id=? AND started_at<=? AND (ended_at IS NULL OR ended_at>?) ORDER BY started_at DESC LIMIT 1")
                .bind(id, at, at2)
                .first<{
                ended_at: number | null;
            }>();
        },
        readGenerateOccurrencesTemplate(template_id: string, company_id: string) {
            return database.prepare("SELECT * FROM routine_templates WHERE id=? AND company_id=? AND published=1")
                .bind(template_id, company_id)
                .first<TemplateRow>();
        },
        readGenerateOccurrencesPrior(id: string, at: number) {
            return database.prepare("SELECT id FROM staff_tasks WHERE schedule_id=? AND occurrence_at=?")
                .bind(id, at)
                .first();
        },
        guard23(key: string, value: "schedule-cursor", id: string, version: number, at: number) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM routine_schedules WHERE id=? AND version=? AND state='active' AND next_occurrence_at=?)", [id, version, at]);
        },
        stageGenerateOccurrencesEffect(next: number | null, next2: number | null, value: 0 | 1, at: number, now: number, id: string) {
            return database.prepare("UPDATE routine_schedules SET next_occurrence_at=?,state=CASE WHEN ?::text IS NULL THEN 'ended' ELSE state END,last_generated_at=CASE WHEN ?=1 THEN ? ELSE last_generated_at END,last_error=NULL,updated_at=? WHERE id=?")
                .bind(next, next2, value, at, now, id);
        },
        stageGenerateOccurrencesEffect2(...args){return operationsRoutineJournalEffects(database).stageGenerateOccurrencesEffect2(...args);},
        stageGenerateOccurrencesEffect3(...args){return operationsRoutineJournalEffects(database).stageGenerateOccurrencesEffect3(...args);},

        stageGenerateOccurrencesEffect4(...args) { return RoutinesWorkflowFacts(database).stageGenerateOccurrencesEffect4(...args); },
        readGenerateOccurrencesLatest(id: string) {
            return database.prepare("SELECT next_occurrence_at,version,state FROM routine_schedules WHERE id=?")
                .bind(id)
                .first<Pick<ScheduleRow, "next_occurrence_at" | "version" | "state">>();
        },
        stageGenerateOccurrencesEffect5(value: string, now: number, id: string, version: number) {
            return database.prepare("UPDATE routine_schedules SET last_error=?,updated_at=? WHERE id=? AND version=?")
                .bind(value, now, id, version)
                .run();
        },
        readGenerateOccurrencesLag(now: number) {
            return database.prepare("SELECT MIN(next_occurrence_at) oldest FROM routine_schedules WHERE state='active' AND next_occurrence_at<=?")
                .bind(now)
                .first<{
                oldest: number | null;
            }>();
        } };
    return createRoutinesApplication({ repository, clock: { now: Date.now }, prepare: async (holding) => { if (!provided)
            database = (await nativeHoldingResources(holding)).write; }, production: process.env.NODE_ENV === 'production', occurrenceUnit:(schedule,at,run)=>withWorkRoutineOccurrence(database,schedule,at,async transaction=>{const original=database;database=transaction;try{return await run();}finally{database=original;}}), batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, companyAuthority: (...args) => fromNative(() => companyAuthority(database, ...args)), notification: (...args) => notification(database, ...args), taskRoute: (...args) => fromNative(() => taskRoute(database, ...args)), openApproval: (...args) => fromNative(() => openApproval(database, ...args)), assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(database, ...args)), productionFeatureGuard: (...args) => productionFeatureGuard(database, ...args) });
}
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> { try {
    return await run();
}
catch (error) {
    if (error instanceof NativeApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new NativeApiError(error.status, error.code, error.message);
    throw error;
} }
export async function listTemplates(db: SqlDatabase, actor: StaffActor) { return invoke(async() => application(db,await workRoutineReadScope(db,actor)).listTemplates(actor)); }
export async function getTemplate(db: SqlDatabase, actor: StaffActor, id: string) { const scope=await workRoutineReadScope(db,actor);return invoke(() => application(db,scope).getTemplate(actor, id)); }
export async function canEditTemplate(db: SqlDatabase, actor: StaffActor, row: TemplateRow) { return invoke(() => application(db).canEditTemplate(actor, row)); }
export async function generateOccurrences(db: SqlDatabase, now = Date.now(), limit = 20) { return invoke(() => application(db).generateOccurrences(now, limit)); }
