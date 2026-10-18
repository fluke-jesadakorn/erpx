import { withPayrollPurchasedCommand, withPayrollPurchasedRead, withPayrollJobsPurchasedTransaction } from "./payroll-purchases";
import { peoplePackageEnforcementMode, type PackageEnforcementMode } from "../../commercial/public/purchase-command-boundary";
import { packageClockSql, purchasedCapabilityPredicate } from "../../commercial/public/purchased-capability-sql";
import type { CapabilityPurpose } from "../../commercial/public/rules/packages";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError } from "@/server/modules/runtime/public/http";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { isTransactionalSqlDatabase, type SqlDatabase, type SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { thaiEvidenceForEmployee, thaiEvidenceGuards } from "@/server/modules/people_payroll/adapters/payroll-statutory";
import { payrollAdjustmentInputs, payrollAdjustmentSourceFresh, payrollAdjustmentGuards } from "@/server/modules/people_payroll/adapters/payroll-adjustments";
import { BusinessRuleError } from '../../shared';
import { calculatePayrollItemForSystem } from '../public/people-payroll';
import { payrollBenefitSources } from '../../benefits/public/payroll-source-facts';
import { systemPayrollPreviewEvents } from '../../workflow/public/system-payroll-preview-events';
import { payrollScheduleSchema, payrollSubmitReviewSchema, type Schedule, type Run } from '../domain/payroll-readiness';
import { createPayrollReadinessApplication, type PayrollReadinessRepository, type PayrollReadinessPorts } from '../application/payroll-readiness';
export { payrollScheduleSchema, payrollSubmitReviewSchema, type PayrollBlocker } from '../domain/payroll-readiness';
type Permits = (companyId: string, purpose: CapabilityPurpose) => Promise<boolean>;
function context(provided?: SqlDatabase, mode: PackageEnforcementMode = "disabled", permits?: Permits, sourceChecks: (() => Promise<void>)[] = []): PayrollReadinessPorts<SqlStatement> {
    let database = provided!;
    const purchase = (company: string, purpose: CapabilityPurpose) => mode === "enforced" ? `(SELECT ${purchasedCapabilityPredicate(company,"people.payroll",purpose,"payroll_purchase_clock.now_ms")} FROM (SELECT ${packageClockSql} now_ms) payroll_purchase_clock)` : "TRUE";
    const repository: PayrollReadinessRepository<SqlStatement> = { readCostEvidenceBlockersEngagements(company_id: string, pay_group_id: string, currency: string, period_end: string, period_start: string, scope_engagement_id: string | null, scope_engagement_id2: string | null) {
            return database.prepare(`SELECT e.id,e.start_date,e.end_date,p.id person_id,p.legal_name FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id
    WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.worker_type='employee' AND e.status IN ('active','ended')
      AND p.status='active' AND p.merged_into_id IS NULL AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?)
      AND (?::text IS NULL OR e.id=?) ORDER BY e.id`)
                .bind(company_id, pay_group_id, currency, period_end, period_start, scope_engagement_id, scope_engagement_id2)
                .all<{
                id: string;
                start_date: string;
                end_date: string | null;
                person_id: string;
                legal_name: string;
            }>();
        },
        readCostEvidenceBlockersAssignments(id: string, company_id: string, period_end: string, period_start: string) {
            return database.prepare(`SELECT id,effective_from,effective_to,branch_id,department_id,evidence_status FROM hr_payroll_cost_assignments
      WHERE engagement_id=? AND company_id=? AND status='active' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from,id`)
                .bind(id, company_id, period_end, period_start)
                .all<{
                id: string;
                effective_from: string;
                effective_to: string | null;
                branch_id: string | null;
                department_id: string | null;
                evidence_status: string;
            }>();
        },
        readCostEvidenceBlockersTimeEntries(id: string, period_start: string, period_end: string) {
            return database.prepare(`SELECT work_date,cost_assignment_id,cost_branch_id,cost_department_id FROM hr_time_entries
      WHERE engagement_id=? AND work_date BETWEEN ? AND ? AND status='approved' ORDER BY id`)
                .bind(id, period_start, period_end)
                .all<{
                work_date: string;
                cost_assignment_id: string | null;
                cost_branch_id: string | null;
                cost_department_id: string | null;
            }>();
        },
        readSourceBlockersRows(id: string) {
            return database.prepare("SELECT engagement_id,input_snapshot_json FROM hr_payroll_items WHERE run_id=? ORDER BY engagement_id").bind(id).all<{
                engagement_id: string;
                input_snapshot_json: string;
            }>();
        },
        readSourceBlockersCurrent(company_id: string, pay_group_id: string, currency: string, period_end: string, period_start: string, scope_engagement_id: string | null, scope_engagement_id2: string | null) {
            return database.prepare("SELECT e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.worker_type='employee' AND e.status IN ('active','ended') AND p.status='active' AND p.merged_into_id IS NULL AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) AND (?::text IS NULL OR e.id=?) ORDER BY e.id").bind(company_id, pay_group_id, currency, period_end, period_start, scope_engagement_id, scope_engagement_id2).all<{
                id: string;
            }>();
        },
        readSourceBlockersGroup(pay_group_id: string, company_id: string) {
            return database.prepare("SELECT overtime_multiplier_bps,proration_method FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active'").bind(pay_group_id, company_id).first<{
                overtime_multiplier_bps: number;
                proration_method: string;
            }>();
        },
        readSourceBlockersEngagement(engagementId: string, company_id: string) { return database.prepare("SELECT e.id,e.version,e.person_id,e.worker_type,e.employment_type,e.start_date,e.end_date,e.job_title,e.branch_id,e.department_id FROM hr_engagements e WHERE e.id=? AND e.company_id=?").bind(engagementId, company_id).first(); },
        readSourceBlockersPayee(value: string, company_id: string) { return database.prepare("SELECT id,version,legal_name,preferred_name FROM hr_people WHERE id=? AND company_id=? AND status='active' AND merged_into_id IS NULL").bind(value, company_id).first(); },
        readSourceBlockersCompensation(engagementId: string, period_end: string, period_start: string) { return database.prepare("SELECT * FROM hr_compensation_revisions WHERE engagement_id=? AND status IN ('active','approved') AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from DESC LIMIT 1").bind(engagementId, period_end, period_start).first(); },
        readSourceBlockersEntries(engagementId: string, period_start: string, period_end: string) { return database.prepare("SELECT id,version,work_date,regular_hours,overtime_hours,cost_assignment_id,cost_branch_id,cost_department_id FROM hr_time_entries WHERE engagement_id=? AND work_date BETWEEN ? AND ? AND status='approved' ORDER BY id").bind(engagementId, period_start, period_end).all(); },
        readSourceBlockersCostAssignments(engagementId: string, company_id: string, activeEnd: string, activeStart: string) { return database.prepare("SELECT id,effective_from,effective_to,branch_id,department_id,evidence_status,version FROM hr_payroll_cost_assignments WHERE engagement_id=? AND company_id=? AND status='active' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from,id").bind(engagementId, company_id, activeEnd, activeStart).all(); },
        readSourceBlockersHandoffs(engagementId: string, period_start: string, period_end: string) { return database.prepare("SELECT id,request_id,request_version,minutes,work_date FROM overtime_payroll_handoffs WHERE engagement_id=? AND work_date BETWEEN ? AND ? ORDER BY id").bind(engagementId, period_start, period_end).all(); },
        readSourceBlockersBenefits(...args) { return payrollBenefitSources(database).readSourceBlockersBenefits(...args); },
        readSourceBlockersAdvances(company_id: string, engagementId: string, period_end: string, period_start: string, id: string) { return database.prepare("SELECT r.id,r.version,r.amount_minor,r.due_period_start,r.due_period_end,a.reason FROM hr_payroll_advance_repayments r JOIN hr_payroll_advances a ON a.id=r.advance_id WHERE r.company_id=? AND a.engagement_id=? AND a.status='approved' AND r.status IN ('open','reserved') AND r.due_period_start<=? AND r.due_period_end>=? AND (r.payroll_run_id IS NULL OR r.payroll_run_id=?) ORDER BY r.sequence").bind(company_id, engagementId, period_end, period_start, id).all(); },
        readAssertPayrollSourceFreshRun(runId: string) { return database.prepare("SELECT id,company_id,pay_group_id,country_pack_id,scope_engagement_id,period_start,period_end,pay_date,currency,mode,status,version,revision,input_digest,submitted_by,submitted_digest,adjustment_of_run_id FROM hr_payroll_runs WHERE id=?").bind(runId).first<Run>(); },
        readPayrollSourceGuardsRun(runId: string) {
            return database.prepare("SELECT id,company_id,pay_group_id,country_pack_id,pay_date,mode,period_start,period_end,currency,scope_engagement_id,adjustment_of_run_id FROM hr_payroll_runs WHERE id=?").bind(runId).first<{
                id: string;
                company_id: string;
                pay_group_id: string;
                country_pack_id: string | null;
                pay_date: string;
                mode: string;
                period_start: string;
                period_end: string;
                currency: string;
                scope_engagement_id: string | null;
                adjustment_of_run_id: string | null;
            }>();
        },
        readPayrollSourceGuardsRows(runId: string) {
            return database.prepare("SELECT engagement_id,input_snapshot_json FROM hr_payroll_items WHERE run_id=? ORDER BY engagement_id").bind(runId).all<{
                engagement_id: string;
                input_snapshot_json: string;
            }>();
        },
        guard18(key: string, value: "source-employee-count", company_id: string, pay_group_id: string, currency: string, period_end: string, period_start: string, scope_engagement_id: string | null, scope_engagement_id2: string | null, length: number) { return assertion(database, key, value, `(SELECT COUNT(*) FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.worker_type='employee' AND e.status IN ('active','ended') AND p.status='active' AND p.merged_into_id IS NULL AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) AND (?::text IS NULL OR e.id=?))=?`, [company_id, pay_group_id, currency, period_end, period_start, scope_engagement_id, scope_engagement_id2, length]); },
        guard19(key: string, value: string, pay_group_id: string, company_id: string, overtimeMultiplierBps: number, prorationMethod: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active' AND overtime_multiplier_bps=? AND proration_method=?)", [pay_group_id, company_id, overtimeMultiplierBps, prorationMethod]); },
        guard20(key: string, value: string, engagement_id: string, company_id: string, pay_group_id: string, currency: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND pay_group_id=? AND currency=? AND worker_type='employee' AND version=? AND status IN ('active','ended'))", [engagement_id, company_id, pay_group_id, currency, version]); },
        guard21(key: string, value: string, engagement_id: string, id: string, company_id: string, version: number, legal_name: string, preferred_name: string | null) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id WHERE e.id=? AND p.id=? AND p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND p.version=? AND p.legal_name=? AND p.preferred_name IS NOT DISTINCT FROM ?)", [engagement_id, id, company_id, version, legal_name, preferred_name]); },
        guard22(key: string, value: string, id: string, engagement_id: string, version: number, period_end: string, period_start: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE id=? AND engagement_id=? AND version=? AND status IN ('active','approved') AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?))", [id, engagement_id, version, period_end, period_start]); },
        guard23(key: string, value: string, engagement_id: string, period_start: string, period_end: string, length: number) { return assertion(database, key, value, "(SELECT COUNT(*) FROM hr_time_entries WHERE engagement_id=? AND work_date BETWEEN ? AND ? AND status='approved')=?", [engagement_id, period_start, period_end, length]); },
        guard24(key: string, value: string, id: string, engagement_id: string, version: number, work_date: string, regular_hours: number, overtime_hours: number, cost_assignment_id: string | null, cost_branch_id: string | null, cost_department_id: string | null) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_time_entries WHERE id=? AND engagement_id=? AND version=? AND status='approved' AND work_date=? AND regular_hours=? AND overtime_hours=? AND cost_assignment_id IS NOT DISTINCT FROM ? AND cost_branch_id IS NOT DISTINCT FROM ? AND cost_department_id IS NOT DISTINCT FROM ?)", [id, engagement_id, version, work_date, regular_hours, overtime_hours, cost_assignment_id, cost_branch_id, cost_department_id]); },
        guard25(key: string, value: string, engagement_id: string, company_id: string, period_end: string, period_start: string, length: number) { return assertion(database, key, value, "(SELECT COUNT(*) FROM hr_payroll_cost_assignments WHERE engagement_id=? AND company_id=? AND status='active' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?))=?", [engagement_id, company_id, period_end, period_start, length]); },
        guard26(key: string, value: string, id: string, engagement_id: string, company_id: string, version: number, effective_from: string, effective_to: string | null, branch_id: string | null, department_id: string | null) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_cost_assignments WHERE id=? AND engagement_id=? AND company_id=? AND status='active' AND evidence_status='verified' AND version=? AND effective_from=? AND effective_to IS NOT DISTINCT FROM ? AND branch_id IS NOT DISTINCT FROM ? AND department_id IS NOT DISTINCT FROM ?)", [id, engagement_id, company_id, version, effective_from, effective_to, branch_id, department_id]); },
        guard27(key: string, value: string, engagement_id: string, period_start: string, period_end: string, length: number) { return assertion(database, key, value, "(SELECT COUNT(*) FROM overtime_payroll_handoffs WHERE engagement_id=? AND work_date BETWEEN ? AND ?)=?", [engagement_id, period_start, period_end, length]); },
        guard28(key: string, value: string, id: string, engagement_id: string, request_version: number, minutes: number, work_date: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM overtime_payroll_handoffs WHERE id=? AND engagement_id=? AND request_version=? AND minutes=? AND work_date=?)", [id, engagement_id, request_version, minutes, work_date]); },
        guard29(...args) { return payrollBenefitSources(database).guard29(...args); },
        guard30(...args) { return payrollBenefitSources(database).guard30(...args); },
        guard31(key: string, value: string, company_id: string, engagement_id: string, period_end: string, period_start: string, runId: string, length: number) { return assertion(database, key, value, "(SELECT COUNT(*) FROM hr_payroll_advance_repayments r JOIN hr_payroll_advances a ON a.id=r.advance_id WHERE r.company_id=? AND a.engagement_id=? AND a.status='approved' AND r.status IN ('open','reserved') AND r.due_period_start<=? AND r.due_period_end>=? AND (r.payroll_run_id IS NULL OR r.payroll_run_id=?))=?", [company_id, engagement_id, period_end, period_start, runId, length]); },
        guard32(key: string, value: string, id: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_advance_repayments WHERE id=? AND version=? AND status IN ('open','reserved'))", [id, version]); },
        readPayrollReadinessRun(runId: string, companyId: string) { return database.prepare("SELECT id,company_id,pay_group_id,country_pack_id,scope_engagement_id,period_start,period_end,pay_date,currency,mode,status,version,revision,input_digest,submitted_by,submitted_digest,adjustment_of_run_id FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId, companyId).first<Run>(); },
        readPayrollReadinessToday(pay_group_id: string, companyId: string) {
            return database.prepare("SELECT timezone FROM hr_pay_groups WHERE id=? AND company_id=?").bind(pay_group_id, companyId).first<{
                timezone: string;
            }>();
        },
        readPayrollReadinessEmployees(companyId: string, pay_group_id: string, currency: string, period_end: string, period_start: string, scope_engagement_id: string | null, scope_engagement_id2: string | null) {
            return database.prepare("SELECT COUNT(*) AS count FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.worker_type='employee' AND e.status IN ('active','ended') AND p.status='active' AND p.merged_into_id IS NULL AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) AND (?::text IS NULL OR e.id=?)").bind(companyId, pay_group_id, currency, period_end, period_start, scope_engagement_id, scope_engagement_id2).first<{
                count: number;
            }>();
        },
        readPayrollReadinessContractors(companyId: string, pay_group_id: string, period_end: string, period_start: string) {
            return database.prepare("SELECT COUNT(*) AS count FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.worker_type='contractor' AND e.status IN ('active','ended') AND p.status='active' AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?)").bind(companyId, pay_group_id, period_end, period_start).first<{
                count: number;
            }>();
        },
        readPayrollReadinessMissing(companyId: string, pay_group_id: string, currency: string, period_end: string, period_start: string, scope_engagement_id: string | null, scope_engagement_id2: string | null, currency2: string, period_start2: string, period_end2: string) {
            return database.prepare("SELECT e.id,p.legal_name,p.id person_id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.worker_type='employee' AND e.status IN ('active','ended') AND p.status='active' AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) AND (?::text IS NULL OR e.id=?) AND NOT EXISTS(SELECT 1 FROM hr_compensation_revisions c WHERE c.engagement_id=e.id AND c.status IN ('active','approved') AND c.currency=? AND c.effective_from<=? AND (c.effective_to IS NULL OR c.effective_to>=?)) ORDER BY e.id LIMIT 20").bind(companyId, pay_group_id, currency, period_end, period_start, scope_engagement_id, scope_engagement_id2, currency2, period_start2, period_end2).all<{
                id: string;
                legal_name: string;
                person_id: string;
            }>();
        },
        readPayrollReadinessMissingOvertimeRates(currency: string, period_start: string, period_end: string, companyId: string, pay_group_id: string, period_end2: string, period_start2: string, scope_engagement_id: string | null, scope_engagement_id2: string | null, period_start3: string, period_end3: string, period_start4: string, period_end4: string) {
            return database.prepare(`SELECT p.id person_id,p.legal_name FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id
    JOIN LATERAL (SELECT c.hourly_rate_minor FROM hr_compensation_revisions c WHERE c.engagement_id=e.id
      AND c.status IN ('active','approved') AND c.currency=? AND c.effective_from<=?
      AND (c.effective_to IS NULL OR c.effective_to>=?) ORDER BY c.effective_from DESC LIMIT 1) c ON TRUE
    WHERE e.company_id=? AND e.pay_group_id=? AND e.employment_type='salaried' AND e.worker_type='employee'
      AND e.status IN ('active','ended') AND p.status='active' AND e.start_date<=?
      AND (e.end_date IS NULL OR e.end_date>=?) AND (?::text IS NULL OR e.id=?)
      AND COALESCE(c.hourly_rate_minor,0)<=0 AND (EXISTS(SELECT 1 FROM hr_time_entries t WHERE t.engagement_id=e.id
        AND t.work_date BETWEEN ? AND ? AND t.status='approved' AND t.overtime_hours>0)
        OR EXISTS(SELECT 1 FROM overtime_payroll_handoffs h WHERE h.engagement_id=e.id
        AND h.work_date BETWEEN ? AND ? AND h.minutes>0)) ORDER BY e.id LIMIT 20`)
                .bind(currency, period_start, period_end, companyId, pay_group_id, period_end2, period_start2, scope_engagement_id, scope_engagement_id2, period_start3, period_end3, period_start4, period_end4).all<{
                person_id: string;
                legal_name: string;
            }>();
        },
        readPayrollReadinessPendingTime(companyId: string, pay_group_id: string, period_start: string, period_end: string) {
            return database.prepare("SELECT COUNT(*) AS count FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id WHERE e.company_id=? AND e.pay_group_id=? AND t.work_date BETWEEN ? AND ? AND t.status IN ('draft','submitted')").bind(companyId, pay_group_id, period_start, period_end).first<{
                count: number;
            }>();
        },
        readSubmitPayrollReviewRun(runId: string, companyId: string) {
            return database.prepare("SELECT id,status,version,input_digest FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId, companyId).first<{
                id: string;
                status: string;
                version: number;
                input_digest: string | null;
            }>();
        },
        guard41(key: string, value: "preview", runId: string, companyId: string, expectedVersion: number, inputDigest: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='preview' AND version=? AND input_digest=?)", [runId, companyId, expectedVersion, inputDigest]); },
        stageSubmitPayrollReviewEffect(userId: string, inputDigest: string, now: number, now2: number, runId: string, companyId: string, expectedVersion: number) { return database.prepare("UPDATE hr_payroll_runs SET status='in_review',submitted_by=?,submitted_digest=?,submitted_at=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND status='preview' AND version=?").bind(userId, inputDigest, now, now2, runId, companyId, expectedVersion); },
        readGetPayrollScheduleGroup(groupId: string, companyId: string) { return database.prepare("SELECT id,frequency,timezone,currency,schedule_enabled,pay_delay_days,schedule_anchor_date,schedule_version FROM hr_pay_groups WHERE id=? AND company_id=?").bind(groupId, companyId).first<Schedule>(); },
        readSavePayrollScheduleGroup(groupId: string, companyId: string) {
            return database.prepare("SELECT id,frequency,schedule_version FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active'").bind(groupId, companyId).first<{
                id: string;
                frequency: string;
                schedule_version: number;
            }>();
        },
        guard45(key: string, value: "pay-group", groupId: string, companyId: string, expectedVersion: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active' AND schedule_version=?)", [groupId, companyId, expectedVersion]); },
        stageSavePayrollScheduleEffect(enabled: boolean, payDelayDays: number, value: string | null, now: number, groupId: string, companyId: string, expectedVersion: number) { return database.prepare("UPDATE hr_pay_groups SET schedule_enabled=?,pay_delay_days=?,schedule_anchor_date=?,schedule_version=schedule_version+1,updated_at=? WHERE id=? AND company_id=? AND schedule_version=?").bind(enabled, payDelayDays, value, now, groupId, companyId, expectedVersion); },
        readDrainPayrollSchedulesGroups() { return database.prepare(`SELECT id,company_id,currency,country_code,frequency,timezone,schedule_enabled,pay_delay_days,schedule_anchor_date,schedule_version FROM hr_pay_groups WHERE status='active' AND schedule_enabled=TRUE AND ${purchase("hr_pay_groups.company_id","new-work")} ORDER BY id`).all<Schedule>(); },
        readDrainPayrollSchedulesExisting(company_id: string, id: string, periodStart: string, periodEnd: string) { return database.prepare("SELECT id FROM hr_payroll_runs WHERE company_id=? AND pay_group_id=? AND period_start=? AND period_end=? AND mode!='final_pay' AND adjustment_of_run_id IS NULL AND status!='cancelled' LIMIT 1").bind(company_id, id, periodStart, periodEnd).first(); },
        async stageDrainPayrollSchedulesResult(id: string, company_id: string, id2: string, periodStart: string, periodEnd: string, payDate: string, currency: string, timestamp: number, timestamp2: number, company_id2: string, id3: string, periodStart2: string, periodEnd2: string, scheduleVersion: number) {
            const insert = (tx: SqlDatabase) => tx.prepare("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,country_pack_id,period_start,period_end,pay_date,currency,mode,status,created_by,created_at,updated_at) SELECT ?,?,?,NULL,?,?,?,?,'preview','draft','system:payroll-schedule',?,? WHERE NOT EXISTS(SELECT 1 FROM hr_payroll_runs WHERE company_id=? AND pay_group_id=? AND period_start=? AND period_end=? AND mode!='final_pay' AND adjustment_of_run_id IS NULL AND status!='cancelled') ON CONFLICT DO NOTHING").bind(id, company_id, id2, periodStart, periodEnd, payDate, currency, timestamp, timestamp2, company_id2, id3, periodStart2, periodEnd2).run();
            if (mode === "disabled") return insert(database);
            let image: string | undefined;
            return fromNative(() => withPayrollJobsPurchasedTransaction(database, async (tx, permits) => {
                if (!await permits(company_id,"new-work")) return {success:true,meta:{changes:0}};
                const group = await tx.prepare("SELECT to_jsonb(g)::text image FROM hr_pay_groups g WHERE g.id=? AND g.company_id=? AND g.status='active' AND g.schedule_enabled AND g.schedule_version=? FOR SHARE").bind(id2,company_id,scheduleVersion).first<{image:string}>();
                if (!group) return {success:true,meta:{changes:0}};
                image=group.image;
                return insert(tx);
            }, async tx => {
                if (image && image !== await tx.prepare("SELECT to_jsonb(g)::text image FROM hr_pay_groups g WHERE g.id=? AND g.company_id=?").bind(id2,company_id).first<string>("image")) throw new ApiError(409,"PAYROLL_SCHEDULE_CHANGED","The payroll schedule changed before its draft committed.");
            }));
        },
        readDrainPayrollScheduledPreviewsDue(value: number) {
            return database.prepare(`SELECT r.id,g.timezone FROM hr_payroll_runs r JOIN hr_pay_groups g ON g.id=r.pay_group_id
    WHERE r.created_by='system:payroll-schedule' AND r.mode='preview' AND r.status IN ('draft','failed')
      AND g.status='active' AND g.schedule_enabled=TRUE AND (r.status='draft' OR r.updated_at<?) AND ${purchase('r.company_id','settle-existing')}
    ORDER BY r.period_end,r.id LIMIT 25`).bind(value).all<{
                id: string;
                timezone: string;
            }>();
        },
        async readDrainPayrollScheduledPreviewsRun(id: string) {
            const run = await database.prepare(`SELECT r.id,r.company_id,r.pay_group_id,r.period_start,r.period_end,r.pay_date,r.currency,r.status,r.version,r.revision,g.timezone,
          g.overtime_multiplier_bps,g.proration_method FROM hr_payroll_runs r JOIN hr_pay_groups g ON g.id=r.pay_group_id
          WHERE r.id=? AND r.mode='preview' AND r.created_by='system:payroll-schedule' AND r.status IN ('draft','failed')
            AND g.status='active' AND g.schedule_enabled=TRUE FOR UPDATE OF r SKIP LOCKED`).bind(id).first<{
                id: string;
                company_id: string;
                pay_group_id: string;
                period_start: string;
                period_end: string;
                pay_date: string;
                currency: string;
                status: string;
                version: number;
                revision: number;
                timezone: string;
                overtime_multiplier_bps: number;
                proration_method: "calendar_days" | "none";
            }>();
            if (run && permits) {
                if (!await permits(run.company_id,"settle-existing")) return null;
                const image = await database.prepare("SELECT to_jsonb(g)::text image FROM hr_pay_groups g WHERE g.id=? AND g.company_id=? AND g.status='active' AND g.schedule_enabled AND g.timezone=? AND g.overtime_multiplier_bps=? AND g.proration_method=? FOR SHARE").bind(run.pay_group_id,run.company_id,run.timezone,run.overtime_multiplier_bps,run.proration_method).first<string>("image");
                if (!image) return null;
                sourceChecks.push(async () => {
                    if (image !== await database.prepare("SELECT to_jsonb(g)::text image FROM hr_pay_groups g WHERE g.id=? AND g.company_id=?").bind(run.pay_group_id,run.company_id).first<string>("image")) throw new ApiError(409,"PAYROLL_SCHEDULE_CHANGED","The payroll schedule changed before its preview committed.");
                });
            }
            return run;
        },
        readDrainPayrollScheduledPreviewsPending(company_id: string, pay_group_id: string, period_start: string, period_end: string) {
            return database.prepare(`SELECT COUNT(*) AS count FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id
          WHERE e.company_id=? AND e.pay_group_id=? AND t.work_date BETWEEN ? AND ? AND t.status IN ('draft','submitted')`).bind(company_id, pay_group_id, period_start, period_end).first<{
                count: number;
            }>();
        },
        readDrainPayrollScheduledPreviewsContractors(company_id: string, pay_group_id: string, period_end: string, period_start: string) {
            return database.prepare(`SELECT COUNT(*) AS count FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id
          WHERE e.company_id=? AND e.pay_group_id=? AND e.worker_type='contractor' AND e.status IN ('active','ended')
            AND p.status='active' AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?)`).bind(company_id, pay_group_id, period_end, period_start).first<{
                count: number;
            }>();
        },
        readDrainPayrollScheduledPreviewsEngagements(company_id: string, pay_group_id: string, currency: string, period_end: string, period_start: string) {
            return database.prepare(`SELECT e.id,e.version,e.person_id,e.worker_type,e.employment_type,e.start_date,e.end_date,e.job_title,e.branch_id,e.department_id
          FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=?
            AND e.worker_type='employee' AND e.status IN ('active','ended') AND p.status='active' AND p.merged_into_id IS NULL
            AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) ORDER BY e.id`).bind(company_id, pay_group_id, currency, period_end, period_start).all<{
                id: string;
                version: number;
                person_id: string;
                worker_type: string;
                employment_type: string;
                start_date: string;
                end_date: string | null;
                job_title: string;
                branch_id: string | null;
                department_id: string | null;
            }>();
        },
        readDrainPayrollScheduledPreviewsIncomplete(company_id: string, pay_group_id: string, currency: string, period_end: string, period_start: string, currency2: string, period_start2: string, period_end2: string) {
            return database.prepare(`SELECT COUNT(*) AS count FROM hr_engagements e WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=?
          AND e.worker_type='employee' AND e.status IN ('active','ended') AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?)
          AND NOT EXISTS(SELECT 1 FROM hr_compensation_revisions c WHERE c.engagement_id=e.id AND c.currency=?
            AND c.status IN ('active','approved') AND c.effective_from<=? AND (c.effective_to IS NULL OR c.effective_to>=?))`).bind(company_id, pay_group_id, currency, period_end, period_start, currency2, period_start2, period_end2).first<{
                count: number;
            }>();
        },
        stageDrainPayrollScheduledPreviewsCompleted(value: string, digest: string, timestamp: number, id: string) {
            return database.prepare(`UPDATE hr_payroll_runs SET status='preview',revision=revision+1,totals_json=?,input_digest=?,
          submitted_by=NULL,submitted_digest=NULL,submitted_at=NULL,version=version+1,updated_at=?,error_message=NULL WHERE id=?`)
                .bind(value, digest, timestamp, id).run();
        },
        stageDrainPayrollScheduledPreviewsCompleted2(value: string, id: string, engagementId: string, workerType: string, value2: string, value3: string, gross: number, deductions: number, employer: number, net: number, currency: string, timestamp: number) {
            return database.prepare(`INSERT INTO hr_payroll_items(id,run_id,engagement_id,worker_type,input_snapshot_json,result_json,
            gross_minor,deductions_minor,employer_cost_minor,net_minor,currency,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)`)
                .bind(value, id, engagementId, workerType, value2, value3, gross, deductions, employer, net, currency, timestamp).run();
        },
        stageDrainPayrollScheduledPreviewsCompleted3(...args) { return systemPayrollPreviewEvents(database).stageDrainPayrollScheduledPreviewsCompleted3(...args); },
        stageDrainPayrollScheduledPreviewsCompleted4(...args) { return systemPayrollPreviewEvents(database).stageDrainPayrollScheduledPreviewsCompleted4(...args); },
        stageDrainPayrollScheduledPreviewsEffect(value: string, value2: number, id: string) {
            return database.prepare(`UPDATE hr_payroll_runs SET status='failed',error_message=?,updated_at=?
        WHERE id=? AND status IN ('draft','failed') AND created_by='system:payroll-schedule'`).bind(value, value2, id).run();
        } };
    return { repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, hash: sha256, authorize: requirePermission, prepare: async (holding) => { database = provided ?? (await holdingResources(holding)).write; }, commands: { replay: (...args) => replayCommand(database, ...args), commit: (...args) => executeCommand(database, ...args) }, thaiEvidence: (...args) => fromNative(() => thaiEvidenceForEmployee(database, ...args)), thaiGuards: (...args) => thaiEvidenceGuards(database, ...args), adjustmentInputs: (run) => fromNative(() => payrollAdjustmentInputs(database, run)), adjustmentFresh: run => fromNative(() => payrollAdjustmentSourceFresh(database, run)), adjustmentGuards: (...args) => payrollAdjustmentGuards(database, ...args), calculate: (...args) => calculatePayrollItemForSystem(database, ...args), transactional: provided ? isTransactionalSqlDatabase(provided) : false, transaction: run => {
            if (!isTransactionalSqlDatabase(database))
                throw new Error('PAYROLL_TRANSACTION_REQUIRED');
            if (mode === "disabled") return database.transaction(tx => run(context(tx)));
            const checks: (() => Promise<void>)[] = [];
            return fromNative(() => withPayrollJobsPurchasedTransaction(database, (tx, permits) => run(context(tx, mode, permits, checks)), async () => {for (const check of checks) await check();}));
        }, deferred: (runId, code) => console.warn(JSON.stringify({ event: 'payroll_scheduled_preview_deferred', runId, code })) };
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
export async function assertPayrollSourceFresh(write: SqlDatabase, runId: string) { return invoke(() => createPayrollReadinessApplication(context(write)).assertPayrollSourceFresh(runId)); }
export async function payrollSourceGuards(write: SqlDatabase, key: string, runId: string) { return invoke(() => createPayrollReadinessApplication(context(write)).payrollSourceGuards(key, runId)); }
export async function payrollReadiness(actor: WorkContext, runId: string) { return withPayrollPurchasedRead(actor, tx => invoke(() => createPayrollReadinessApplication(context(tx)).payrollReadiness(actor, runId))); }
export async function submitPayrollReview(actor: WorkContext, runId: string, input: z.infer<typeof payrollSubmitReviewSchema>, key: string) { return withPayrollPurchasedCommand(actor, {key, operation:"people.payroll.submit-review", purpose:"settle-existing"}, tx => invoke(() => createPayrollReadinessApplication(context(tx)).submitPayrollReview(actor, runId, input, key))); }
export async function getPayrollSchedule(actor: WorkContext, groupId: string) { return withPayrollPurchasedRead(actor, tx => invoke(() => createPayrollReadinessApplication(context(tx)).getPayrollSchedule(actor, groupId))); }
export async function savePayrollSchedule(actor: WorkContext, groupId: string, input: z.infer<typeof payrollScheduleSchema>, key: string) { return withPayrollPurchasedCommand(actor, {key, operation:"people.payroll.schedule", purpose:input.enabled?"new-work":"settle-existing"}, tx => invoke(() => createPayrollReadinessApplication(context(tx)).savePayrollSchedule(actor, groupId, input, key))); }
export async function drainPayrollSchedules(write: SqlDatabase, now = new Date()) { const mode=await peoplePackageEnforcementMode();return invoke(() => createPayrollReadinessApplication(context(write,mode)).drainPayrollSchedules(now)); }
export async function drainPayrollScheduledPreviews(write: SqlDatabase, now = new Date()) { const mode=await peoplePackageEnforcementMode();return invoke(() => createPayrollReadinessApplication(context(write,mode)).drainPayrollScheduledPreviews(now)); }
