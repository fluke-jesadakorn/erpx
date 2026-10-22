import {peoplePackageEnforcementMode} from "@erpx/server-commercial/public/purchase-command-boundary";
import {workforcePurchaseControls,withWorkforcePurchasedRead,withWorkforcePurchasedCommand,withWorkforceAccrualTransaction} from "@erpx/server-people_payroll/adapters/workforce-purchases";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { activeStaff, requireCompanyAuthority } from "@erpx/server-operations/public/staff-policy";
import { ApiError } from "@erpx/server-runtime/public/http";
import { executeCommand, replayCommand } from "@erpx/server-workflow/public/commands";
import { openApproval } from "@erpx/server-workflow/public/approvals";
import { leaveCreateSchema, leavePreviewSchema } from "@erpx/feature-people_payroll/lib/leave-self-service";
import { z } from "zod";
import { peopleRequestRoute } from "@erpx/server-organization/public/department-authority";
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/hospitality-rollout";
import { availableLeaveEvidence } from "@erpx/server-people_payroll/public/leave-evidence";
import { isTransactionalSqlDatabase, type SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { type Policy, type LeaveType, leaveListFilterSchema, type LeaveListRow, leaveEditSchema, leaveAdminSchema } from '@erpx/server-people_payroll/domain/leave-self-service';
import { createLeaveSelfServiceApplication, type LeaveSelfServicePorts, type LeaveSelfServiceRepository } from '@erpx/server-people_payroll/application/leave-self-service';
import { leaveEmployeeFacts } from '@erpx/server-operations/public/leave-self-service-facts';
export { leaveAdminSchema, leaveEditSchema, leaveListFilterSchema, type LeaveCalendarMonth } from '@erpx/server-people_payroll/domain/leave-self-service';
async function rows<T>(db: SqlDatabase, sql: string, ...args: unknown[]) {
    return (await db.prepare(sql).bind(...args).all<T>()).results;
}
function serviceContext(provided?: SqlDatabase,providedOwned=false): LeaveSelfServicePorts<SqlStatement> {
    let database = provided!;
    const r: LeaveSelfServiceRepository<SqlStatement> = { readPolicyForMatches(companyId: string, engagement_id: string, date: string, date2: string, date3: string, date4: string) {
            return rows<Policy>(database, `SELECT a.id assignment_id,p.id policy_version_id,p.name policy_name,p.version policy_version,p.timezone,p.workweek_json,p.allow_negative,a.holiday_calendar_id
    FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id AND p.company_id=a.company_id
    WHERE a.company_id=? AND a.engagement_id=? AND p.status='active' AND a.effective_from<=? AND (a.effective_to IS NULL OR a.effective_to>=?)
    AND p.effective_from<=? AND (p.effective_to IS NULL OR p.effective_to>=?) ORDER BY a.effective_from DESC,p.version DESC`, companyId, engagement_id, date, date2, date3, date4);
        },
        readBalanceEffect(companyId: string, engagementId: string, leaveTypeId: string, year: number) {
            return database.prepare(`SELECT
    coalesce(sum(units),0)::float available,
    coalesce(-sum(units) FILTER (WHERE kind IN ('reservation','reservation_release')),0)::float reserved,
    coalesce(-sum(units) FILTER (WHERE kind IN ('usage','usage_reversal')),0)::float used,
    coalesce(sum(units) FILTER (WHERE kind='accrual'),0)::float accrued
    FROM leave_balance_entries WHERE company_id=? AND engagement_id=? AND leave_type_id=? AND entitlement_year=?`)
                .bind(companyId, engagementId, leaveTypeId, year)
                .first<{
                available: number;
                reserved: number;
                used: number;
                accrued: number;
            }>();
        },
        readPreviewLeaveType(policy_version_id: string, leaveTypeId: string, companyId: string, startDate: string, endDate: string) {
            return database.prepare(`SELECT t.id,t.code,t.name,t.description,t.tracked,t.evidence_after_units,t.evidence_note,
      r.evidence_min_units,r.evidence_purpose,r.evidence_min_count,r.evidence_due_stage,r.evidence_grace_days,
      r.evidence_note rule_evidence_note,r.evidence_privacy,
      r.grant_kind,r.grant_units,r.accrual_units,r.accrual_period,r.carryover_limit,r.employee_eligible,r.contractor_eligible
    FROM leave_types t JOIN leave_policy_rules r ON r.leave_type_id=t.id AND r.policy_version_id=?
    WHERE t.id=? AND t.company_id=? AND t.active=true AND t.effective_from<=? AND (t.effective_to IS NULL OR t.effective_to>=?)`)
                .bind(policy_version_id, leaveTypeId, companyId, startDate, endDate).first<LeaveType>();
        },
        readPreviewLeaveHolidays(holiday_calendar_id: string, startDate: string, endDate: string) {
            return rows<{
                holiday_date: string;
            }>(database, "SELECT holiday_date FROM leave_holidays WHERE calendar_id=? AND holiday_date BETWEEN ? AND ?", holiday_calendar_id, startDate, endDate);
        },
        readPreviewLeavePublished(companyId: string, department_id: string | null, endDate: string, startDate: string) {
            return rows<{
                week_start: string;
            }>(database, `SELECT week_start FROM people_roster_weeks WHERE company_id=? AND state='published' AND department_id IS NOT DISTINCT FROM ? AND week_start::date<=?::date AND week_start::date+6>=?::date`, companyId, department_id, endDate, startDate);
        },
        readPreviewLeaveRosterDates(companyId: string, engagement_id: string, startDate: string, endDate: string) {
            return rows<{
                local_date: string;
            }>(database, `SELECT DISTINCT s.local_date FROM people_roster_assignments a JOIN people_roster_shifts s ON s.id=a.shift_id JOIN people_roster_weeks w ON w.id=s.week_id
    WHERE w.company_id=? AND w.state='published' AND a.engagement_id=? AND a.state='assigned' AND s.state='active' AND s.local_date BETWEEN ? AND ?`, companyId, engagement_id, startDate, endDate);
        },
        readPreviewLeaveCandidates(companyId: string, engagement_id: string, excludeId: string | null, startDate: string, endDate: string) {
            return rows<{
                id: string;
                number: string;
                state: string;
                leave_date: string;
                segment: string;
            }>(database, `SELECT r.id,r.number,r.state,s.leave_date,s.segment FROM leave_requests r JOIN leave_day_segments s ON s.request_id=r.id
    WHERE r.company_id=? AND r.engagement_id=? AND r.id IS DISTINCT FROM ? AND r.state IN ('pending_approval','approved') AND s.leave_date BETWEEN ? AND ?`, companyId, engagement_id, excludeId, startDate, endDate);
        },
        readPreviewLeaveLegacy(companyId: string, engagement_id: string, excludeId: string | null, endDate: string, startDate: string) {
            return rows<{
                id: string;
                number: string;
                state: string;
            }>(database, `SELECT r.id,r.number,r.state FROM leave_requests r WHERE r.company_id=? AND r.engagement_id=? AND r.id IS DISTINCT FROM ? AND r.state IN ('pending_approval','approved') AND r.start_date<=? AND coalesce(r.end_date,r.start_date)>=? AND NOT EXISTS(SELECT 1 FROM leave_day_segments s WHERE s.request_id=r.id)`, companyId, engagement_id, excludeId, endDate, startDate);
        },
        readPreviewLeaveApprovers(companyId: string, value: string) {
            return rows<{
                user_id: string;
                legal_name: string;
            }>(database, "SELECT user_id,legal_name FROM hr_people WHERE company_id=? AND user_id=ANY(?::text[])", companyId, value);
        },
        readLeaveSummaryTypes(policy_version_id: string) {
            return rows<LeaveType>(database, `SELECT t.id,t.code,t.name,t.description,t.tracked,t.evidence_after_units,t.evidence_note,
      r.evidence_min_units,r.evidence_purpose,r.evidence_min_count,r.evidence_due_stage,r.evidence_grace_days,
      r.evidence_note rule_evidence_note,r.evidence_privacy,
      r.grant_kind,r.grant_units,r.accrual_units,r.accrual_period,r.carryover_limit,r.employee_eligible,r.contractor_eligible
    FROM leave_policy_rules r JOIN leave_types t ON t.id=r.leave_type_id WHERE r.policy_version_id=? AND t.active=true ORDER BY t.name`, policy_version_id);
        },
        readLeaveSummaryRequests(companyId: string, engagement_id: string) {
            return rows<Record<string, unknown>>(database, `SELECT r.id,r.number,r.start_date,r.end_date,r.start_segment,r.end_segment,r.calculated_units,r.days,r.state,r.cancellation_state,r.version,r.created_at,r.updated_at,r.decision_note,t.name leave_type_name
    FROM leave_requests r LEFT JOIN leave_types t ON t.id=r.leave_type_id WHERE r.company_id=? AND r.engagement_id=? ORDER BY r.created_at DESC,r.id DESC LIMIT 100`, companyId, engagement_id);
        },
        readLeaveSummaryHolidays(holiday_calendar_id: string) {
            return rows<{
                holiday_date: string;
                name: string;
            }>(database, "SELECT h.holiday_date,h.name FROM leave_holidays h WHERE h.calendar_id=? AND h.holiday_date>=to_char(current_date,'YYYY-MM-DD') ORDER BY h.holiday_date LIMIT 8", holiday_calendar_id);
        },
        readListOwnLeavePagePage(args: unknown[], value: number, clauses: string[]) {
            return rows<LeaveListRow>(database, `SELECT r.id,r.number,r.start_date,r.end_date,r.start_segment,r.end_segment,r.calculated_units,r.days,r.state,r.cancellation_state,r.version,r.created_at,r.updated_at,r.decision_note,t.name leave_type_name
      FROM leave_requests r LEFT JOIN leave_types t ON t.id=r.leave_type_id
      WHERE ${clauses.join(" AND ")} ORDER BY r.created_at DESC,r.id DESC LIMIT ?`, ...args, value);
        },
        readLeaveCalendarMonthRequestsholidays(companyId: string, engagement_id: string, userId: string, end: string, start: string) {
            return rows<LeaveListRow>(database, `SELECT r.id,r.number,r.start_date,r.end_date,r.start_segment,r.end_segment,r.calculated_units,r.days,r.state,r.cancellation_state,r.version,r.created_at,r.updated_at,r.decision_note,t.name leave_type_name
        FROM leave_requests r LEFT JOIN leave_types t ON t.id=r.leave_type_id
        WHERE r.company_id=? AND r.engagement_id=? AND r.created_by=?
          AND r.state IN ('draft','pending_approval','approved')
          AND r.start_date::date<=?::date
          AND coalesce(r.end_date::date,r.start_date::date+greatest(r.days,1)::int-1)>=?::date
        ORDER BY r.start_date,r.id`, companyId, engagement_id, userId, end, start);
        },
        readLeaveCalendarMonthRequestsholidays2(companyId: string, engagement_id: string, end: string, start: string, end2: string, start2: string, start3: string, end3: string) {
            return rows<{
                holiday_date: string;
                name: string;
            }>(database, `SELECT DISTINCT h.holiday_date,h.name
        FROM leave_policy_assignments a
        JOIN leave_policy_versions p ON p.id=a.policy_version_id AND p.company_id=a.company_id
        JOIN leave_holidays h ON h.calendar_id=a.holiday_calendar_id
        WHERE a.company_id=? AND a.engagement_id=? AND a.holiday_calendar_id IS NOT NULL
          AND a.effective_from<=? AND (a.effective_to IS NULL OR a.effective_to>=?)
          AND p.effective_from<=? AND (p.effective_to IS NULL OR p.effective_to>=?)
          AND h.holiday_date BETWEEN ? AND ?
        ORDER BY h.holiday_date,h.name`, companyId, engagement_id, end, start, end2, start2, start3, end3);
        },
        readSaveSelfServiceLeaveEffect(engagement_id: string) { return database.prepare('SELECT id FROM hr_engagements WHERE id=? FOR UPDATE').bind(engagement_id).first(); },
        readSaveSelfServiceLeaveRow(id: string, companyId: string, userId: string) {
            return database.prepare('SELECT * FROM leave_requests WHERE id=? AND company_id=? AND created_by=? FOR UPDATE').bind(id, companyId, userId).first<{
                version: number;
                state: string;
                engagement_id: string;
            }>();
        },
        readSaveSelfServiceLeaveEmployee(...args) { return leaveEmployeeFacts(database).readSaveSelfServiceLeaveEmployee(...args); },
        stageSaveSelfServiceLeaveEffect(...args) { return leaveEmployeeFacts(database).stageSaveSelfServiceLeaveEffect(...args); },
        stageSaveSelfServiceLeaveEffect2(id: string, value: string, employeeId: string, person_id: string, engagement_id: string, companyId: string, value2: string, department_id: string | null, startDate: string, value3: number, handoverNote: string, userId: string, now: number, now2: number) { return database.prepare('INSERT INTO leave_requests(id,number,employee_id,person_id,engagement_id,company_id,branch_id,department_id,start_date,days,handover_note,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)').bind(id, value, employeeId, person_id, engagement_id, companyId, value2, department_id, startDate, value3, handoverNote, userId, now, now2); },
        stageSaveSelfServiceLeaveEffect3(startDate: string, value: number, handoverNote: string, leaveTypeId: string, endDate: string, startSegment: "full" | "first_half" | "second_half", endSegment: "full" | "first_half" | "second_half", id: string, units: number, value2: string, value3: string, evidenceDueAt: number | null, state: "draft" | "pending_approval", now: number, state2: "draft" | "pending_approval", version: number, userId: string, key: string, now2: number, id2: string) {
            return database.prepare(`UPDATE leave_requests SET start_date=?,days=?,handover_note=?,leave_type_id=?,end_date=?,start_segment=?,end_segment=?,policy_version_id=?,calculated_units=?,calculation_snapshot=?::jsonb,
    evidence_policy_snapshot=?::jsonb,evidence_due_at=?,submitted_at=CASE WHEN ?='pending_approval' THEN ? ELSE submitted_at END,
    state=?,version=?,prepared_by=?,decided_by=NULL,decision_note=NULL,last_command_id=?,updated_at=? WHERE id=?`).bind(startDate, value, handoverNote, leaveTypeId, endDate, startSegment, endSegment, id, units, value2, value3, evidenceDueAt, state, now, state2, version, userId, key, now2, id2);
        },
        stageSaveSelfServiceLeaveEffect4(id: string) { return database.prepare('DELETE FROM leave_day_segments WHERE request_id=?').bind(id); },
        stageSaveSelfServiceLeaveEffect5(value: string, id: string, companyId: string, date: string, segment: "full" | "first_half" | "second_half", units: number, source: "roster" | "workweek") { return database.prepare('INSERT INTO leave_day_segments(id,request_id,company_id,leave_date,segment,units,source) VALUES(?,?,?,?,?,?,?)').bind(value, id, companyId, date, segment, units, source); },
        stageSaveSelfServiceLeaveEffect6(value: string, companyId: string, engagement_id: string, leaveTypeId: string, id: string, value2: number, value3: string, year: number, value4: string, value5: "Pending leave reservation", userId: string, now: number) { return database.prepare("INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,request_id,kind,units,effective_date,entitlement_year,command_key,note,created_by,created_at) VALUES(?,?,?,?,?,'reservation',?,?,?,?,?,?,?)").bind(value, companyId, engagement_id, leaveTypeId, id, value2, value3, year, value4, value5, userId, now); },
        readNormalizeLegacyLeaveExisting(recordId: string, companyId: string, userId: string) {
            return database.prepare('SELECT leave_type_id,end_date,start_segment,end_segment FROM leave_requests WHERE id=? AND company_id=? AND created_by=?').bind(recordId, companyId, userId).first<{
                leave_type_id: string;
                end_date: string;
                start_segment: 'full' | 'first_half' | 'second_half';
                end_segment: 'full' | 'first_half' | 'second_half';
            }>();
        },
        readAdministerLeaveEffect(leaveTypeId: string, companyId: string) { return database.prepare('SELECT 1 FROM leave_types WHERE id=? AND company_id=?').bind(leaveTypeId, companyId).first(); },
        readAdministerLeaveEffect2(engagementId: string, companyId: string) { return database.prepare('SELECT 1 FROM hr_engagements WHERE id=? AND company_id=?').bind(engagementId, companyId).first(); },
        readAdministerLeaveEffect3(leaveTypeId: string, companyId: string) { return database.prepare('SELECT 1 FROM leave_types WHERE id=? AND company_id=?').bind(leaveTypeId, companyId).first(); },
        readAdministerLeaveEffect4(calendarId: string, companyId: string) { return database.prepare('SELECT 1 FROM leave_holiday_calendars WHERE id=? AND company_id=? AND active=true').bind(calendarId, companyId).first(); },
        readAdministerLeaveEffect5(engagementId: string, companyId: string) { return database.prepare('SELECT 1 FROM hr_engagements WHERE id=? AND company_id=?').bind(engagementId, companyId).first(); },
        stageAdministerLeaveEffect(id: string, companyId: string, code: string, name: string, description: string, tracked: boolean, evidenceAfterUnits: number | null, evidenceNote: string, value: string, userId: string, now: number, now2: number) {
            return database.prepare("INSERT INTO leave_types(id,company_id,code,name,description,tracked,evidence_after_units,evidence_note,effective_from,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id, companyId, code, name, description, tracked, evidenceAfterUnits, evidenceNote, value, userId, now, now2).run();
        },
        stageAdministerLeaveEffect2(id: string, companyId: string, name: string, timezone: string, userId: string, now: number) { return database.prepare("INSERT INTO leave_holiday_calendars(id,company_id,name,timezone,created_by,created_at) VALUES(?,?,?,?,?,?)").bind(id, companyId, name, timezone, userId, now).run(); },
        stageAdministerLeaveEffect3(id: string, companyId: string, policyKey: string, version: number, name: string, effectiveFrom: string, effectiveTo: string | null, timezone: string, value: string, allowNegative: boolean, userId: string, now: number) { return database.prepare("INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,effective_from,effective_to,timezone,workweek_json,allow_negative,created_by,created_at) VALUES(?,?,?,?,?,'active',?,?,?,?::jsonb,?,?,?)").bind(id, companyId, policyKey, version, name, effectiveFrom, effectiveTo, timezone, value, allowNegative, userId, now); },
        stageAdministerLeaveEffect4(id: string, leaveTypeId: string, grantKind: "annual" | "period" | "accrual" | "untracked", grantUnits: number, accrualUnits: number, accrualPeriod: "annual" | "monthly" | "quarterly" | null, carryoverLimit: number | null, employeeEligible: boolean, contractorEligible: boolean, evidenceMinUnits: number | null, evidencePurpose: "medical_certificate" | "leave_supporting_photo" | null, evidenceMinCount: number, evidenceDueStage: "before_submit" | "before_final_approval", evidenceGraceDays: number, evidenceNote: string, leaveTypeId2: string, companyId: string) {
            return database.prepare(`INSERT INTO leave_policy_rules(
      policy_version_id,leave_type_id,grant_kind,grant_units,accrual_units,accrual_period,carryover_limit,employee_eligible,contractor_eligible,
      evidence_min_units,evidence_purpose,evidence_min_count,evidence_due_stage,evidence_grace_days,evidence_note,evidence_privacy
    ) SELECT ?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'requester_approvers_hr' WHERE EXISTS(SELECT 1 FROM leave_types WHERE id=? AND company_id=?)`).bind(id, leaveTypeId, grantKind, grantUnits, accrualUnits, accrualPeriod, carryoverLimit, employeeEligible, contractorEligible, evidenceMinUnits, evidencePurpose, evidenceMinCount, evidenceDueStage, evidenceGraceDays, evidenceNote, leaveTypeId2, companyId);
        },
        stageAdministerLeaveEffect5(id: string, calendarId: string, date: string, name: string, calendarId2: string, companyId: string) { return database.prepare("INSERT INTO leave_holidays(id,calendar_id,holiday_date,name) SELECT ?,?,?,? WHERE EXISTS(SELECT 1 FROM leave_holiday_calendars WHERE id=? AND company_id=?)").bind(id, calendarId, date, name, calendarId2, companyId).run(); },
        readAdministerLeaveOverlaps(companyId: string, engagementId: string, value: string, effectiveFrom: string) {
            return rows<{
                id: string;
                policy_key: string;
                effective_from: string;
            }>(database, `SELECT a.id,p.policy_key,a.effective_from FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id WHERE a.company_id=? AND a.engagement_id=? AND a.effective_from<=? AND coalesce(a.effective_to,'9999-12-31')>=?`, companyId, engagementId, value, effectiveFrom);
        },
        stageAdministerLeaveEffect6(value: string, id: string) { return database.prepare('UPDATE leave_policy_assignments SET effective_to=? WHERE id=?').bind(value, id).run(); },
        readAdministerLeaveEffect6(policyVersionId: string, companyId: string) { return database.prepare('SELECT id FROM leave_policy_versions WHERE id=? AND company_id=?').bind(policyVersionId, companyId).first(); },
        stageAdministerLeaveEffect7(id: string, companyId: string, engagementId: string, policyVersionId: string, calendarId: string | null, effectiveFrom: string, effectiveTo: string | null, userId: string, now: number, engagementId2: string, companyId2: string) { return database.prepare("INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,holiday_calendar_id,effective_from,effective_to,created_by,created_at) SELECT ?,?,?,?,?,?,?,?,? WHERE EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=?)").bind(id, companyId, engagementId, policyVersionId, calendarId, effectiveFrom, effectiveTo, userId, now, engagementId2, companyId2).run(); },
        readAdministerLeaveRow(id: string, companyId: string) {
            return database.prepare('SELECT effective_from,version FROM leave_policy_assignments WHERE id=? AND company_id=? FOR UPDATE').bind(id, companyId).first<{
                effective_from: string;
                version: number;
            }>();
        },
        stageAdministerLeaveEffect8(effectiveTo: string, id: string, companyId: string) { return database.prepare('UPDATE leave_policy_assignments SET effective_to=?,version=version+1 WHERE id=? AND company_id=?').bind(effectiveTo, id, companyId).run(); },
        stageAdministerLeaveEffect9(id: string, companyId: string, engagementId: string, leaveTypeId: string, units: number, effectiveDate: string, key: string, note: string, userId: string, now: number) { return database.prepare("INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,kind,units,effective_date,command_key,note,created_by,created_at) VALUES(?,?,?,?,'adjustment',?,?,?,?,?,?) ON CONFLICT(company_id,command_key,kind,leave_type_id) DO NOTHING").bind(id, companyId, engagementId, leaveTypeId, units, effectiveDate, key, note, userId, now).run(); },
        readRunLeaveAccrualsDue(effectiveDate: string, effectiveDate2: string, effectiveDate3: string, effectiveDate4: string, effectiveDate5: string, value: string | null, value2: string | null) {
            return rows<{
                company_id: string;
                engagement_id: string;
                leave_type_id: string;
                assignment_id: string;
                policy_version_id: string;
                grant_kind: string;
                grant_units: number;
                accrual_units: number;
                accrual_period: string | null;
                carryover_limit: number | null;
                first_day: string;
                last_day: string;
            }>(database, `SELECT a.company_id,a.engagement_id,r.leave_type_id,a.id assignment_id,p.id policy_version_id,r.grant_kind,r.grant_units,r.accrual_units,r.accrual_period,r.carryover_limit,
 greatest(a.effective_from,p.effective_from,e.start_date) first_day,least(coalesce(a.effective_to,?),coalesce(p.effective_to,?),coalesce(e.end_date,?),?) last_day
 FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id AND p.company_id=a.company_id JOIN leave_policy_rules r ON r.policy_version_id=p.id
 JOIN hr_engagements e ON e.id=a.engagement_id AND e.company_id=a.company_id JOIN leave_types t ON t.id=r.leave_type_id AND t.company_id=a.company_id
 WHERE p.status IN ('active','retired') AND t.tracked=true AND a.effective_from<=? AND (?::text IS NULL OR a.company_id=?) AND (CASE WHEN e.worker_type='contractor' THEN r.contractor_eligible ELSE r.employee_eligible END)
 AND ${scheduledFeaturePredicate("a.company_id", "people_payroll")} ORDER BY first_day,a.id,r.leave_type_id`, effectiveDate, effectiveDate2, effectiveDate3, effectiveDate4, effectiveDate5, value, value2);
        },
        stageRunLeaveAccrualsResult(value: string, company_id: string, engagement_id: string, leave_type_id: string, kind: string, units: number, date: string, value2: string, value3: string, now: number, value4: string[]) {
            return database.prepare(`INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,kind,units,effective_date,command_key,note,created_by,created_at)
    SELECT ?,?,?,?,?,?,?,?,?,'system:leave-accrual',? WHERE ${scheduledFeaturePredicate("?", "people_payroll")}
    ON CONFLICT(company_id,command_key,kind,leave_type_id) DO NOTHING`)
                .bind(value, company_id, engagement_id, leave_type_id, kind, units, date, value2, value3, now, ...value4).run();
        },
        readLeaveAdministrationTypespoliciescalendarsengagementsassignmentseligibilities(companyId: string) {
            return rows<{
                id: string;
                name: string;
                tracked: boolean;
            }>(database, 'SELECT id,name,tracked FROM leave_types WHERE company_id=? ORDER BY name', companyId);
        },
        readLeaveAdministrationTypespoliciescalendarsengagementsassignmentseligibilities2(companyId: string) {
            return rows<{
                id: string;
                name: string;
                version: number;
                effective_from: string;
                effective_to: string | null;
            }>(database, 'SELECT id,name,version,effective_from,effective_to FROM leave_policy_versions WHERE company_id=? ORDER BY name,version DESC', companyId);
        },
        readLeaveAdministrationTypespoliciescalendarsengagementsassignmentseligibilities3(companyId: string) {
            return rows<{
                id: string;
                name: string;
            }>(database, 'SELECT id,name FROM leave_holiday_calendars WHERE company_id=? ORDER BY name', companyId);
        },
        readLeaveAdministrationTypespoliciescalendarsengagementsassignmentseligibilities4(companyId: string) {
            return rows<{
                id: string;
                name: string;
                worker_type: string;
            }>(database, "SELECT e.id,p.legal_name name,e.worker_type FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE e.company_id=? AND e.status='active' ORDER BY p.legal_name", companyId);
        },
        readLeaveAdministrationTypespoliciescalendarsengagementsassignmentseligibilities5(companyId: string) {
            return rows<{
                id: string;
                name: string;
                policy: string;
                effective_from: string;
                effective_to: string | null;
                version: number;
            }>(database, 'SELECT a.id,p.legal_name name,v.name policy,a.effective_from,a.effective_to,a.version FROM leave_policy_assignments a JOIN hr_engagements e ON e.id=a.engagement_id JOIN hr_people p ON p.id=e.person_id JOIN leave_policy_versions v ON v.id=a.policy_version_id WHERE a.company_id=? ORDER BY p.legal_name,a.effective_from DESC', companyId);
        },
        readLeaveAdministrationTypespoliciescalendarsengagementsassignmentseligibilities6(companyId: string) {
            return rows<{
                engagement_id: string;
                eligible: boolean;
                version: number;
                top_level: boolean;
            }>(database, 'SELECT o.engagement_id,o.eligible,o.version,e.self_service_top_level top_level FROM overtime_eligibility o JOIN hr_engagements e ON e.id=o.engagement_id AND e.company_id=o.company_id WHERE o.company_id=?', companyId);
        } };
    return { repository: r, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, prepare: async (holding) => {
            if (!provided)
                database = (await holdingResources(holding)).write;
        }, commands: { replay: (...args) => replayCommand(database, ...args), commit: (...args) => executeCommand(database, ...args) }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), decode: value => Buffer.from(value, 'base64url').toString('utf8'), encode: value => Buffer.from(value).toString('base64url'), listOwn: async (context, engagementId, input, cursor) => {
            const staff = { engagement_id: engagementId };
            const clauses = [
                "r.company_id=?",
                "r.engagement_id=?",
                "r.created_by=?",
            ], args: unknown[] = [context.companyId, staff.engagement_id, context.userId];
            const filterSql: Record<z.infer<typeof leaveListFilterSchema>, string> = {
                all: "TRUE",
                drafts: "r.state='draft'",
                review: "(r.state='pending_approval' OR r.cancellation_state='pending_review')",
                approved: "(r.state='approved' AND r.cancellation_state IS DISTINCT FROM 'pending_review')",
                closed: "r.state NOT IN ('draft','pending_approval','approved')",
            };
            clauses.push(filterSql[input.filter]);
            if (cursor) {
                clauses.push("(r.created_at<? OR (r.created_at=? AND r.id<?))");
                args.push(cursor.at, cursor.at, cursor.id);
            }
            const page = await rows<LeaveListRow>(database, `SELECT r.id,r.number,r.start_date,r.end_date,r.start_segment,r.end_segment,r.calculated_units,r.days,r.state,r.cancellation_state,r.version,r.created_at,r.updated_at,r.decision_note,t.name leave_type_name
      FROM leave_requests r LEFT JOIN leave_types t ON t.id=r.leave_type_id
      WHERE ${clauses.join(" AND ")} ORDER BY r.created_at DESC,r.id DESC LIMIT ?`, ...args, input.limit + 1);
            return page;
        }, activeStaff: (...args) => fromNative(() => activeStaff(database, ...args)), requireCompanyAuthority: (...args) => fromNative(() => requireCompanyAuthority(database, ...args)), openApproval: (...args) => fromNative(() => openApproval(database, ...args)), peopleRequestRoute: (...args) => fromNative(() => peopleRequestRoute(database, ...args)), availableLeaveEvidence: (...args) => fromNative(() => availableLeaveEvidence(database, ...args)), transaction: run => {
            if(providedOwned)return run(serviceContext(database,true));
            if (!isTransactionalSqlDatabase(database))
                throw new Error('LEAVE_TRANSACTION_REQUIRED');
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
export async function previewLeave(context: WorkContext, raw: z.infer<typeof leavePreviewSchema>, excludeId: string | null = null, database?: SqlDatabase) { return withWorkforcePurchasedRead(context,tx=>invoke(() => createLeaveSelfServiceApplication(serviceContext(tx)).previewLeave(context,raw,excludeId)),database); }
export async function leaveSummary(context: WorkContext) { return withWorkforcePurchasedRead(context,async tx=>{const data=await invoke(() => createLeaveSelfServiceApplication(serviceContext(tx)).leaveSummary(context)),purchase=await workforcePurchaseControls(context,tx);return {...data,...(purchase?{purchase}:{})};}); }
export async function listOwnLeavePage(context: WorkContext, raw: {
    filter?: string;
    limit?: number | string;
    cursor?: string;
}) { return withWorkforcePurchasedRead(context,tx=>invoke(() => createLeaveSelfServiceApplication(serviceContext(tx)).listOwnLeavePage(context, raw))); }
export async function leaveCalendarMonth(context: WorkContext, rawMonth: string) { return withWorkforcePurchasedRead(context,tx=>invoke(() => createLeaveSelfServiceApplication(serviceContext(tx)).leaveCalendarMonth(context, rawMonth))); }
export async function createSelfServiceLeave(context: WorkContext, raw: z.infer<typeof leaveCreateSchema>, key: string) { return withWorkforcePurchasedCommand(context,{key,operation:"leave.self.create",purpose:"new-work",resultKind:"leave"},tx=>invoke(() => createLeaveSelfServiceApplication(serviceContext(tx,Boolean(tx))).createSelfServiceLeave(context,raw,key))); }
export async function editSelfServiceLeave(context: WorkContext, id: string, raw: z.infer<typeof leaveEditSchema>, key: string) { return withWorkforcePurchasedCommand(context,{key,operation:raw.saveOnly?"leave.self.save":"leave.self.submit",purpose:"new-work",target:{kind:"leave",id}},tx=>invoke(() => createLeaveSelfServiceApplication(serviceContext(tx,Boolean(tx))).editSelfServiceLeave(context,id,raw,key))); }
export async function normalizeLegacyLeave(context: WorkContext, input: {
    startDate: string;
    days: number;
    handoverNote: string;
}, recordId?: string) { return withWorkforcePurchasedRead(context,tx=>invoke(() => createLeaveSelfServiceApplication(serviceContext(tx)).normalizeLegacyLeave(context, input, recordId))); }
export async function listOwnLeave(context: WorkContext, state?: string) { return withWorkforcePurchasedRead(context,tx=>invoke(() => createLeaveSelfServiceApplication(serviceContext(tx)).listOwnLeave(context, state))); }
export async function administerLeave(context: WorkContext, raw: z.infer<typeof leaveAdminSchema>, key: string) { return withWorkforcePurchasedCommand(context,{key,operation:"leave.policy.manage",purpose:raw.resource==="assignment_end"?"settle-existing":"new-work",permission:"leave.policy.manage",target:{kind:"configuration",id:context.companyId}},tx=>invoke(() => createLeaveSelfServiceApplication(serviceContext(tx,Boolean(tx))).administerLeave(context,raw,key))); }
export async function runLeaveAccruals(db: SqlDatabase, effectiveDate: string, companyId?: string) { if(await peoplePackageEnforcementMode()==="disabled")return invoke(()=>createLeaveSelfServiceApplication(serviceContext(db)).runLeaveAccruals(effectiveDate,companyId));return withWorkforceAccrualTransaction(db,async(tx,permits)=>{const companies=companyId?[{company_id:companyId}]:(await tx.prepare("SELECT DISTINCT company_id FROM leave_policy_assignments ORDER BY company_id").all<{company_id:string}>()).results;let inserted=0;for(const company of companies)if(await permits(company.company_id))inserted+=await invoke(()=>createLeaveSelfServiceApplication(serviceContext(tx,true)).runLeaveAccruals(effectiveDate,company.company_id));return inserted;}); }
export async function leaveAdministration(context: WorkContext) { return withWorkforcePurchasedRead(context,tx=>invoke(() => createLeaveSelfServiceApplication(serviceContext(tx)).leaveAdministration(context))); }
