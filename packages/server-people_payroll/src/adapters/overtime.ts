import {workforcePurchaseControls,withWorkforcePurchasedRead,withWorkforcePurchasedCommand} from "@erpx/server-people_payroll/adapters/workforce-purchases";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { activeStaff, requireCompanyAuthority } from "@erpx/server-operations/public/staff-policy";
import { peopleRequestRoute } from "@erpx/server-organization/public/department-authority";
import { approvalView, openApproval, prepareDecision, closeApprovals, participantSql } from "@erpx/server-workflow/public/approvals";
import { executeCommand, replayCommand } from "@erpx/server-workflow/public/commands";
import { ApiError } from "@erpx/server-runtime/public/http";
import { overtimeCommandSchema, type OvertimeDetails } from "@erpx/feature-people_payroll/lib/overtime";
import { prepareAttachmentBinding } from "@erpx/server-files/public/attachments/service";
import { isTransactionalSqlDatabase, type SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { overtimeEvidenceFacts } from '@erpx/server-files/public/overtime-evidence-facts';
import { overtimeNotification } from '@erpx/server-collaboration/public/overtime-notification';
import { type OvertimeRow } from '@erpx/server-people_payroll/domain/overtime';
import { createOvertimeApplication, type OvertimeRepository, type OvertimePorts } from '@erpx/server-people_payroll/application/overtime';
export type { OvertimeRow } from '@erpx/server-people_payroll/domain/overtime';
export type OvertimeDetail=Awaited<ReturnType<typeof getOvertime>>;
function serviceContext(provided?: SqlDatabase,providedOwned=false): OvertimePorts<SqlStatement> {
    let database = provided!;
    const repository: OvertimeRepository<SqlStatement> = { readOvertimeSetupEligibility(companyId: string, engagement_id: string) {
            return database.prepare("SELECT eligible FROM overtime_eligibility WHERE company_id=? AND engagement_id=?").bind(companyId, engagement_id).first<{
                eligible: boolean;
            }>();
        },
        readPreviewOvertimeTime(startLocal: string, timezone: string, endLocal: string, timezone2: string, startLocal2: string, timezone3: string, timezone4: string, endLocal2: string, timezone5: string, timezone6: string) {
            return database.prepare(`SELECT (extract(epoch FROM (?::timestamp AT TIME ZONE ?))*1000)::bigint starts_at,(extract(epoch FROM (?::timestamp AT TIME ZONE ?))*1000)::bigint ends_at,
 to_char((?::timestamp AT TIME ZONE ?) AT TIME ZONE ?,'YYYY-MM-DD"T"HH24:MI') local_start,to_char((?::timestamp AT TIME ZONE ?) AT TIME ZONE ?,'YYYY-MM-DD"T"HH24:MI') local_end`)
                .bind(startLocal, timezone, endLocal, timezone2, startLocal2, timezone3, timezone4, endLocal2, timezone5, timezone6).first<{
                starts_at: number;
                ends_at: number;
                local_start: string;
                local_end: string;
            }>();
        },
        readPreviewOvertimeOverlap(companyId: string, engagement_id: string, excludeId: string | null, ends_at: number, starts_at: number) { return database.prepare(`SELECT id FROM overtime_requests WHERE company_id=? AND engagement_id=? AND id IS DISTINCT FROM ? AND state NOT IN ('draft','withdrawn','rejected') AND starts_at<? AND ends_at>? LIMIT 1`).bind(companyId, engagement_id, excludeId, ends_at, starts_at).first(); },
        readPreviewOvertimeEffect(rosterShiftId: string, companyId: string, engagement_id: string) { return database.prepare(`SELECT 1 FROM people_roster_shifts s JOIN people_roster_weeks w ON w.id=s.week_id JOIN people_roster_assignments a ON a.shift_id=s.id WHERE s.id=? AND w.company_id=? AND w.state='published' AND s.state='active' AND a.state='assigned' AND a.engagement_id=?`).bind(rosterShiftId, companyId, engagement_id).first(); },
        readPreviewOvertimeReviewers(companyId: string, userId: string) {
            return database.prepare('SELECT legal_name FROM hr_people WHERE company_id=? AND user_id=?').bind(companyId, userId).first<{
                legal_name: string;
            }>();
        },
        readGetOvertimeRow(id: string, companyId: string, userId: string, userId2: string, assignmentId: string) { return database.prepare(`SELECT o.* FROM overtime_requests o WHERE o.id=? AND o.company_id=? AND (o.created_by=? OR ${participantSql('overtime', 'o', '?', '?')})`).bind(id, companyId, userId, userId2, assignmentId).first<OvertimeRow>(); },
        readGetOvertimeHandoff(id: string, companyId: string) { return database.prepare('SELECT * FROM overtime_payroll_handoffs WHERE request_id=? AND company_id=?').bind(id, companyId).first(); },
        readListOvertimeRows(companyId: string, userId: string, userId2: string, assignmentId: string) {
            return database.prepare(`SELECT o.id FROM overtime_requests o WHERE o.company_id=? AND (o.created_by=? OR ${participantSql('overtime', 'o', '?', '?')}) ORDER BY o.updated_at DESC LIMIT 100`).bind(companyId, userId, userId2, assignmentId).all<{
                id: string;
            }>();
        },
        stageCreateOvertimeEffect(id: string, companyId: string, engagement_id: string, person_id: string, branch_id: string | null, department_id: string | null, userId: string, mode: "advance" | "retrospective", timezone: string, startLocal: string, endLocal: string, startsAt: number, endsAt: number, breakMinutes: number, minutes: number, reason: string, lateExplanation: string, rosterShiftId: string | null, now: number, now2: number) { return database.prepare(`INSERT INTO overtime_requests(id,company_id,engagement_id,person_id,branch_id,department_id,created_by,mode,timezone,start_local,end_local,starts_at,ends_at,break_minutes,minutes,reason,late_explanation,roster_shift_id,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).bind(id, companyId, engagement_id, person_id, branch_id, department_id, userId, mode, timezone, startLocal, endLocal, startsAt, endsAt, breakMinutes, minutes, reason, lateExplanation, rosterShiftId, now, now2); },
        readCommandOvertimeEffect(id: string, companyId: string) { return database.prepare('SELECT id FROM overtime_requests WHERE id=? AND company_id=? FOR UPDATE').bind(id, companyId).first(); },
        readCommandOvertimeRecord(id: string, companyId: string) { return database.prepare("SELECT * FROM overtime_requests WHERE id=? AND company_id=?").bind(id, companyId).first<OvertimeRow>(); },
        readCommandOvertimeSets(...args) { return overtimeEvidenceFacts(database).readCommandOvertimeSets(...args); },
        stageCommandOvertimeEffect(startLocal: string, endLocal: string, timezone: string, startsAt: number, endsAt: number, breakMinutes: number, minutes: number, reason: string, lateExplanation: string, rosterShiftId: string | null, id: string) { return database.prepare('UPDATE overtime_requests SET start_local=?,end_local=?,timezone=?,starts_at=?,ends_at=?,break_minutes=?,minutes=?,reason=?,late_explanation=?,roster_shift_id=? WHERE id=?').bind(startLocal, endLocal, timezone, startsAt, endsAt, breakMinutes, minutes, reason, lateExplanation, rosterShiftId, id); },
        stageCommandOvertimeEffect2(value: string, id: string, authorization: boolean) { return database.prepare(`UPDATE overtime_requests SET ${authorization ? 'planned_snapshot' : 'actuals_snapshot'}=?::jsonb,decision_note='' WHERE id=?`).bind(value, id); },
        readCommandOvertimeConflict(companyId: string, engagement_id: string, value: string, value2: string) { return database.prepare("SELECT id FROM hr_time_entries WHERE company_id=? AND engagement_id=? AND work_date BETWEEN ? AND ? AND overtime_hours>0 AND status IN ('draft','submitted','approved') LIMIT 1").bind(companyId, engagement_id, value, value2).first(); },
        stageCommandOvertimeEffect3(value: string, id: string, companyId: string, engagement_id: string, version: number, value2: string, minutes: number, timezone: string, userId: string, now: number) { return database.prepare('INSERT INTO overtime_payroll_handoffs(id,request_id,company_id,engagement_id,request_version,work_date,minutes,timezone,approved_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)').bind(value, id, companyId, engagement_id, version, value2, minutes, timezone, userId, now); },
        stageCommandOvertimeEffect4(note: string, id: string) { return database.prepare('UPDATE overtime_requests SET decision_note=? WHERE id=?').bind(note, id); },
        stageCommandOvertimeEffect5(state: string, version: number, now: number, id: string) { return database.prepare('UPDATE overtime_requests SET state=?,version=?,updated_at=? WHERE id=?').bind(state, version, now, id); },
        readSetOvertimeEligibilityEffect(engagementId: string, companyId: string) { return database.prepare('SELECT id FROM hr_engagements WHERE id=? AND company_id=? FOR UPDATE').bind(engagementId, companyId).first(); },
        readSetOvertimeEligibilityCurrent(companyId: string, engagementId: string) {
            return database.prepare('SELECT version FROM overtime_eligibility WHERE company_id=? AND engagement_id=?').bind(companyId, engagementId).first<{
                version: number;
            }>();
        },
        stageSetOvertimeEligibilityEffect(topLevel: boolean, engagementId: string, companyId: string) { return database.prepare('UPDATE hr_engagements SET self_service_top_level=?,version=version+1 WHERE id=? AND company_id=?').bind(topLevel, engagementId, companyId); },
        stageSetOvertimeEligibilityEffect2(companyId: string, engagementId: string, eligible: boolean, reason: string, userId: string, now: number, version: number) { return database.prepare('INSERT INTO overtime_eligibility(company_id,engagement_id,eligible,reason,updated_by,updated_at,version) VALUES(?,?,?,?,?,?,?) ON CONFLICT(company_id,engagement_id) DO UPDATE SET eligible=excluded.eligible,reason=excluded.reason,updated_by=excluded.updated_by,updated_at=excluded.updated_at,version=excluded.version').bind(companyId, engagementId, eligible, reason, userId, now, version); } };
    return { repository, clock: { now: Date.now }, prepare: async (holding) => {
            if (!provided)
                database = (await holdingResources(holding)).write;
        }, commands: { replay: (...args) => replayCommand(database, ...args), commit: (...args) => executeCommand(database, ...args) }, activeStaff: (...args) => fromNative(() => activeStaff(database, ...args)), notification: (...args) => overtimeNotification(database, ...args), requireCompanyAuthority: (...args) => fromNative(() => requireCompanyAuthority(database, ...args)), peopleRequestRoute: (...args) => fromNative(() => peopleRequestRoute(database, ...args)), approvalView: (...args) => fromNative(() => approvalView(database, ...args)), openApproval: (...args) => fromNative(() => openApproval(database, ...args)), prepareDecision: (...args) => fromNative(() => prepareDecision(database, ...args)), closeApprovals: (...args) => closeApprovals(database, ...args), prepareAttachmentBinding: (...args) => fromNative(() => prepareAttachmentBinding(database, ...args)), transaction: run => {
            if(providedOwned)return run(serviceContext(database,true));
            if (!isTransactionalSqlDatabase(database))
                throw new Error('OVERTIME_TRANSACTION_REQUIRED');
            return database.transaction(tx => run(serviceContext(tx)));
        } };
}
async function fromNative<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof ApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function invoke<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function overtimeSetup(db: SqlDatabase, actor: WorkContext) { return withWorkforcePurchasedRead(actor,tx=>invoke(() => createOvertimeApplication(serviceContext(tx)).overtimeSetup(actor)),db); }
export async function previewOvertime(actor: WorkContext, raw: OvertimeDetails, actual = false, excludeId: string | null = null, database?: SqlDatabase) { return withWorkforcePurchasedRead(actor,tx=>invoke(() => createOvertimeApplication(serviceContext(tx)).previewOvertime(actor,raw,actual,excludeId)),database); }
export async function getOvertime(actor: WorkContext, id: string, database?: SqlDatabase) { return withWorkforcePurchasedRead(actor,async tx=>{const data=await invoke(() => createOvertimeApplication(serviceContext(tx)).getOvertime(actor,id)),purchase=await workforcePurchaseControls(actor,tx);return {...data,...(purchase?{purchase}:{})};},database); }
export async function listOvertime(actor: WorkContext) { return withWorkforcePurchasedRead(actor,tx=>invoke(() => createOvertimeApplication(serviceContext(tx)).listOvertime(actor))); }
export async function createOvertime(actor: WorkContext, raw: OvertimeDetails, key: string) { return withWorkforcePurchasedCommand(actor,{key,operation:"overtime.create",purpose:"new-work",resultKind:"overtime"},tx=>invoke(() => createOvertimeApplication(serviceContext(tx,Boolean(tx))).createOvertime(actor,raw,key))); }
export async function commandOvertime(actor: WorkContext, id: string, raw: z.infer<typeof overtimeCommandSchema>, key: string) { return withWorkforcePurchasedCommand(actor,{key,operation:`overtime.${raw.action}`,purpose:["save","submit"].includes(raw.action)?"new-work":"settle-existing",target:{kind:"overtime",id}},tx=>invoke(() => createOvertimeApplication(serviceContext(tx,Boolean(tx))).commandOvertime(actor,id,raw,key))); }
export async function overtimeWork(actor: WorkContext) { return withWorkforcePurchasedRead(actor,async tx=>{const rows=await invoke(() => createOvertimeApplication(serviceContext(tx)).overtimeWork(actor)),purchase=await workforcePurchaseControls(actor,tx);return !purchase?rows:rows.map(row=>((["draft","rejected"].includes(row.status)?purchase.newWork:purchase.settleExisting)?row:({...row,canAct:false,queue:row.queue==="completed"?"completed" as const:"waiting" as const})));}); }
export async function setOvertimeEligibility(actor: WorkContext, raw: unknown, key: string) { return withWorkforcePurchasedCommand(actor,{key,operation:"overtime.eligibility",purpose:"new-work",permission:"leave.policy.manage",target:{kind:"configuration",id:actor.companyId}},tx=>invoke(() => createOvertimeApplication(serviceContext(tx,Boolean(tx))).setOvertimeEligibility(actor,raw,key))); }
