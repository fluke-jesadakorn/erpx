import {operationsRoutineJournalEffects} from '../../workflow/public/operations-routine-journal-effects';
import { getProject } from "../../projects/public/staff-projects";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { z } from "zod";
import { validateTaskConfiguration, configGuards } from "@/server/modules/operations/public/staff-tasks";
import { activeStaff, companyAuthority, staffGuard, notification, type StaffActor } from "@/server/modules/operations/public/staff-policy";
import { taskRoute, openApproval } from "@/server/modules/workflow/public/approvals";
import { assertion } from "@/server/modules/workflow/public/commands";
import { assertProductionModuleEnabled, productionFeatureGuard, scheduledFeaturePredicate } from "@/server/modules/pms/public/hospitality-rollout";
import { holdingResources as nativeHoldingResources } from '../../runtime/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '../../identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '../../workflow/public/commands';
import { ApiError as NativeApiError } from '../../runtime/public/http';
import { templateSchema, scheduleSchema, type TemplateRow, type ScheduleRow } from '../domain/routines';
import { createRoutinesApplication, type RoutinesRepository } from '../application/routines';
import { BusinessRuleError } from '../../shared';
import { RoutinesIdentityAccessFacts } from '../../identity_access/public/operations-routines-facts';
import { RoutinesProjectsFacts } from '../../projects/public/operations-routines-facts';
import { RoutinesWorkflowFacts } from '../../workflow/public/operations-routines-facts';
export { templateSchema } from '../domain/routines';
export { scheduleSchema } from '../domain/routines';
export type { TemplateRow } from '../domain/routines';
export type { ScheduleRow } from '../domain/routines';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: RoutinesRepository<SqlStatement> = { readListTemplatesEffect(companyId: string, userId: string, value: 0 | 1) {
            return database.prepare("SELECT * FROM routine_templates WHERE company_id=? AND (published=1 OR creator_user_id=? OR ?=1) ORDER BY updated_at DESC,id")
                .bind(companyId, userId, value)
                .all<TemplateRow>();
        },
        readGetTemplateRow(id: string, companyId: string) {
            return database.prepare("SELECT * FROM routine_templates WHERE id=? AND company_id=?")
                .bind(id, companyId)
                .first<TemplateRow>();
        },
        guard3(...args) { return RoutinesIdentityAccessFacts(database).guard3(...args); },
        guard4(key: string, value: "template-version", recordId: string, expectedVersion: number) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM routine_templates WHERE id=? AND version=?)", [recordId, expectedVersion]);
        },
        stageSaveTemplateEffect(title: string, description: string, assigneeUserId: string, projectId: string | null, value: string, value2: string, value3: 0 | 1, now: number, recordId: string) {
            return database.prepare("UPDATE routine_templates SET title=?,description=?,assignee_user_id=?,project_id=?,start_approvers_json=?,completion_approvers_json=?,published=?,version=version+1,updated_at=? WHERE id=?")
                .bind(title, description, assigneeUserId, projectId, value, value2, value3, now, recordId);
        },
        stageSaveTemplateEffect2(recordId: string, companyId: string, branch_id: string | null, department_id: string | null, title: string, description: string, userId: string, assigneeUserId: string, projectId: string | null, value: string, value2: string, value3: 0 | 1, now: number, now2: number) {
            return database.prepare("INSERT INTO routine_templates(id,company_id,branch_id,department_id,title,description,creator_user_id,assignee_user_id,project_id,start_approvers_json,completion_approvers_json,published,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(recordId, companyId, branch_id, department_id, title, description, userId, assigneeUserId, projectId, value, value2, value3, now, now2);
        },
        guard7(key: string, value: "schedule-version", recordId: string, templateId: string, companyId: string, expectedVersion: number) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM routine_schedules WHERE id=? AND template_id=? AND company_id=? AND version=? AND state='paused')", [recordId, templateId, companyId, expectedVersion]);
        },
        stageSaveScheduleEffect(frequency: "daily" | "weekly" | "monthly", timezone: string, occurrence_time: string, start_date: string, end_date: string | null, weekday: number | null, month_day: number | null, dueOffsetHours: number, next: number | null, now: number, recordId: string) {
            return database.prepare("UPDATE routine_schedules SET frequency=?,timezone=?,occurrence_time=?,start_date=?,end_date=?,weekday=?,month_day=?,due_offset_hours=?,next_occurrence_at=?,last_error=NULL,version=version+1,updated_at=? WHERE id=?")
                .bind(frequency, timezone, occurrence_time, start_date, end_date, weekday, month_day, dueOffsetHours, next, now, recordId);
        },
        stageSaveScheduleEffect2(recordId: string, templateId: string, companyId: string, frequency: "daily" | "weekly" | "monthly", timezone: string, occurrence_time: string, start_date: string, end_date: string | null, weekday: number | null, month_day: number | null, dueOffsetHours: number, next: number | null, now: number, now2: number, now3: number) {
            return database.prepare("INSERT INTO routine_schedules(id,template_id,company_id,frequency,timezone,occurrence_time,start_date,end_date,weekday,month_day,due_offset_hours,next_occurrence_at,paused_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(recordId, templateId, companyId, frequency, timezone, occurrence_time, start_date, end_date, weekday, month_day, dueOffsetHours, next, now, now2, now3);
        },
        stageSaveScheduleEffect3(key: string, recordId: string, now: number) {
            return database.prepare("INSERT INTO routine_schedule_pauses(id,schedule_id,started_at) VALUES(?,?,?)")
                .bind(key, recordId, now);
        },
        readChangeScheduleStateRow(id: string, companyId: string) {
            return database.prepare("SELECT * FROM routine_schedules WHERE id=? AND company_id=?")
                .bind(id, companyId)
                .first<ScheduleRow>();
        },
        guard12(key: string, value: "schedule-version", id: string, version: number, value2: "paused" | "active") {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM routine_schedules WHERE id=? AND version=? AND state=?)", [id, version, value2]);
        },
        stageChangeScheduleStateEffect(value: "paused" | "active", value2: number | null, now: number, id: string) {
            return database.prepare("UPDATE routine_schedules SET state=?,paused_at=?,version=version+1,updated_at=? WHERE id=?")
                .bind(value, value2, now, id);
        },
        stageChangeScheduleStateEffect2(now: number, id: string) {
            return database.prepare("UPDATE routine_schedule_pauses SET ended_at=? WHERE schedule_id=? AND ended_at IS NULL")
                .bind(now, id);
        },
        stageChangeScheduleStateEffect3(key: string, id: string, now: number) {
            return database.prepare("INSERT INTO routine_schedule_pauses(id,schedule_id,started_at) VALUES(?,?,?)")
                .bind(key, id, now);
        },
        readOccurrenceStatementsEffect(...args) { return RoutinesProjectsFacts(database).readOccurrenceStatementsEffect(...args); },
        guard17(key: string, value: "template-version", id: string, version: number) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM routine_templates WHERE id=? AND version=? AND published=1)", [id, version]);
        },
        stageOccurrenceStatementsSteps(id: string, company_id: string, branch_id: string | null, department_id: string | null, title: string, description: string, creator: string, assignee_user_id: string, dueAt: number, project_id: string | null, start_approvers_json: string, completion_approvers_json: string, state: "draft" | "start_approval" | "ready", blocked: string | null, id2: string, version: number, value: string | null, occurrenceAt: number | null, now: number, now2: number) {
            return database.prepare("INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,description,creator_user_id,assignee_user_id,due_at,project_id,start_approvers_json,completion_approvers_json,state,blocked_reason,template_id,template_version,schedule_id,occurrence_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id, company_id, branch_id, department_id, title, description, creator, assignee_user_id, dueAt, project_id, start_approvers_json, completion_approvers_json, state, blocked, id2, version, value, occurrenceAt, now, now2);
        },
        readGenerateOccurrencesSchedules(now: number, value: number) {
            return database.prepare(`SELECT * FROM routine_schedules s WHERE s.state='active' AND s.next_occurrence_at<=? AND ${scheduledFeaturePredicate("s.company_id", "operations")} ORDER BY s.next_occurrence_at,s.id LIMIT ?`)
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
        ...operationsRoutineJournalEffects(database),
        
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
    return createRoutinesApplication({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => { if (!provided)
            database = (await nativeHoldingResources(holding)).write; }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, getProject: (...args) => fromNative(() => getProject(database, ...args)), validateTaskConfiguration: (...args) => fromNative(() => validateTaskConfiguration(database, ...args)), configGuards: (...args) => configGuards(database, ...args), activeStaff: (...args) => fromNative(() => activeStaff(database, ...args)), companyAuthority: (...args) => fromNative(() => companyAuthority(database, ...args)), staffGuard: (...args) => staffGuard(database, ...args), notification: (...args) => notification(database, ...args), taskRoute: (...args) => fromNative(() => taskRoute(database, ...args)), openApproval: (...args) => fromNative(() => openApproval(database, ...args)), assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(database, ...args)), productionFeatureGuard: (...args) => productionFeatureGuard(database, ...args) });
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
export async function listTemplates(db: SqlDatabase, actor: StaffActor) { return invoke(() => application(db).listTemplates(actor)); }
export async function getTemplate(db: SqlDatabase, actor: StaffActor, id: string) { return invoke(() => application(db).getTemplate(actor, id)); }
export async function canEditTemplate(db: SqlDatabase, actor: StaffActor, row: TemplateRow) { return invoke(() => application(db).canEditTemplate(actor, row)); }
export async function saveTemplate(db: SqlDatabase, actor: StaffActor, id: string | null, input: z.infer<typeof templateSchema>, key: string, cloneId?: string) { return invoke(() => application(db).saveTemplate(actor, id, input, key, cloneId)); }
export async function saveSchedule(db: SqlDatabase, actor: StaffActor, templateId: string, id: string | null, input: z.infer<typeof scheduleSchema>, key: string) { return invoke(() => application(db).saveSchedule(actor, templateId, id, input, key)); }
export async function changeScheduleState(db: SqlDatabase, actor: StaffActor, id: string, version: number, resume: boolean, key: string) { return invoke(() => application(db).changeScheduleState(actor, id, version, resume, key)); }
export async function runTemplate(db: SqlDatabase, actor: StaffActor, id: string, dueAt: number, key: string) { return invoke(() => application(db).runTemplate(actor, id, dueAt, key)); }
export async function generateOccurrences(db: SqlDatabase, now = Date.now(), limit = 20) { return invoke(() => application(db).generateOccurrences(now, limit)); }
