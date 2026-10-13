import {peopleVerifiedContextProjections as organizationFinalOwnerProjections} from "../../organization/public/people-payroll-verified-context-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import {payrollDecisionNotificationEffects} from '../../collaboration/public/payroll-decision-notification-effects';
import { readListPage } from "@/server/modules/work_projection/public/list-page";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { organizationGuard, validateStaffOrganization } from "@/server/modules/organization/public/organization";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources, runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { ApiError } from "@/server/modules/runtime/public/http";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { calculateThaiHotelPayrollFixture, isThaiHotelPayrollMockEnabled, thaiHotelPayrollMockId } from "@/server/modules/runtime/public/simulation-only-adapters";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { directoryPerson, canReadCompensation, canManagePeople } from "@/server/modules/people_payroll/adapters/people-directory";
import { prepareJournal } from "@/server/modules/finance/public/finance";
import { type PayrollPostingItem } from "@/server/modules/people_payroll/adapters/payroll-accounting";
import { assertPayrollSourceFresh, payrollSourceGuards } from "@/server/modules/people_payroll/adapters/payroll-readiness";
import { prepareAttachmentBinding } from "@/server/modules/files/public/attachments/service";
import { snapshotMoney } from "@/server/modules/finance/public/company-money";
import { assertStatutoryRunReady, recognizedThaiPack, thaiEvidenceForEmployee, thaiEvidenceGuards, thaiPackForRun, type PayrollStatutoryRun } from "@/server/modules/people_payroll/adapters/payroll-statutory";
import { preparePayslipIssue } from "@/server/modules/people_payroll/adapters/payroll-payslips";
import { calculatePayrollAdjustmentRun } from "@/server/modules/people_payroll/adapters/payroll-adjustments";
import { assertPayrollRecoveryDeductionLimits } from "@/server/modules/people_payroll/adapters/payroll-recoveries";
import { preparePayrollDeductionConsumption } from "@/server/modules/people_payroll/adapters/payroll-deductions";
import type { FileStore } from '../../runtime/public/runtime/types';
import { BusinessRuleError } from '../../shared';
import { createPersonSchema, createEngagementSchema, compensationSchema, compensationTransitionSchema, engagementTransitionSchema, payrollCostVerificationSchema, payGroupSchema, countryPackSchema, countryPackTransitionSchema, payrollRunSchema, paymentBatchSchema, timeEntrySchema, paymentConfirmationSchema, payrollPostSchema, payrollAccountMappingSchema, payrollCorrectionRequestSchema, payrollCorrectionDecisionSchema, finalPayPreparationSchema, payrollAdvanceRequestSchema, payrollAdvanceDecisionSchema, assignPayGroupSchema, timeEntryUpdateSchema, timeEntryReviewSchema } from '../domain/people-payroll';
import { createPeoplePayrollApplication, type PeoplePayrollRepository, type PeoplePayrollPorts } from '../application/people-payroll';
import { payrollWorkflowFacts } from '../../workflow/public/payroll-owner-facts';
import { payrollOrganizationFacts } from '../../organization/public/payroll-owner-facts';
import { payrollIdentityFacts } from '../../identity_access/public/payroll-owner-facts';
import { payrollBenefitOwnerFacts } from '../../benefits/public/payroll-owner-facts';
import { payrollFinanceFacts } from '../../finance/public/payroll-owner-facts';
export { createPersonSchema, createEngagementSchema, compensationSchema, compensationTransitionSchema, engagementTransitionSchema, payrollCostVerificationSchema, payGroupSchema, countryPackSchema, countryPackTransitionSchema, payrollRunSchema, paymentBatchSchema, timeEntrySchema, paymentConfirmationSchema, payrollPostSchema, payrollAccountMappingSchema, payrollCorrectionRequestSchema, payrollCorrectionDecisionSchema, finalPayPreparationSchema, payrollAdvanceRequestSchema, payrollAdvanceDecisionSchema, assignPayGroupSchema, timeEntryUpdateSchema, timeEntryReviewSchema } from '../domain/people-payroll';
export { cancelPayrollRun, payrollCancelSchema } from "@/server/modules/people_payroll/adapters/payroll-cancellation";
export { createPayrollAdjustment, editPayrollAdjustment, payrollAdjustmentCreateSchema, payrollAdjustmentEditSchema } from "@/server/modules/people_payroll/adapters/payroll-adjustments";
function context(provided?: SqlDatabase): PeoplePayrollPorts<SqlStatement> {
    let database = provided!, files: FileStore;
    const repository: PeoplePayrollRepository<SqlStatement> = { readPayrollCostAssignmentsEngagement(engagementId: string, companyId: string) { return database.prepare("SELECT id FROM hr_engagements WHERE id=? AND company_id=?").bind(engagementId, companyId).first(); },
        readPayrollCostAssignmentsEffect(engagementId: string, companyId: string) {
            return database.prepare(`SELECT a.id,a.effective_from,a.effective_to,a.branch_id,a.department_id,a.evidence_status,a.source,a.version,
    v.evidence_ref,v.verified_by,v.created_at verified_at FROM hr_payroll_cost_assignments a
    LEFT JOIN hr_payroll_cost_assignment_verifications v ON v.assignment_id=a.id
    WHERE a.engagement_id=? AND a.company_id=? AND a.status='active' ORDER BY a.effective_from,a.id`)
                .bind(engagementId, companyId).all();
        },
        readVerifyPayrollCostAssignmentAssignment(assignmentId: string, companyId: string) {
            return database.prepare(`SELECT a.id,a.engagement_id,a.effective_from,a.effective_to,a.branch_id,a.department_id,a.evidence_status,a.source,a.version,
    e.start_date,e.branch_id current_branch_id,e.department_id current_department_id FROM hr_payroll_cost_assignments a
    JOIN hr_engagements e ON e.id=a.engagement_id AND e.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.status='active'`)
                .bind(assignmentId, companyId).first<{
                id: string;
                engagement_id: string;
                effective_from: string;
                effective_to: string | null;
                branch_id: string | null;
                department_id: string | null;
                evidence_status: string;
                source: string;
                version: number;
                start_date: string;
                current_branch_id: string | null;
                current_department_id: string | null;
            }>();
        },
        readVerifyPayrollCostAssignmentHistory(engagement_id: string, companyId: string) { return database.prepare("SELECT 1 FROM staff_organization_history WHERE engagement_id=? AND company_id=? LIMIT 1").bind(engagement_id, companyId).first(); },
        guard5(commandId: string, value: "assignment-version", assignmentId: string, companyId: string, expectedVersion: number) { return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM hr_payroll_cost_assignments a JOIN hr_engagements e ON e.id=a.engagement_id AND e.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.status='active' AND a.evidence_status='legacy_unverified' AND a.source='migration' AND a.version=? AND a.branch_id IS NOT DISTINCT FROM e.branch_id AND a.department_id IS NOT DISTINCT FROM e.department_id AND NOT EXISTS(SELECT 1 FROM staff_organization_history h WHERE h.engagement_id=a.engagement_id AND h.company_id=a.company_id))", [assignmentId, companyId, expectedVersion]); },
        stageVerifyPayrollCostAssignmentEffect(now: number, assignmentId: string, companyId: string, expectedVersion: number) { return database.prepare("UPDATE hr_payroll_cost_assignments SET evidence_status='verified',source='hr_attestation',version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?").bind(now, assignmentId, companyId, expectedVersion); },
        stageVerifyPayrollCostAssignmentEffect2(value: string, companyId: string, assignmentId: string, userId: string, evidenceRef: string, expectedVersion: number, now: number) { return database.prepare("INSERT INTO hr_payroll_cost_assignment_verifications(id,company_id,assignment_id,verified_by,evidence_ref,previous_version,created_at) VALUES(?,?,?,?,?,?,?)").bind(value, companyId, assignmentId, userId, evidenceRef, expectedVersion, now); },
        stageVerifyPayrollCostAssignmentEffect3(assignmentId: string) { return database.prepare("SELECT set_config('erpx.payroll_cost_repair',?,true)").bind(assignmentId); },
        stageVerifyPayrollCostAssignmentEffect4(now: number, assignmentId: string, companyId: string) {
            return database.prepare(`UPDATE hr_time_entries t SET cost_assignment_id=a.id,cost_branch_id=a.branch_id,cost_department_id=a.department_id,version=t.version+1,updated_at=?
      FROM hr_payroll_cost_assignments a WHERE a.id=? AND a.company_id=? AND t.engagement_id=a.engagement_id AND t.status='approved'
      AND t.work_date>=a.effective_from AND (a.effective_to IS NULL OR t.work_date<=a.effective_to)
      AND t.cost_assignment_id IS NULL`).bind(now, assignmentId, companyId);
        },
        readCommandCurrent(...args) { return payrollWorkflowFacts(database).readCommandCurrent(...args); },
        readCommandOld(id: string) { return database.prepare("SELECT 1 FROM hr_command_receipts WHERE command_id=?").bind(id).first(); },
        stageReceiptEffect(id: string, operation: string, digest: string, value: string, now: number) {
            return database.prepare("INSERT INTO hr_command_receipts(command_id,operation,request_hash,result_json,created_at) VALUES(?,?,?,?,?)")
                .bind(id, operation, digest, value, now);
        },
        readListPeopleRows(...args) { return payrollOrganizationFacts(database).readListPeopleRows(...args); },
        readGetPersonCompensation(engagement_id: string | number | null) { return database.prepare("SELECT base_amount_minor,hourly_rate_minor,pay_frequency,effective_from AS compensation_effective_from FROM hr_compensation_revisions WHERE engagement_id=? AND status='approved' AND effective_from<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (effective_to IS NULL OR effective_to>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) ORDER BY effective_from DESC LIMIT 1").bind(engagement_id).first(); },
        readGetPersonLeavetime(personId: string, companyId: string, branchId: string | null, branchId2: string | null) { return database.prepare("SELECT l.id,l.number,l.start_date,l.days,l.state,l.decision_note FROM leave_requests l JOIN employees e ON e.id=l.employee_id WHERE e.person_id=? AND l.company_id=? AND (?::text IS NULL OR l.branch_id=?) ORDER BY l.start_date DESC LIMIT 24").bind(personId, companyId, branchId, branchId2).all(); },
        readGetPersonLeavetime2(engagement_id: string | number | null) { return database.prepare("SELECT id,version,submitted_by,approved_by,work_date,regular_hours,overtime_hours,status,note,rejection_reason FROM hr_time_entries WHERE engagement_id=? ORDER BY work_date DESC LIMIT 24").bind(engagement_id).all(); },
        readGetPersonRevisions(engagement_id: string | number | null) { return database.prepare("SELECT * FROM hr_compensation_revisions WHERE engagement_id=? ORDER BY effective_from DESC,created_at DESC").bind(engagement_id).all(); },
        readGetPersonRecommendedPay(companyId: string, position_id: string | number) {
            return database.prepare(`SELECT currency,monthly_amount_minor,version ${organizationFinalSources.people_payroll_position_pay_defaults_from} WHERE company_id=? AND position_id=? ORDER BY version DESC LIMIT 1`).bind(companyId, position_id).first<{
                currency: string;
                monthly_amount_minor: number;
                version: number;
            }>();
        },
        readGetPersonRecommendedLeave(companyId: string, level_id: string | number) {
            return database.prepare(`SELECT p.name,d.version,
    max(CASE WHEN t.code='annual' THEN r.grant_units END) annual_days,
    max(CASE WHEN t.code='sick' THEN r.grant_units END) sick_days
    ${organizationFinalSources.people_payroll_level_leave_defaults_d_from} JOIN leave_policy_versions p ON p.id=d.policy_version_id
    JOIN leave_policy_rules r ON r.policy_version_id=p.id JOIN leave_types t ON t.id=r.leave_type_id
    WHERE d.company_id=? AND d.level_id=? AND d.version=(SELECT max(x.version) ${organizationFinalSources.people_payroll_level_leave_defaults_x_from} WHERE x.company_id=d.company_id AND x.level_id=d.level_id)
    GROUP BY p.name,d.version`).bind(companyId, level_id).first<{
                name: string;
                version: number;
                annual_days: number;
                sick_days: number;
            }>();
        },
        readGetPersonLeaveAssignment(companyId: string, engagement_id: string | number) {
            return database.prepare(`SELECT p.name,p.version,a.effective_from,a.effective_to FROM leave_policy_assignments a JOIN leave_policy_versions p ON p.id=a.policy_version_id WHERE a.company_id=? AND a.engagement_id=? AND a.effective_from<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (a.effective_to IS NULL OR a.effective_to>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) ORDER BY a.effective_from DESC LIMIT 1`).bind(companyId, engagement_id).first<{
                name: string;
                version: number;
                effective_from: string;
                effective_to: string | null;
            }>();
        },
        readPeopleSummaryPeopleactivecontractorsrunspending(actor: WorkContext) {
            const scope = scopeWhere(actor, 'e');
            return database.prepare(`SELECT COUNT(DISTINCT p.id) AS count FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id WHERE ${scope.sql} AND p.status!='archived' AND p.merged_into_id IS NULL`).bind(...scope.args).first<{
                count: number;
            }>();
        },
        readPeopleSummaryPeopleactivecontractorsrunspending2(actor: WorkContext) {
            const scope = scopeWhere(actor, 'e');
            return database.prepare(`SELECT COUNT(*) AS count FROM hr_engagements e WHERE ${scope.sql} AND e.status='active'`).bind(...scope.args).first<{
                count: number;
            }>();
        },
        readPeopleSummaryPeopleactivecontractorsrunspending3(actor: WorkContext) {
            const scope = scopeWhere(actor, 'e');
            return database.prepare(`SELECT COUNT(*) AS count FROM hr_engagements e WHERE ${scope.sql} AND e.worker_type='contractor' AND e.status='active'`).bind(...scope.args).first<{
                count: number;
            }>();
        },
        readPeopleSummaryPeopleactivecontractorsrunspending4(companyId: string) {
            return database.prepare("SELECT COUNT(*) AS count FROM hr_payroll_runs WHERE company_id=? AND status IN ('draft','calculating','preview')").bind(companyId).first<{
                count: number;
            }>();
        },
        readPeopleSummaryPeopleactivecontractorsrunspending5(actor: WorkContext, value: 0 | 1, userId: string) {
            const scope = scopeWhere(actor, 'e');
            return database.prepare(`SELECT COUNT(*) AS count FROM leave_requests e WHERE ${scope.sql} AND e.state='pending_approval' AND (?=1 OR e.created_by=?)`).bind(...scope.args, value, userId).first<{
                count: number;
            }>();
        },
        readPayrollSummaryEffect(companyId: string, value: string | null, value2: string | null) {
            return database.prepare(`SELECT substring(r.pay_date FROM 1 FOR 7) pay_month,r.currency,
    COUNT(DISTINCT r.id) run_count,COUNT(DISTINCT i.engagement_id) headcount,
    SUM(i.gross_minor) gross_minor,SUM(i.deductions_minor) deductions_minor,SUM(i.employer_cost_minor) employer_cost_minor,SUM(i.net_minor) net_minor
    FROM hr_payroll_runs r JOIN hr_payroll_items i ON i.run_id=r.id
    WHERE r.company_id=? AND r.status='posted' AND (?::text IS NULL OR substring(r.pay_date FROM 1 FOR 7)=?)
    GROUP BY substring(r.pay_date FROM 1 FOR 7),r.currency ORDER BY pay_month DESC,r.currency LIMIT 120`)
                .bind(companyId, value, value2).all<{
                pay_month: string;
                currency: string;
                run_count: number;
                headcount: number;
                gross_minor: number;
                deductions_minor: number;
                employer_cost_minor: number;
                net_minor: number;
            }>();
        },
        stageCreatePersonEffect(personId: string, value: null, companyId: string, legalName: string, value2: string | null, value3: string | null, value4: string | null, countryCode: string, userId: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,preferred_name,email,phone,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(personId, value, companyId, legalName, value2, value3, value4, countryCode, userId, now, now2); },
        readCreateEngagementPerson(personId: string, companyId: string) { return database.prepare("SELECT id FROM hr_people WHERE id=? AND company_id=? AND status!='archived'").bind(personId, companyId).first(); },
        stageCreateEngagementEffect(engagementId: string, personId: string, companyId: string, value: string | null, value2: string | null, workerType: "employee" | "contractor", employmentType: "contractor" | "salaried" | "daily" | "hourly" | "shift", value3: "onboarding", startDate: string, value4: string | null, value5: string | null, jobTitle: string, currency: string, value6: string | null, userId: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,end_date,manager_person_id,job_title,currency,pay_group_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(engagementId, personId, companyId, value, value2, workerType, employmentType, value3, startDate, value4, value5, jobTitle, currency, value6, userId, now, now2); },
        readCreateCompensationE(engagementId: string, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) { return database.prepare("SELECT e.* FROM hr_engagements e WHERE e.id=? AND e.company_id=? AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)").bind(engagementId, companyId, branchId, branchId2, departmentId, departmentId2).first<Record<string, string | number | null>>(); },
        guard31(key: string, value: "engagement", engagementId: string, version: string | number | null, currency: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=? AND currency=?)", [engagementId, version, currency]); },
        stageCreateCompensationEffect(id: string, engagementId: string, effectiveFrom: string, value: string | null, payFrequency: "weekly" | "fortnightly" | "semimonthly" | "monthly" | "off_cycle", base: number, hourly: number, daily: number, standardDailyHours: number, expectedHours: number, currency: string, value2: string, userId: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,effective_to,pay_frequency,base_amount_minor,hourly_rate_minor,daily_rate_minor,standard_daily_hours,expected_hours,currency,components_json,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'draft',?,?,?)").bind(id, engagementId, effectiveFrom, value, payFrequency, base, hourly, daily, standardDailyHours, expectedHours, currency, value2, userId, now, now2); },
        readPayrollOptionsEffect(companyId: string) { return database.prepare("SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=? AND status='active' ORDER BY name").bind(companyId).all(); },
        readAssignPayGroupMissing(...args) { return payrollOrganizationFacts(database).readAssignPayGroupMissing(...args); },
        guard35(key: string, value: "engagement", engagementId: string, companyId: string, expectedVersion: number, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?))", [engagementId, companyId, expectedVersion, branchId, branchId2, departmentId, departmentId2]); },
        guard36(...args) { return payrollOrganizationFacts(database).guard36(...args); },
        guard37(key: string, value: "pay-group", payGroupId: string | null, engagementId: string, payGroupId2: string | null) { return assertion(database, key, value, "?::text IS NULL OR EXISTS(SELECT 1 FROM hr_pay_groups p JOIN hr_engagements e ON e.id=? WHERE p.id=? AND p.company_id=e.company_id AND p.currency=e.currency AND p.status='active')", [payGroupId, engagementId, payGroupId2]); },
        stageAssignPayGroupEffect(payGroupId: string | null, now: number, engagementId: string) { return database.prepare("UPDATE hr_engagements SET pay_group_id=?,version=version+1,updated_at=? WHERE id=?").bind(payGroupId, now, engagementId); },
        readTransitionCompensationEffect(compensationId: string, companyId: string) { return database.prepare("SELECT 1 FROM hr_evaluation_compensation_drafts WHERE compensation_id=? AND company_id=?").bind(compensationId, companyId).first(); },
        readTransitionCompensationRow(compensationId: string, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) { return database.prepare("SELECT c.*,e.person_id,e.branch_id,e.department_id FROM hr_compensation_revisions c JOIN hr_engagements e ON e.id=c.engagement_id WHERE c.id=? AND e.company_id=? AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)").bind(compensationId, companyId, branchId, branchId2, departmentId, departmentId2).first<Record<string, string | number | null>>(); },
        guard41(key: string, value: "revision", compensationId: string, expectedVersion: number, value2: "approved" | "draft") { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE id=? AND version=? AND status=?)", [compensationId, expectedVersion, value2]); },
        guard42(key: string, value: "overlap", engagement_id: string | number | null, compensationId: string, effective_to: string | number | null, effective_from: string | number | null) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE engagement_id=? AND id!=? AND status IN ('active','approved') AND effective_from<=COALESCE(?,'9999-12-31') AND COALESCE(effective_to,'9999-12-31')>=?)", [engagement_id, compensationId, effective_to, effective_from]); },
        stageTransitionCompensationEffect(status: "approved" | "superseded", userId: string, now: number, compensationId: string) { return database.prepare("UPDATE hr_compensation_revisions SET status=?,approved_by=?,version=version+1,updated_at=? WHERE id=?").bind(status, userId, now, compensationId); },
        stageTransitionCompensationEffect2(...args) { return payrollWorkflowFacts(database).stageTransitionCompensationEffect2(...args); },
        readTransitionEngagementRow(engagementId: string, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) {
            return database.prepare("SELECT e.*,p.user_id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.id=? AND e.company_id=? AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)").bind(engagementId, companyId, branchId, branchId2, departmentId, departmentId2).first<{
                id: string;
                person_id: string;
                user_id: string | null;
                status: string;
                version: number;
                start_date: string;
                end_date: string | null;
                branch_id: string | null;
                department_id: string | null;
                position_id: string | null;
                level_id: string | null;
                manager_person_id: string | null;
            }>();
        },
        guard46(key: string, value: "engagement", engagementId: string, companyId: string, expectedVersion: number, status: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND status=?)", [engagementId, companyId, expectedVersion, status]); },
        guard47(...args) { return payrollOrganizationFacts(database).guard47(...args); },
        stageTransitionEngagementEffect(status: "active" | "suspended" | "ended", status2: "active" | "suspended" | "ended", value: string | null, now: number, engagementId: string) { return database.prepare("UPDATE hr_engagements SET status=?,end_date=CASE WHEN ?='ended' THEN ? ELSE end_date END,version=version+1,updated_at=? WHERE id=?").bind(status, status2, value, now, engagementId); },
        stageTransitionEngagementEffect2(person_id: string, person_id2: string) { return database.prepare("UPDATE employees SET status=CASE WHEN EXISTS(SELECT 1 FROM hr_engagements WHERE person_id=? AND status='active') THEN 'active' ELSE 'inactive' END WHERE person_id=?").bind(person_id, person_id2); },
        stageTransitionEngagementEffect3(...args) { return payrollIdentityFacts(database).stageTransitionEngagementEffect3(...args); },
        readCreateTimeEntryE(engagementId: string, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) { return database.prepare("SELECT e.*,p.user_id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.id=? AND e.company_id=? AND e.status='active' AND p.status='active' AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)").bind(engagementId, companyId, branchId, branchId2, departmentId, departmentId2).first<Record<string, string | number | null>>(); },
        guard52(key: string, value: "engagement", engagementId: string, version: string | number | null) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=? AND status='active')", [engagementId, version]); },
        stageCreateTimeEntryEffect(id: string, engagementId: string, companyId: string, workDate: string, regularHours: number, overtimeHours: number, note: string, userId: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_time_entries(id,engagement_id,company_id,work_date,regular_hours,overtime_hours,note,status,submitted_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'submitted',?,?,?)").bind(id, engagementId, companyId, workDate, regularHours, overtimeHours, note, userId, now, now2); },
        readUpdateTimeEntryEntry(entryId: string, companyId: string, userId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) { return database.prepare("SELECT t.*,e.start_date,e.end_date FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id WHERE t.id=? AND t.company_id=? AND t.submitted_by=? AND e.status='active' AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)").bind(entryId, companyId, userId, branchId, branchId2, departmentId, departmentId2).first<Record<string, string | number | null>>(); },
        guard55(key: string, value: "editable-time", entryId: string, expectedVersion: number, userId: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_time_entries WHERE id=? AND version=? AND status IN ('draft','submitted','rejected') AND submitted_by=?)", [entryId, expectedVersion, userId]); },
        stageUpdateTimeEntryEffect(workDate: string, regularHours: number, overtimeHours: number, note: string, now: number, entryId: string) { return database.prepare("UPDATE hr_time_entries SET work_date=?,regular_hours=?,overtime_hours=?,note=?,status='submitted',approved_by=NULL,rejection_reason=NULL,version=version+1,updated_at=? WHERE id=?").bind(workDate, regularHours, overtimeHours, note, now, entryId); },
        guard57(key: string, value: "reviewable-time", entryId: string, companyId: string, expectedVersion: number, userId: string, userId2: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id JOIN hr_people p ON p.id=e.person_id WHERE t.id=? AND t.company_id=? AND t.version=? AND t.status='submitted' AND t.submitted_by!=? AND (p.user_id IS NULL OR p.user_id!=?) AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?))", [entryId, companyId, expectedVersion, userId, userId2, branchId, branchId2, departmentId, departmentId2]); },
        stageRejectTimeEntryEffect(reason: string, now: number, entryId: string) { return database.prepare("UPDATE hr_time_entries SET status='rejected',rejection_reason=?,version=version+1,updated_at=? WHERE id=?").bind(reason, now, entryId); },
        stageRejectTimeEntryEffect2(...args) { return payrollWorkflowFacts(database).stageRejectTimeEntryEffect2(...args); },
        guard60(key: string, value: "time", entryId: string, companyId: string, expectedVersion: number, userId: string, userId2: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id JOIN hr_people p ON p.id=e.person_id WHERE t.id=? AND t.company_id=? AND t.version=? AND t.status='submitted' AND t.submitted_by!=? AND (p.user_id IS NULL OR p.user_id!=?) AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?))", [entryId, companyId, expectedVersion, userId, userId2, branchId, branchId2, departmentId, departmentId2]); },
        stageApproveTimeEntryEffect(userId: string, now: number, entryId: string) { return database.prepare("UPDATE hr_time_entries SET status='approved',approved_by=?,version=version+1,updated_at=? WHERE id=?").bind(userId, now, entryId); },
        stageApproveTimeEntryEffect2(...args) { return payrollWorkflowFacts(database).stageApproveTimeEntryEffect2(...args); },
        stageCreatePayGroupEffect(groupId: string, companyId: string, name: string, countryCode: string, currency: string, frequency: "weekly" | "fortnightly" | "semimonthly" | "monthly", timezone: string, overtimeMultiplierBps: number, prorationMethod: "calendar_days" | "none", userId: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,overtime_multiplier_bps,proration_method,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(groupId, companyId, name, countryCode, currency, frequency, timezone, overtimeMultiplierBps, prorationMethod, userId, now, now2); },
        stageCreateCountryPackEffect(packId: string, countryCode: string, version: string, value: "testing", value2: string, value3: string, userId: string, now: number, now2: number, value4: string | null, value5: string | null, value6: string | null, value7: string | null) { return database.prepare("INSERT INTO hr_country_packs(id,country_code,version,status,rules_json,export_formats_json,created_by,created_at,updated_at,adapter_id,source_digest,effective_from,effective_to) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(packId, countryCode, version, value, value2, value3, userId, now, now2, value4, value5, value6, value7); },
        readListCountryPacksEffect() { return database.prepare("SELECT id,country_code,version,status,effective_from,effective_to,adapter_id,source_digest,record_version,export_formats_json,validated_by,updated_at FROM hr_country_packs ORDER BY country_code,version DESC").all(); },
        readTransitionCountryPackPack(packId: string) {
            return database.prepare("SELECT status,country_code,version,rules_json,created_by,record_version,source_digest,adapter_id FROM hr_country_packs WHERE id=?").bind(packId).first<{
                status: string;
                country_code: string;
                version: string;
                rules_json: string;
                created_by: string;
                record_version: number;
                source_digest: string | null;
                adapter_id: string | null;
            }>();
        },
        guard67(commandId: string, value: "pack-status", packId: string, status: string, record_version: number, source_digest: string) { return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM hr_country_packs WHERE id=? AND status=? AND record_version=? AND source_digest=?)", [packId, status, record_version, source_digest]); },
        stageTransitionCountryPackEffect(status: "superseded" | "active" | "validated", proof: string, userId: string, now: number, packId: string) { return database.prepare("UPDATE hr_country_packs SET status=?,approval_evidence=?,validated_by=?,record_version=record_version+1,updated_at=? WHERE id=?").bind(status, proof, userId, now, packId); },
        stageTransitionCountryPackEffect2(...args) { return payrollWorkflowFacts(database).stageTransitionCountryPackEffect2(...args); },
        readCalculateItemPayee(person_id: string, company_id: string) {
            return database.prepare("SELECT id,version,legal_name,preferred_name FROM hr_people WHERE id=? AND company_id=? AND status='active' AND merged_into_id IS NULL").bind(person_id, company_id).first<{
                id: string;
                version: number;
                legal_name: string;
                preferred_name: string | null;
            }>();
        },
        readCalculateItemCompensation(id: string, period_end: string, period_start: string) { return database.prepare(`SELECT * FROM hr_compensation_revisions WHERE engagement_id=? AND status IN ('active','approved') AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from DESC LIMIT 1`).bind(id, period_end, period_start).first<Record<string, string | number | null>>(); },
        readCalculateItemTime(id: string, period_start: string, period_end: string) {
            return database.prepare("SELECT id,version,work_date,regular_hours,overtime_hours,cost_assignment_id,cost_branch_id,cost_department_id FROM hr_time_entries WHERE engagement_id=? AND work_date BETWEEN ? AND ? AND status='approved' ORDER BY id").bind(id, period_start, period_end).all<{
                id: string;
                version: number;
                work_date: string;
                regular_hours: number;
                overtime_hours: number;
                cost_assignment_id: string | null;
                cost_branch_id: string | null;
                cost_department_id: string | null;
            }>();
        },
        readCalculateItemHandoffs(id: string, period_start: string, period_end: string) {
            return database.prepare('SELECT id,request_id,request_version,minutes,work_date FROM overtime_payroll_handoffs WHERE engagement_id=? AND work_date BETWEEN ? AND ? ORDER BY id').bind(id, period_start, period_end).all<{
                id: string;
                request_id: string;
                request_version: number;
                minutes: number;
                work_date: string;
            }>();
        },
        readCalculateItemBenefitInputs(...args) { return payrollBenefitOwnerFacts(database).readCalculateItemBenefitInputs(...args); },
        readCalculateItemAdvanceRepayments(company_id: string, id: string, period_end: string, period_start: string, id2: string) {
            return database.prepare(`SELECT r.id,r.version,r.amount_minor,r.due_period_start,r.due_period_end,a.reason
      FROM hr_payroll_advance_repayments r JOIN hr_payroll_advances a ON a.id=r.advance_id
      WHERE r.company_id=? AND a.engagement_id=? AND a.status='approved' AND r.status IN ('open','reserved')
        AND r.due_period_start<=? AND r.due_period_end>=? AND (r.payroll_run_id IS NULL OR r.payroll_run_id=?)
      ORDER BY r.sequence`).bind(company_id, id, period_end, period_start, id2).all<{
                id: string;
                version: number;
                amount_minor: number;
                due_period_start: string;
                due_period_end: string;
                reason: string;
            }>();
        },
        readCalculateItemCostAssignments(id: string, company_id: string, activeEnd: string, activeStart: string) {
            return database.prepare("SELECT id,effective_from,effective_to,branch_id,department_id,evidence_status,version FROM hr_payroll_cost_assignments WHERE engagement_id=? AND company_id=? AND status='active' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from,id").bind(id, company_id, activeEnd, activeStart).all<{
                id: string;
                effective_from: string;
                effective_to: string | null;
                branch_id: string | null;
                department_id: string | null;
                evidence_status: string;
                version: number;
            }>();
        },
        guard77(key: string, value: "pay-group", payGroupId: string, companyId: string, currency: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_pay_groups WHERE id=? AND company_id=? AND currency=? AND status='active')", [payGroupId, companyId, currency]); },
        guard78(key: string, value: "duplicate-period", companyId: string, payGroupId: string, periodEnd: string, periodStart: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_payroll_runs WHERE company_id=? AND pay_group_id=? AND mode!='final_pay' AND adjustment_of_run_id IS NULL AND status!='cancelled' AND period_start<=? AND period_end>=?)", [companyId, payGroupId, periodEnd, periodStart]); },
        guard79(key: string, value: "statutory-pack", id: string, record_version: number, sourceDigest: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_country_packs WHERE id=? AND status='active' AND record_version=? AND source_digest=?)", [id, record_version, sourceDigest]); },
        stageCreatePayrollRunEffect(id: string, companyId: string, payGroupId: string, value: string | null, periodStart: string, periodEnd: string, payDate: string, currency: string, mode: "preview" | "production", userId: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,country_pack_id,period_start,period_end,pay_date,currency,mode,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,'calculating',?,?,?)").bind(id, companyId, payGroupId, value, periodStart, periodEnd, payDate, currency, mode, userId, now, now2); },
        readPrepareFinalPayTask(taskId: string, companyId: string) {
            return database.prepare(`SELECT t.id,t.version,t.status,t.assignee_id,t.clearance_type,t.case_id,c.person_id,c.definition,e.id engagement_id,e.pay_group_id,e.currency,e.end_date
    FROM people_preparation_tasks t JOIN people_lifecycle_cases c ON c.id=t.case_id AND c.company_id=t.company_id
    JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id
    WHERE t.id=? AND t.company_id=?`).bind(taskId, companyId).first<{
                id: string;
                version: number;
                status: string;
                assignee_id: string | null;
                clearance_type: string | null;
                case_id: string;
                person_id: string;
                definition: string;
                engagement_id: string;
                pay_group_id: string | null;
                currency: string;
                end_date: string | null;
            }>();
        },
        guard82(key: string, value: "task-version", taskId: string, companyId: string, expectedTaskVersion: number, userId: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM people_preparation_tasks WHERE id=? AND company_id=? AND version=? AND status='pending' AND clearance_type='payroll' AND assignee_id=?)", [taskId, companyId, expectedTaskVersion, userId]); },
        guard83(key: string, value: "engagement-pay-group", engagement_id: string, companyId: string, payGroupId: string, currency: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND pay_group_id=? AND currency=?)", [engagement_id, companyId, payGroupId, currency]); },
        guard84(key: string, value: "one-final-pay", taskId: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_payroll_final_pay_preparations WHERE task_id=? AND status!='cancelled')", [taskId]); },
        guard85(key: string, value: "statutory-pack", id: string, record_version: number, sourceDigest: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_country_packs WHERE id=? AND status='active' AND record_version=? AND source_digest=?)", [id, record_version, sourceDigest]); },
        stagePrepareFinalPayEffect(runId: string, companyId: string, payGroupId: string, value: string | null, engagement_id: string, periodStart: string, periodEnd: string, payDate: string, currency: string, userId: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,country_pack_id,scope_engagement_id,period_start,period_end,pay_date,currency,mode,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,'final_pay','calculating',?,?,?)").bind(runId, companyId, payGroupId, value, engagement_id, periodStart, periodEnd, payDate, currency, userId, now, now2); },
        stagePrepareFinalPayEffect2(prepId: string, companyId: string, taskId: string, case_id: string, engagement_id: string, runId: string, reason: string, userId: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_payroll_final_pay_preparations(id,company_id,task_id,case_id,engagement_id,run_id,reason,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(prepId, companyId, taskId, case_id, engagement_id, runId, reason, userId, now, now2); },
        stagePrepareFinalPayEffect3(value: string, prepId: string, taskId: string, runId: string, companyId: string, userId: string, expectedTaskVersion: number, value2: string, now: number) { return database.prepare("INSERT INTO hr_payroll_final_pay_events(id,preparation_id,task_id,run_id,company_id,action,actor_id,task_version,evidence_json,created_at) VALUES(?,?,?,?,?,'prepared',?,?,?,?)").bind(value, prepId, taskId, runId, companyId, userId, expectedTaskVersion, value2, now); },
        readRequestPayrollAdvanceEngagement(engagementId: string, companyId: string) {
            return database.prepare(`SELECT e.id,e.company_id,e.currency,e.status,e.pay_group_id,p.id person_id,p.user_id,p.legal_name
    FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id
    WHERE e.id=? AND e.company_id=? AND p.status='active'`).bind(engagementId, companyId).first<{
                id: string;
                company_id: string;
                currency: string;
                status: string;
                pay_group_id: string | null;
                person_id: string;
                user_id: string;
                legal_name: string;
            }>();
        },
        guard90(key: string, value: "engagement", engagementId: string, companyId: string, currency: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.id=? AND e.company_id=? AND e.currency=? AND e.status IN ('active','suspended') AND p.status='active')", [engagementId, companyId, currency]); },
        guard91(key: string, value: "no-pending-advance", engagementId: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_payroll_advances WHERE engagement_id=? AND status='pending')", [engagementId]); },
        stageRequestPayrollAdvanceEffect(id: string, companyId: string, id2: string, person_id: string, amountMinor: number, installmentCount: number, firstDeductionPeriod: string, currency: string, reason: string, userId: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_payroll_advances(id,company_id,engagement_id,person_id,amount_minor,installment_count,first_deduction_period,currency,reason,requested_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(id, companyId, id2, person_id, amountMinor, installmentCount, firstDeductionPeriod, currency, reason, userId, now, now2); },
        stageRequestPayrollAdvanceEffect2(value: string, id: string, companyId: string, userId: string, value2: 1, value3: string, now: number) { return database.prepare("INSERT INTO hr_payroll_advance_events(id,advance_id,company_id,action,actor_id,advance_version,evidence_json,created_at) VALUES(?,?,?,'requested',?,?,?,?)").bind(value, id, companyId, userId, value2, value3, now); },
        stageRequestPayrollAdvanceEffect3(...args) { return payrollIdentityFacts(database).stageRequestPayrollAdvanceEffect3(...args); },
        readDecidePayrollAdvanceRow(advanceId: string, companyId: string) { return database.prepare("SELECT a.*,p.user_id FROM hr_payroll_advances a JOIN hr_people p ON p.id=a.person_id WHERE a.id=? AND a.company_id=?").bind(advanceId, companyId).first<Record<string, string | number | null>>(); },
        guard96(key: string, value: "advance-version", advanceId: string, companyId: string, expectedVersion: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_advances WHERE id=? AND company_id=? AND status='pending' AND version=?)", [advanceId, companyId, expectedVersion]); },
        stageDecidePayrollAdvanceStatements(status: "approved" | "rejected" | "cancelled", status2: "approved" | "rejected" | "cancelled", userId: string, reason: string, now: number, now2: number, advanceId: string) { return database.prepare("UPDATE hr_payroll_advances SET status=?,version=version+1,approved_by=CASE WHEN ?='approved' THEN ? ELSE approved_by END,review_reason=?,reviewed_at=?,updated_at=? WHERE id=?").bind(status, status2, userId, reason, now, now2, advanceId); },
        stageDecidePayrollAdvanceStatements2(value: string, advanceId: string, companyId: string, value2: "approved" | "rejected" | "cancelled", userId: string, version: number, value3: string, now: number) { return database.prepare("INSERT INTO hr_payroll_advance_events(id,advance_id,company_id,action,actor_id,advance_version,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(value, advanceId, companyId, value2, userId, version, value3, now); },
        ...payrollDecisionNotificationEffects(()=>database),
        stageDecidePayrollAdvanceEffect(value: string, advanceId: string, companyId: string, value2: number, value3: string, value4: string, value5: number, now: number) { return database.prepare("INSERT INTO hr_payroll_advance_repayments(id,advance_id,company_id,sequence,due_period_start,due_period_end,amount_minor,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(value, advanceId, companyId, value2, value3, value4, value5, now); },
        readPayrollAdvancePageRows(value: string[], privileged: boolean) { return database.prepare(`SELECT a.*,p.legal_name,e.job_title,e.currency,e.pay_group_id FROM hr_payroll_advances a JOIN hr_people p ON p.id=a.person_id JOIN hr_engagements e ON e.id=a.engagement_id WHERE a.company_id=? AND (${privileged ? "TRUE" : "p.user_id=?"}) ORDER BY a.created_at DESC,a.id DESC`).bind(...value).all(); },
        readPayrollAdvancePageOptions(value: string[], privileged: boolean) { return database.prepare(`SELECT e.id engagement_id,p.legal_name,e.currency,e.pay_group_id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.status IN ('active','suspended') AND e.pay_group_id IS NOT NULL AND p.status='active' AND ${privileged ? "TRUE" : "p.user_id=?"} ORDER BY p.legal_name,e.id`).bind(...value).all(); },
        readPayrollAdvanceAlertTargetEffect(...args) { return payrollIdentityFacts(database).readPayrollAdvanceAlertTargetEffect(...args); },
        readCalculatePayrollRunRun(runId: string, companyId: string) {
            return database.prepare("SELECT * FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId, companyId).first<{
                id: string;
                company_id: string;
                pay_group_id: string;
                period_start: string;
                period_end: string;
                pay_date: string;
                country_pack_id: string | null;
                currency: string;
                mode: string;
                status: string;
                version: number;
                adjustment_of_run_id: string | null;
            }>();
        },
        readCalculatePayrollRunEngagements(companyId: string, pay_group_id: string, currency: string, period_end: string, period_start: string, value: unknown | null, value2: unknown | null) {
            return database.prepare("SELECT e.id,e.version,e.person_id,e.worker_type,e.employment_type,e.start_date,e.end_date,e.job_title,e.branch_id,e.department_id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.pay_group_id=? AND e.currency=? AND e.worker_type='employee' AND e.status IN ('active','ended') AND p.status='active' AND p.merged_into_id IS NULL AND e.start_date<=? AND (e.end_date IS NULL OR e.end_date>=?) AND (?::text IS NULL OR e.id=?) ORDER BY e.id").bind(companyId, pay_group_id, currency, period_end, period_start, value, value2).all<{
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
        readCalculatePayrollRunGroup(pay_group_id: string, companyId: string) {
            return database.prepare("SELECT overtime_multiplier_bps,proration_method FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active'").bind(pay_group_id, companyId).first<{
                overtime_multiplier_bps: number;
                proration_method: "calendar_days" | "none";
            }>();
        },
        guard107(key: string, value: "run", runId: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND version=? AND status IN ('draft','calculating','preview','failed'))", [runId, version]); },
        guard108(key: string, value: string, engagementId: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.id=? AND e.version=? AND e.status IN ('active','ended') AND p.status='active')", [engagementId, version]); },
        guard109(key: string, value: string, engagementId: string, period_start: string, period_end: string, length: number) { return assertion(database, key, value, "(SELECT COUNT(*) FROM hr_time_entries WHERE engagement_id=? AND work_date BETWEEN ? AND ? AND status='approved')=?", [engagementId, period_start, period_end, length]); },
        guard110(key: string, value: string, engagementId: string, period_start: string, period_end: string, length: number) { return assertion(database, key, value, "(SELECT COUNT(*) FROM overtime_payroll_handoffs WHERE engagement_id=? AND work_date BETWEEN ? AND ?)=?", [engagementId, period_start, period_end, length]); },
        guard111(key: string, value: string, id: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_time_entries WHERE id=? AND version=? AND status='approved')", [id, version]); },
        guard112(key: string, value: string, id: string, version: number, engagementId: string, companyId: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_advance_repayments r JOIN hr_payroll_advances a ON a.id=r.advance_id WHERE r.id=? AND r.version=? AND r.status IN ('open','reserved') AND a.status='approved' AND a.engagement_id=? AND a.company_id=?)", [id, version, engagementId, companyId]); },
        guard113(key: string, value: string, id: string | number | null, version: string | number | null) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE id=? AND version=? AND status IN ('active','approved'))", [id, version]); },
        stageCalculatePayrollRunEffect(runId: string) { return database.prepare("DELETE FROM hr_payroll_items WHERE run_id=?").bind(runId); },
        stageCalculatePayrollRunEffect2(value: string, digest: string, now: number, runId: string) { return database.prepare("UPDATE hr_payroll_runs SET status='preview',revision=revision+1,totals_json=?,input_digest=?,version=version+1,updated_at=?,error_message=NULL WHERE id=?").bind(value, digest, now, runId); },
        stageCalculatePayrollRunEffect3(value: string, runId: string, engagementId: string, workerType: string, value2: string, value3: string, gross: number, deductions: number, employer: number, net: number, currency: string, now: number) { return database.prepare("INSERT INTO hr_payroll_items(id,run_id,engagement_id,worker_type,input_snapshot_json,result_json,gross_minor,deductions_minor,employer_cost_minor,net_minor,currency,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(value, runId, engagementId, workerType, value2, value3, gross, deductions, employer, net, currency, now); },
        readWorkPayrollRowsEffect(companyId: string) { return database.prepare("SELECT r.* FROM hr_payroll_runs r JOIN hr_pay_groups g ON g.id=r.pay_group_id WHERE r.company_id=? ORDER BY r.updated_at DESC").bind(companyId).all<Record<string, string | number | null>>(); },
        readGetPayrollRunRun(runId: string, companyId: string) { return database.prepare("SELECT r.*,g.overtime_multiplier_bps,g.proration_method FROM hr_payroll_runs r JOIN hr_pay_groups g ON g.id=r.pay_group_id WHERE r.id=? AND r.company_id=?").bind(runId, companyId).first(); },
        readGetPayrollRunItemsexportscorrectionspaymentBatch(runId: string) { return database.prepare("SELECT i.*,p.legal_name,p.id person_id FROM hr_payroll_items i JOIN hr_engagements e ON e.id=i.engagement_id JOIN hr_people p ON p.id=e.person_id WHERE i.run_id=? ORDER BY p.legal_name").bind(runId).all(); },
        readGetPayrollRunItemsexportscorrectionspaymentBatch2(runId: string) { return database.prepare("SELECT id,run_revision,checksum_sha256,created_at FROM hr_payroll_review_exports WHERE run_id=? ORDER BY run_revision DESC").bind(runId).all(); },
        readGetPayrollRunItemsexportscorrectionspaymentBatch3(runId: string, companyId: string) { return database.prepare("SELECT * FROM hr_payroll_corrections WHERE run_id=? AND company_id=? ORDER BY created_at DESC,id DESC").bind(runId, companyId).all(); },
        readGetPayrollRunItemsexportscorrectionspaymentBatch4(companyId: string, runId: string) { return database.prepare("SELECT * FROM hr_payment_batches WHERE company_id=? AND source_type='payroll' AND source_id=? AND status<>'cancelled' ORDER BY created_at DESC,id DESC LIMIT 1").bind(companyId, runId).first(); },
        readGetPayrollRunPaymentItems(value: string) { return database.prepare("SELECT * FROM hr_payment_batch_items WHERE batch_id=? ORDER BY reference").bind(value).all(); },
        readListPayrollRunsEffect(companyId: string) { return database.prepare("SELECT r.id,r.period_start,r.period_end,r.pay_date,r.currency,r.status,r.version,b.id payment_batch_id,b.status payment_status,b.total_minor net_payable_minor FROM hr_payroll_runs r LEFT JOIN hr_payment_batches b ON b.source_type='payroll' AND b.source_id=r.id AND b.company_id=r.company_id AND b.status<>'cancelled' WHERE r.company_id=? AND r.status='posted' ORDER BY r.pay_date DESC,r.created_at DESC").bind(companyId).all(); },
        readListPayrollRunsEffect2(companyId: string) { return database.prepare("SELECT * FROM hr_payroll_runs WHERE company_id=? ORDER BY pay_date DESC,created_at DESC").bind(companyId).all(); },
        readPayrollPostingOptionsChartsperiodsmappingsgroups(...args) { return payrollFinanceFacts(database).readPayrollPostingOptionsChartsperiodsmappingsgroups(...args); },
        readPayrollPostingOptionsChartsperiodsmappingsgroups2(...args) { return payrollFinanceFacts(database).readPayrollPostingOptionsChartsperiodsmappingsgroups2(...args); },
        readPayrollPostingOptionsChartsperiodsmappingsgroups3(companyId: string) { return database.prepare("SELECT id,pay_group_id,chart_id,accounts_json,status,version,created_by,approved_by FROM hr_payroll_account_mappings WHERE company_id=? AND status IN ('draft','approved') ORDER BY pay_group_id,created_at DESC").bind(companyId).all(); },
        readPayrollPostingOptionsChartsperiodsmappingsgroups4(companyId: string) { return database.prepare("SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=? AND status='active' ORDER BY name").bind(companyId).all(); },
        readPayrollPostingOptionsAccounts(...args) { return payrollFinanceFacts(database).readPayrollPostingOptionsAccounts(...args); },
        readResolvePayrollPostRun(runId: string, companyId: string) {
            return database.prepare("SELECT pay_group_id,period_end FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId, companyId).first<{
                pay_group_id: string;
                period_end: string;
            }>();
        },
        readResolvePayrollPostMapping(value: string[], input: {
            expectedVersion: number;
            chartId?: string | undefined;
            periodId?: string | undefined;
            entryDate?: string | undefined;
            accountMappingId?: string | undefined;
            expenseAccountId?: string | undefined;
            netPayableAccountId?: string | undefined;
            taxPayableAccountId?: string | undefined;
            employerContributionAccountId?: string | undefined;
            employerExpenseAccountId?: string | undefined;
            socialSecurityPayableAccountId?: string | undefined;
            advanceReceivableAccountId?: string | undefined;
            benefitPayableAccountId?: string | undefined;
            otherDeductionPayableAccountId?: string | undefined;
            serviceChargeExpenseAccountId?: string | undefined;
            overtimeExpenseAccountId?: string | undefined;
            bonusExpenseAccountId?: string | undefined;
        }) {
            return database.prepare(`SELECT id,chart_id,accounts_json,version FROM hr_payroll_account_mappings WHERE company_id=? AND pay_group_id=? AND status='approved' ${input.accountMappingId ? "AND id=?" : ""} ORDER BY created_at DESC LIMIT 1`).bind(...value).first<{
                id: string;
                chart_id: string;
                accounts_json: string;
                version: number;
            }>();
        },
        readResolvePayrollPostPeriods(...args) { return payrollFinanceFacts(database).readResolvePayrollPostPeriods(...args); },
        readResolvePayrollPostNext(...args) { return payrollFinanceFacts(database).readResolvePayrollPostNext(...args); },
        readResolvePayrollPostEffect(...args) { return payrollFinanceFacts(database).readResolvePayrollPostEffect(...args); },
        readSavePayrollAccountMappingGroup(payGroupId: string, companyId: string) {
            return database.prepare("SELECT id,currency FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active'").bind(payGroupId, companyId).first<{
                id: string;
                currency: string;
            }>();
        },
        readSavePayrollAccountMappingChart(...args) { return payrollFinanceFacts(database).readSavePayrollAccountMappingChart(...args); },
        readSavePayrollAccountMappingAccounts(...args) { return payrollFinanceFacts(database).readSavePayrollAccountMappingAccounts(...args); },
        stageSavePayrollAccountMappingEffect(id: string, companyId: string, payGroupId: string, chartId: string, value: string, userId: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_payroll_account_mappings(id,company_id,pay_group_id,chart_id,accounts_json,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'draft',?,?,?)").bind(id, companyId, payGroupId, chartId, value, userId, now, now2); },
        readApprovePayrollAccountMappingRow(mappingId: string, companyId: string) {
            return database.prepare("SELECT id,pay_group_id,created_by,status,version FROM hr_payroll_account_mappings WHERE id=? AND company_id=?").bind(mappingId, companyId).first<{
                id: string;
                pay_group_id: string;
                created_by: string;
                status: string;
                version: number;
            }>();
        },
        guard141(...args) { return payrollIdentityFacts(database).guard141(...args); },
        stageApprovePayrollAccountMappingEffect(now: number, companyId: string, pay_group_id: string) { return database.prepare("UPDATE hr_payroll_account_mappings SET status='superseded',version=version+1,updated_at=? WHERE company_id=? AND pay_group_id=? AND status='approved'").bind(now, companyId, pay_group_id); },
        stageApprovePayrollAccountMappingEffect2(userId: string, now: number, mappingId: string) { return database.prepare("UPDATE hr_payroll_account_mappings SET status='approved',approved_by=?,version=version+1,updated_at=? WHERE id=?").bind(userId, now, mappingId); },
        readPreparePayrollAccrualRun(runId: string, companyId: string) {
            return database.prepare(`SELECT r.*, COUNT(i.id) item_count,
      COUNT(i.id) FILTER (WHERE i.status NOT IN ('calculated','approved','posted')) invalid_items
    FROM hr_payroll_runs r LEFT JOIN hr_payroll_items i ON i.run_id=r.id
    WHERE r.id=? AND r.company_id=? GROUP BY r.id`).bind(runId, companyId).first<Record<string, string | number | null>>();
        },
        readPreparePayrollAccrualItems(runId: string) {
            return database.prepare("SELECT id,worker_type,gross_minor,deductions_minor,employer_cost_minor,net_minor,input_snapshot_json,result_json FROM hr_payroll_items WHERE run_id=? ORDER BY id").bind(runId).all<PayrollPostingItem & {
                worker_type: string;
            }>();
        },
        readPreparePayrollAccrualValid(...args) { return payrollFinanceFacts(database).readPreparePayrollAccrualValid(...args); },
        guard147(key: string, value: "payroll-run", runId: string, companyId: string, expectedVersion: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='approved' AND version=? AND input_digest IS NOT NULL)", [runId, companyId, expectedVersion]); },
        guard148(key: string, value: "payroll-mapping", resolvedMappingId: string, companyId: string, runId: string, resolvedMappingVersion: number | null) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_account_mappings WHERE id=? AND company_id=? AND pay_group_id=(SELECT pay_group_id FROM hr_payroll_runs WHERE id=?) AND status='approved' AND version=?)", [resolvedMappingId, companyId, runId, resolvedMappingVersion]); },
        guard149(key: string, value: "payroll-items", runId: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_payroll_items WHERE run_id=? AND status NOT IN ('calculated','approved','posted'))", [runId]); },
        guard150(key: string, value: "payroll-posting-unique", runId: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_payroll_postings WHERE run_id=?)", [runId]); },
        stagePostPayrollEffect(userId: string, now: number, runId: string, companyId: string, expectedVersion: number) { return database.prepare("UPDATE hr_payroll_runs SET status='posted',posted_by=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND status='approved' AND version=?").bind(userId, now, runId, companyId, expectedVersion); },
        stagePostPayrollEffect2(runId: string) { return database.prepare("UPDATE hr_payroll_items SET status='posted' WHERE run_id=? AND status IN ('calculated','approved')").bind(runId); },
        stagePostPayrollEffect3(value: string, runId: string, id: string, resolvedMappingId: string | null, resolvedMappingVersion: number | null, now: number) { return database.prepare("INSERT INTO hr_payroll_postings(id,run_id,journal_id,account_mapping_id,account_mapping_version,created_at) VALUES(?,?,?,?,?,?)").bind(value, runId, id, resolvedMappingId, resolvedMappingVersion, now); },
        readApprovePayrollRunSubmitted(runId: string, companyId: string) {
            return database.prepare("SELECT submitted_by FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId, companyId).first<{
                submitted_by: string | null;
            }>();
        },
        guard155(...args) { return payrollIdentityFacts(database).guard155(...args); },
        stageApprovePayrollRunEffect(userId: string, now: number, runId: string) { return database.prepare("UPDATE hr_payroll_runs SET status='approved',approved_by=?,version=version+1,updated_at=? WHERE id=?").bind(userId, now, runId); },
        stageApprovePayrollRunEffect2(...args) { return payrollWorkflowFacts(database).stageApprovePayrollRunEffect2(...args); },
        readRequestPayrollCorrectionRun(runId: string, companyId: string) {
            return database.prepare("SELECT id,status,version,revision,input_digest,totals_json FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId, companyId).first<{
                id: string;
                status: string;
                version: number;
                revision: number;
                input_digest: string | null;
                totals_json: string;
            }>();
        },
        guard159(key: string, value: "approved-run", runId: string, companyId: string, expectedVersion: number, revision: number, input_digest: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='approved' AND version=? AND revision=? AND input_digest=?)", [runId, companyId, expectedVersion, revision, input_digest]); },
        stageRequestPayrollCorrectionEffect(id: string, runId: string, companyId: string, version: number, revision: number, input_digest: string, totals_json: string, reason: string, userId: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_payroll_corrections(id,run_id,company_id,run_version,run_revision,input_digest,totals_json,reason,requested_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(id, runId, companyId, version, revision, input_digest, totals_json, reason, userId, now, now2); },
        stageRequestPayrollCorrectionEffect2(value: string, id: string, runId: string, companyId: string, userId: string, version: number, value2: string, now: number) { return database.prepare("INSERT INTO hr_payroll_correction_events(id,correction_id,run_id,company_id,action,actor_id,run_version,evidence_json,created_at) VALUES(?,?,?,?, 'requested',?,?,?,?)").bind(value, id, runId, companyId, userId, version, value2, now); },
        stageRequestPayrollCorrectionEffect3(...args) { return payrollIdentityFacts(database).stageRequestPayrollCorrectionEffect3(...args); },
        readDecidePayrollCorrectionRow(correctionId: string, companyId: string) { return database.prepare("SELECT c.*,r.status run_status,r.version current_run_version,r.revision current_run_revision,r.input_digest current_input_digest FROM hr_payroll_corrections c JOIN hr_payroll_runs r ON r.id=c.run_id WHERE c.id=? AND c.company_id=?").bind(correctionId, companyId).first<Record<string, string | number | null>>(); },
        guard164(key: string, value: "correction-version", correctionId: string, companyId: string, expectedVersion: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_corrections WHERE id=? AND company_id=? AND status='pending' AND version=?)", [correctionId, companyId, expectedVersion]); },
        guard165(key: string, value: "run-evidence", run_id: string | number | null, companyId: string, run_version: string | number | null, run_revision: string | number | null, input_digest: string | number | null) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='approved' AND version=? AND revision=? AND input_digest=?)", [run_id, companyId, run_version, run_revision, input_digest]); },
        stageDecidePayrollCorrectionStatements(status: "approved" | "rejected" | "cancelled", userId: string, reason: string, now: number, now2: number, correctionId: string) { return database.prepare("UPDATE hr_payroll_corrections SET status=?,version=version+1,reviewed_by=?,review_reason=?,reviewed_at=?,updated_at=? WHERE id=?").bind(status, userId, reason, now, now2, correctionId); },
        stageDecidePayrollCorrectionStatements2(value: string, correctionId: string, run_id: string | number | null, companyId: string, status: "approved" | "rejected" | "cancelled", userId: string, run_version: string | number | null, value2: string, now: number) { return database.prepare("INSERT INTO hr_payroll_correction_events(id,correction_id,run_id,company_id,action,actor_id,run_version,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(value, correctionId, run_id, companyId, status, userId, run_version, value2, now); },
        
        stageDecidePayrollCorrectionEffect(value: string, now: number, run_id: string | number | null) { return database.prepare("UPDATE hr_payroll_runs SET status='calculating',approved_by=NULL,version=version+1,error_message=?,updated_at=? WHERE id=?").bind(value, now, run_id); },
        readPayrollCorrectionAlertTargetEffect(...args) { return payrollIdentityFacts(database).readPayrollCorrectionAlertTargetEffect(...args); },
        readExportPayrollReviewRun(runId: string, companyId: string) {
            return database.prepare("SELECT id,status,revision,input_digest,currency,period_start,period_end FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(runId, companyId).first<{
                id: string;
                status: string;
                revision: number;
                input_digest: string | null;
                currency: string;
                period_start: string;
                period_end: string;
            }>();
        },
        readExportPayrollReviewExisting(runId: string, revision: number) {
            return database.prepare("SELECT checksum_sha256 FROM hr_payroll_review_exports WHERE run_id=? AND run_revision=?").bind(runId, revision).first<{
                checksum_sha256: string;
            }>();
        },
        readExportPayrollReviewItems(runId: string) {
            return database.prepare("SELECT p.legal_name,i.worker_type,i.gross_minor,i.deductions_minor,i.employer_cost_minor,i.net_minor,i.currency,i.input_snapshot_json FROM hr_payroll_items i JOIN hr_engagements e ON e.id=i.engagement_id JOIN hr_people p ON p.id=e.person_id WHERE i.run_id=? ORDER BY p.legal_name").bind(runId).all<{
                legal_name: string;
                worker_type: string;
                gross_minor: number;
                deductions_minor: number;
                employer_cost_minor: number;
                net_minor: number;
                currency: string;
                input_snapshot_json: string;
            }>();
        },
        guard174(key: string, value: "run", runId: string, companyId: string, revision: number, input_digest: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND revision=? AND input_digest=? AND status IN ('preview','in_review','approved'))", [runId, companyId, revision, input_digest]); },
        stageExportPayrollReviewEffect(exportId: string, runId: string, revision: number, fileKey: string, checksum: string, userId: string, now: number) { return database.prepare("INSERT INTO hr_payroll_review_exports(id,run_id,run_revision,file_key,checksum_sha256,created_by,created_at) VALUES(?,?,?,?,?,?,?)").bind(exportId, runId, revision, fileKey, checksum, userId, now); },
        readGetPayrollReviewFileRow(runId: string, revision: number, companyId: string) {
            return database.prepare("SELECT e.file_key,e.checksum_sha256 FROM hr_payroll_review_exports e JOIN hr_payroll_runs r ON r.id=e.run_id WHERE e.run_id=? AND e.run_revision=? AND r.company_id=?").bind(runId, revision, companyId).first<{
                file_key: string;
                checksum_sha256: string;
            }>();
        },
        readCreatePaymentBatchDuplicate(companyId: string, sourceType: "contractor" | "payroll" | "expense", sourceId: string) { return database.prepare("SELECT id FROM hr_payment_batches WHERE company_id=? AND source_type=? AND source_id=? AND status NOT IN ('cancelled') LIMIT 1").bind(companyId, sourceType, sourceId).first(); },
        readCreatePaymentBatchSource(sourceId: string, companyId: string) {
            return database.prepare("SELECT id,status,currency FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(sourceId, companyId).first<{
                id: string;
                status: string;
                currency: string;
            }>();
        },
        readCreatePaymentBatchPosted(sourceId: string) {
            return database.prepare("SELECT id,engagement_id,net_minor,currency,input_snapshot_json FROM hr_payroll_items WHERE run_id=? AND net_minor>0 AND status='posted' ORDER BY engagement_id").bind(sourceId).all<{
                id: string;
                engagement_id: string;
                net_minor: number;
                currency: string;
                input_snapshot_json: string;
            }>();
        },
        readCreatePaymentBatchInvoice(...args) { return payrollFinanceFacts(database).readCreatePaymentBatchInvoice(...args); },
        readCreatePaymentBatchBank(...args) { return payrollFinanceFacts(database).readCreatePaymentBatchBank(...args); },
        guard182(commandId: string, value: "posted-payroll-source", sourceId: string, companyId: string) { return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='posted')", [sourceId, companyId]); },
        guard183(commandId: string, value: string, id: string, sourceId: string, net_minor: number, input_snapshot_json: string) { return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM hr_payroll_items WHERE id=? AND run_id=? AND status='posted' AND net_minor=? AND input_snapshot_json=?)", [id, sourceId, net_minor, input_snapshot_json]); },
        stageCreatePaymentBatchStatements(batchId: string, companyId: string, sourceType: "contractor" | "payroll", sourceId: string, currency: string, value: "draft", formatCode: string, total: number, value2: string | null, userId: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_payment_batches(id,company_id,source_type,source_id,currency,status,format_code,total_minor,bank_account_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(batchId, companyId, sourceType, sourceId, currency, value, formatCode, total, value2, userId, now, now2); },
        guard185(...args) { return payrollFinanceFacts(database).guard185(...args); },
        guard186(commandId: string, value: "contractor-batch-unique", companyId: string, sourceId: string, batchId: string) { return assertion(database, commandId, value, "NOT EXISTS(SELECT 1 FROM hr_payment_batches WHERE company_id=? AND source_type='contractor' AND source_id=? AND status<>'cancelled' AND id<>?)", [companyId, sourceId, batchId]); },
        stageCreatePaymentBatchEffect(...args) { return payrollFinanceFacts(database).stageCreatePaymentBatchEffect(...args); },
        stageCreatePaymentBatchEffect2(value: string, batchId: string, engagement_id: string | null, person_id: string | null, value2: string, value3: string, net_minor: number, currency: string) { return database.prepare("INSERT INTO hr_payment_batch_items(id,batch_id,engagement_id,person_id,reference,payee_name,amount_minor,currency) VALUES(?,?,?,?,?,?,?,?)").bind(value, batchId, engagement_id, person_id, value2, value3, net_minor, currency); },
        readPreparePaymentRetryBatch(batchId: string, companyId: string) {
            return database.prepare("SELECT source_type,source_id,status,version FROM hr_payment_batches WHERE id=? AND company_id=?").bind(batchId, companyId).first<{
                source_type: string;
                source_id: string;
                status: string;
                version: number;
            }>();
        },
        readPreparePaymentRetryFailed(batchId: string) {
            return database.prepare("SELECT COUNT(*) count FROM hr_payment_batch_items WHERE batch_id=? AND status='failed'").bind(batchId).first<{
                count: number;
            }>();
        },
        guard191(commandId: string, value: "batch-retry-version", batchId: string, companyId: string, expectedVersion: number) { return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM hr_payment_batches WHERE id=? AND company_id=? AND source_type='payroll' AND status='partially_settled' AND version=?)", [batchId, companyId, expectedVersion]); },
        guard192(commandId: string, value: "batch-retry-items", batchId: string) { return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM hr_payment_batch_items WHERE batch_id=? AND status='failed')", [batchId]); },
        stagePreparePaymentRetryEffect(userId: string, now: number, batchId: string, companyId: string, expectedVersion: number) { return database.prepare("UPDATE hr_payment_batches SET status='draft',retry_requested_by=?,approved_by=NULL,version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?").bind(userId, now, batchId, companyId, expectedVersion); },
        readApprovePaymentBatchBatch(batchId: string, companyId: string) {
            return database.prepare("SELECT status,version,source_type,created_by,retry_requested_by FROM hr_payment_batches WHERE id=? AND company_id=?").bind(batchId, companyId).first<{
                status: string;
                version: number;
                source_type: string;
                created_by: string;
                retry_requested_by: string | null;
            }>();
        },
        guard195(...args) { return payrollIdentityFacts(database).guard195(...args); },
        stageApprovePaymentBatchEffect(userId: string, now: number, batchId: string, companyId: string, expectedVersion: number) { return database.prepare("UPDATE hr_payment_batches SET status='approved',approved_by=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND status='draft' AND version=?").bind(userId, now, batchId, companyId, expectedVersion); },
        stageApprovePaymentBatchEffect2(...args) { return payrollWorkflowFacts(database).stageApprovePaymentBatchEffect2(...args); },
        readExportPaymentBatchV2Batch(batchId: string, companyId: string) {
            return database.prepare("SELECT id,status,version,source_type,source_id,format_code,retry_requested_by FROM hr_payment_batches WHERE id=? AND company_id=?").bind(batchId, companyId).first<{
                id: string;
                status: string;
                version: number;
                source_type: string;
                source_id: string;
                format_code: string;
                retry_requested_by: string | null;
            }>();
        },
        readExportPaymentBatchV2Items(batchId: string, eligibleStatus: "failed" | "outstanding") {
            return database.prepare("SELECT id,reference,person_id,payee_name,amount_minor,currency,status FROM hr_payment_batch_items WHERE batch_id=? AND status=? ORDER BY reference").bind(batchId, eligibleStatus).all<{
                id: string;
                reference: string;
                person_id: string | null;
                payee_name: string;
                amount_minor: number;
                currency: string;
                status: string;
            }>();
        },
        readExportPaymentBatchV2Posted(source_id: string, reference: string, source_id2: string) {
            return database.prepare("SELECT id,engagement_id,net_minor,currency,input_snapshot_json FROM hr_payroll_items WHERE run_id=? AND status='posted' AND ?=('PAY-'||?||'-'||id)").bind(source_id, reference, source_id2).first<{
                id: string;
                engagement_id: string;
                net_minor: number;
                currency: string;
                input_snapshot_json: string;
            }>();
        },
        guard201(commandId: string, value: string, source_id: string, id: string, id2: string, batchId: string, eligibleStatus: "failed" | "outstanding", reference: string, person_id: string, payee_name: string, amount_minor: number, currency: string, net_minor: number, currency2: string, input_snapshot_json: string) { return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM hr_payment_batch_items b JOIN hr_payroll_items p ON p.run_id=? AND p.id=? WHERE b.id=? AND b.batch_id=? AND b.status=? AND b.reference=? AND b.person_id=? AND b.payee_name=? AND b.amount_minor=? AND b.currency=? AND p.status='posted' AND p.net_minor=? AND p.currency=? AND p.input_snapshot_json=?)", [source_id, id, id2, batchId, eligibleStatus, reference, person_id, payee_name, amount_minor, currency, net_minor, currency2, input_snapshot_json]); },
        guard202(commandId: string, value: "batch-export-version", batchId: string, companyId: string, version: number) { return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM hr_payment_batches WHERE id=? AND company_id=? AND status='approved' AND version=?)", [batchId, companyId, version]); },
        guard203(commandId: string, value: "batch-export-items", batchId: string, eligibleStatus: "failed" | "outstanding", length: number) { return assertion(database, commandId, value, "(SELECT COUNT(*) FROM hr_payment_batch_items WHERE batch_id=? AND status=?)=?", [batchId, eligibleStatus, length]); },
        guard204(commandId: string, value: "posted-payroll-source", source_id: string, companyId: string) { return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='posted')", [source_id, companyId]); },
        stageExportPaymentBatchV2Effect(value: string, companyId: string, batchId: string, version: number, fileKey: string, checksum: string, length: number, total: number, userId: string, now: number) { return database.prepare("INSERT INTO hr_payment_batch_exports(id,company_id,batch_id,batch_version,file_key,checksum_sha256,item_count,total_minor,exported_by,exported_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(value, companyId, batchId, version, fileKey, checksum, length, total, userId, now); },
        stageExportPaymentBatchV2Effect2(fileKey: string, checksum: string, now: number, now2: number, batchId: string, companyId: string, version: number) { return database.prepare("UPDATE hr_payment_batches SET status='exported',file_key=?,file_checksum=?,exported_at=?,version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?").bind(fileKey, checksum, now, now2, batchId, companyId, version); },
        readConfirmPaymentBatchBatch(batchId: string, companyId: string) {
            return database.prepare("SELECT id,status,version,source_type,source_id,currency,bank_account_id FROM hr_payment_batches WHERE id=? AND company_id=?").bind(batchId, companyId).first<{
                id: string;
                status: string;
                version: number;
                source_type: string;
                source_id: string;
                currency: string;
                bank_account_id: string | null;
            }>();
        },
        readConfirmPaymentBatchItems(batchId: string) {
            return database.prepare("SELECT id,reference,status,amount_minor,engagement_id FROM hr_payment_batch_items WHERE batch_id=?").bind(batchId).all<{
                id: string;
                reference: string;
                status: string;
                amount_minor: number;
                engagement_id: string | null;
            }>();
        },
        guard209(commandId: string, value: "payment-batch-version", batchId: string, companyId: string, version: number, status: string) { return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM hr_payment_batches WHERE id=? AND company_id=? AND version=? AND status=?)", [batchId, companyId, version, status]); },
        readConfirmPaymentBatchPosting(...args) { return payrollFinanceFacts(database).readConfirmPaymentBatchPosting(...args); },
        readConfirmPaymentBatchPayables(...args) { return payrollFinanceFacts(database).readConfirmPaymentBatchPayables(...args); },
        readConfirmPaymentBatchBankAccount(...args) { return payrollFinanceFacts(database).readConfirmPaymentBatchBankAccount(...args); },
        readConfirmPaymentBatchTransaction(...args) { return payrollFinanceFacts(database).readConfirmPaymentBatchTransaction(...args); },
        readConfirmPaymentBatchPeriods(...args) { return payrollFinanceFacts(database).readConfirmPaymentBatchPeriods(...args); },
        readConfirmPaymentBatchPayrollItem(source_id: string, engagement_id: string | null, amount_minor: number) { return database.prepare("SELECT id,gross_minor,deductions_minor,employer_cost_minor,net_minor,input_snapshot_json,result_json FROM hr_payroll_items WHERE run_id=? AND engagement_id=? AND net_minor=?").bind(source_id, engagement_id, amount_minor).first<PayrollPostingItem>(); },
        guard216(...args) { return payrollFinanceFacts(database).guard216(...args); },
        stageConfirmPaymentBatchEffect(value: string, companyId: string, batchId: string, id: string, id2: string, id3: string, userId: string, now: number) { return database.prepare("INSERT INTO hr_payroll_payment_settlements(id,company_id,batch_id,batch_item_id,bank_transaction_id,journal_id,confirmed_by,confirmed_at) VALUES(?,?,?,?,?,?,?,?)").bind(value, companyId, batchId, id, id2, id3, userId, now); },
        stageConfirmPaymentBatchEffect2(...args) { return payrollFinanceFacts(database).stageConfirmPaymentBatchEffect2(...args); },
        stageConfirmPaymentBatchEffect3(value: "failed" | "settled", value2: string | null, value3: number | null, batchId: string, reference: string) { return database.prepare("UPDATE hr_payment_batch_items SET status=?,confirmation_ref=?,settled_at=? WHERE batch_id=? AND reference=? AND status IN ('outstanding','failed')").bind(value, value2, value3, batchId, reference); },
        stageConfirmPaymentBatchEffect4(status: "partially_settled" | "settled", now: number, batchId: string, companyId: string) { return database.prepare("UPDATE hr_payment_batches SET status=?,version=version+1,updated_at=? WHERE id=? AND company_id=?").bind(status, now, batchId, companyId); },
        readGetPaymentFileBatch(batchId: string, companyId: string) {
            return database.prepare("SELECT file_key,file_checksum FROM hr_payment_batches WHERE id=? AND company_id=? AND status IN ('exported','partially_settled','settled')").bind(batchId, companyId).first<{
                file_key: string | null;
                file_checksum: string | null;
            }>();
        } };
    return { repository, pagedRuns:(companyId,cursor)=>readListPage<Record<string,string|number|null>&{id:string;page_key:string}>(database,{sql:"SELECT *,pay_date||':'||created_at::text page_key FROM hr_payroll_runs WHERE company_id=?",args:[companyId],cursor,descending:true,scope:[companyId,'payroll']}), clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, hash: sha256, authorize: requirePermission, prepare: async (holding) => { const resources = await holdingResources(holding); database = provided ?? resources.write; files = resources.files; }, commands: { replay: (...args) => replayCommand(database, ...args), commit: (...args) => executeCommand(database, ...args) }, environment: async () => (await runtimeEnv()).ERPX_ENVIRONMENT, simulationAdapterId: thaiHotelPayrollMockId, files: { put: (...args) => files.put(...args), get: (...args) => files.get(...args) }, organizationGuard: (...args) => fromNative(() => organizationGuard(database, ...args)), validateStaffOrganization: (...args) => fromNative(() => validateStaffOrganization(database, ...args)), prepareJournal: (...args) => fromNative(() => prepareJournal(database, ...args)), assertPayrollSourceFresh: (...args) => fromNative(() => assertPayrollSourceFresh(database, ...args)), payrollSourceGuards: (...args) => fromNative(() => payrollSourceGuards(database, ...args)), prepareAttachmentBinding: (...args) => fromNative(() => prepareAttachmentBinding(database, ...args)), snapshotMoney: (...args) => fromNative(() => snapshotMoney(database, ...args)), assertStatutoryRunReady: (...args) => fromNative(() => assertStatutoryRunReady(database, ...args)), thaiEvidenceForEmployee: (...args) => fromNative(() => thaiEvidenceForEmployee(database, ...args)), thaiEvidenceGuards: (...args) => thaiEvidenceGuards(database, ...args), thaiPackForRun: (...args) => fromNative(() => thaiPackForRun(database, ...args)), preparePayslipIssue: (...args) => fromNative(() => preparePayslipIssue(database, ...args)), calculatePayrollAdjustmentRun: (...args) => fromNative(() => calculatePayrollAdjustmentRun(database, ...args)), assertPayrollRecoveryDeductionLimits: (...args) => fromNative(() => assertPayrollRecoveryDeductionLimits(database, ...args)), preparePayrollDeductionConsumption: (...args) => fromNative(() => preparePayrollDeductionConsumption(database, ...args)), directoryPerson: (actor, id, engagementId) => fromNative(() => directoryPerson(actor, id, engagementId, database)), canReadCompensation: canReadCompensation, canManagePeople: canManagePeople, recognizedThaiPack: recognizedThaiPack, calculateThaiHotelPayrollFixture: calculateThaiHotelPayrollFixture, isThaiHotelPayrollMockEnabled: isThaiHotelPayrollMockEnabled };
}
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> {
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
export async function payrollCostAssignments(actor: WorkContext, engagementId: string) { return invoke(() => createPeoplePayrollApplication(context()).payrollCostAssignments(actor, engagementId)); }
export async function verifyPayrollCostAssignment(actor: WorkContext, assignmentId: string, input: z.infer<typeof payrollCostVerificationSchema>, commandId: string) { return invoke(() => createPeoplePayrollApplication(context()).verifyPayrollCostAssignment(actor, assignmentId, input, commandId)); }
export async function listPeople(actor: WorkContext, filters: {
    levelId?: string;
    departmentId?: string;
} = {}) { return invoke(() => createPeoplePayrollApplication(context()).listPeople(actor, filters)); }
export async function getPerson(actor: WorkContext, personId: string, engagementId?: string) { return invoke(() => createPeoplePayrollApplication(context()).getPerson(actor, personId, engagementId)); }
export async function peopleSummary(actor: WorkContext) { return invoke(() => createPeoplePayrollApplication(context()).peopleSummary(actor)); }
export async function payrollSummary(actor: WorkContext, payMonth?: string) { return invoke(() => createPeoplePayrollApplication(context()).payrollSummary(actor, payMonth)); }
export async function createPerson(actor: WorkContext, input: z.infer<typeof createPersonSchema>, id: string) { return invoke(() => createPeoplePayrollApplication(context()).createPerson(actor, input, id)); }
export async function createEngagement(actor: WorkContext, input: z.infer<typeof createEngagementSchema>, id: string) { return invoke(() => createPeoplePayrollApplication(context()).createEngagement(actor, input, id)); }
export async function createCompensation(actor: WorkContext, input: z.infer<typeof compensationSchema>, key: string) { return invoke(() => createPeoplePayrollApplication(context()).createCompensation(actor, input, key)); }
export async function payrollOptions(actor: WorkContext) { return invoke(() => createPeoplePayrollApplication(context()).payrollOptions(actor)); }
export async function assignPayGroup(actor: WorkContext, engagementId: string, input: z.infer<typeof assignPayGroupSchema>, key: string) { return invoke(() => createPeoplePayrollApplication(context()).assignPayGroup(actor, engagementId, input, key)); }
export async function transitionCompensation(actor: WorkContext, compensationId: string, input: z.infer<typeof compensationTransitionSchema>, key: string) { return invoke(() => createPeoplePayrollApplication(context()).transitionCompensation(actor, compensationId, input, key)); }
export async function transitionEngagement(actor: WorkContext, engagementId: string, input: z.infer<typeof engagementTransitionSchema>, key: string) { return invoke(() => createPeoplePayrollApplication(context()).transitionEngagement(actor, engagementId, input, key)); }
export async function createTimeEntry(actor: WorkContext, input: z.infer<typeof timeEntrySchema>, key: string) { return invoke(() => createPeoplePayrollApplication(context()).createTimeEntry(actor, input, key)); }
export async function updateTimeEntry(actor: WorkContext, entryId: string, input: z.infer<typeof timeEntryUpdateSchema>, key: string) { return invoke(() => createPeoplePayrollApplication(context()).updateTimeEntry(actor, entryId, input, key)); }
export async function rejectTimeEntry(actor: WorkContext, entryId: string, input: z.infer<typeof timeEntryReviewSchema>, key: string) { return invoke(() => createPeoplePayrollApplication(context()).rejectTimeEntry(actor, entryId, input, key)); }
export async function approveTimeEntry(actor: WorkContext, entryId: string, expectedVersion: number, key: string) { return invoke(() => createPeoplePayrollApplication(context()).approveTimeEntry(actor, entryId, expectedVersion, key)); }
export async function createPayGroup(actor: WorkContext, input: z.infer<typeof payGroupSchema>, id: string) { return invoke(() => createPeoplePayrollApplication(context()).createPayGroup(actor, input, id)); }
export async function createCountryPack(actor: WorkContext, input: z.infer<typeof countryPackSchema>, id: string) { return invoke(() => createPeoplePayrollApplication(context()).createCountryPack(actor, input, id)); }
export async function listCountryPacks(actor: WorkContext) { return invoke(() => createPeoplePayrollApplication(context()).listCountryPacks(actor)); }
export async function transitionCountryPack(actor: WorkContext, packId: string, input: z.infer<typeof countryPackTransitionSchema>, commandId: string) { return invoke(() => createPeoplePayrollApplication(context()).transitionCountryPack(actor, packId, input, commandId)); }
export async function createPayrollRun(actor: WorkContext, input: z.infer<typeof payrollRunSchema>, key: string) { return invoke(() => createPeoplePayrollApplication(context()).createPayrollRun(actor, input, key)); }
export async function prepareFinalPay(actor: WorkContext, input: z.infer<typeof finalPayPreparationSchema>, key: string) { return invoke(() => createPeoplePayrollApplication(context()).prepareFinalPay(actor, input, key)); }
export async function requestPayrollAdvance(actor: WorkContext, input: z.infer<typeof payrollAdvanceRequestSchema>, key: string) { return invoke(() => createPeoplePayrollApplication(context()).requestPayrollAdvance(actor, input, key)); }
export async function decidePayrollAdvance(actor: WorkContext, advanceId: string, input: z.infer<typeof payrollAdvanceDecisionSchema>, key: string) { return invoke(() => createPeoplePayrollApplication(context()).decidePayrollAdvance(actor, advanceId, input, key)); }
export async function payrollAdvancePage(actor: WorkContext) { return invoke(() => createPeoplePayrollApplication(context()).payrollAdvancePage(actor)); }
export async function payrollAdvanceAlertTarget(db: SqlDatabase, companyId: string, userId: string, advanceId: string, now = Date.now()) { return invoke(() => createPeoplePayrollApplication(context(db)).payrollAdvanceAlertTarget(companyId, userId, advanceId, now)); }
export async function calculatePayrollRun(actor: WorkContext, runId: string, key = crypto.randomUUID()) { return invoke(() => createPeoplePayrollApplication(context()).calculatePayrollRun(actor, runId, key)); }
export async function workPayrollRows(actor: WorkContext, provided?: SqlDatabase) { return invoke(() => createPeoplePayrollApplication(context(provided)).workPayrollRows(actor)); }
export async function getPayrollRun(actor: WorkContext, runId: string, provided?: SqlDatabase) { return invoke(() => createPeoplePayrollApplication(context(provided)).getPayrollRun(actor, runId)); }
export async function listPayrollRuns(actor: WorkContext) { return invoke(() => createPeoplePayrollApplication(context()).listPayrollRuns(actor)); }
export async function payrollPostingOptions(actor: WorkContext) { return invoke(() => createPeoplePayrollApplication(context()).payrollPostingOptions(actor)); }
export async function savePayrollAccountMapping(actor: WorkContext, payGroupId: string, input: z.infer<typeof payrollAccountMappingSchema>, key: string) { return invoke(() => createPeoplePayrollApplication(context()).savePayrollAccountMapping(actor, payGroupId, input, key)); }
export async function approvePayrollAccountMapping(actor: WorkContext, mappingId: string, expectedVersion: number, key: string) { return invoke(() => createPeoplePayrollApplication(context()).approvePayrollAccountMapping(actor, mappingId, expectedVersion, key)); }
export async function previewPayrollAccounting(actor: WorkContext, runId: string, input: z.infer<typeof payrollPostSchema>) { return invoke(() => createPeoplePayrollApplication(context()).previewPayrollAccounting(actor, runId, input)); }
export async function postPayroll(actor: WorkContext, runId: string, input: z.infer<typeof payrollPostSchema>, key: string) { return invoke(() => createPeoplePayrollApplication(context()).postPayroll(actor, runId, input, key)); }
export async function approvePayrollRun(actor: WorkContext, runId: string, expectedVersion: number, key: string) { return invoke(() => createPeoplePayrollApplication(context()).approvePayrollRun(actor, runId, expectedVersion, key)); }
export async function requestPayrollCorrection(actor: WorkContext, runId: string, input: z.infer<typeof payrollCorrectionRequestSchema>, key: string) { return invoke(() => createPeoplePayrollApplication(context()).requestPayrollCorrection(actor, runId, input, key)); }
export async function decidePayrollCorrection(actor: WorkContext, correctionId: string, input: z.infer<typeof payrollCorrectionDecisionSchema>, key: string) { return invoke(() => createPeoplePayrollApplication(context()).decidePayrollCorrection(actor, correctionId, input, key)); }
export async function payrollCorrectionAlertTarget(db: SqlDatabase, companyId: string, userId: string, correctionId: string, now = Date.now()) { return invoke(() => createPeoplePayrollApplication(context(db)).payrollCorrectionAlertTarget(companyId, userId, correctionId, now)); }
export async function exportPayrollReview(actor: WorkContext, runId: string, key: string) { return invoke(() => createPeoplePayrollApplication(context()).exportPayrollReview(actor, runId, key)); }
export async function getPayrollReviewFile(actor: WorkContext, runId: string, revision: number) { return invoke(() => createPeoplePayrollApplication(context()).getPayrollReviewFile(actor, runId, revision)); }
export async function createPaymentBatch(actor: WorkContext, input: z.infer<typeof paymentBatchSchema>, commandId: string) { return invoke(() => createPeoplePayrollApplication(context()).createPaymentBatch(actor, input, commandId)); }
export async function preparePaymentRetry(actor: WorkContext, batchId: string, expectedVersion: number, commandId: string) { return invoke(() => createPeoplePayrollApplication(context()).preparePaymentRetry(actor, batchId, expectedVersion, commandId)); }
export async function approvePaymentBatch(actor: WorkContext, batchId: string, expectedVersion: number, commandId: string) { return invoke(() => createPeoplePayrollApplication(context()).approvePaymentBatch(actor, batchId, expectedVersion, commandId)); }
export async function exportPaymentBatch(actor: WorkContext, batchId: string, commandId: string) { return invoke(() => createPeoplePayrollApplication(context()).exportPaymentBatch(actor, batchId, commandId)); }
export async function confirmPaymentBatch(actor: WorkContext, batchId: string, input: z.infer<typeof paymentConfirmationSchema>, commandId: string) { return invoke(() => createPeoplePayrollApplication(context()).confirmPaymentBatch(actor, batchId, input, commandId)); }
export async function importSyntheticPaymentResults(actor: WorkContext, batchId: string, csv: string, commandId: string) { return invoke(() => createPeoplePayrollApplication(context()).importSyntheticPaymentResults(actor, batchId, csv, commandId)); }
export async function getPaymentFile(actor: WorkContext, batchId: string) { return invoke(() => createPeoplePayrollApplication(context()).getPaymentFile(actor, batchId)); }
export async function calculatePayrollItemForSystem(write: SqlDatabase, run: PayrollStatutoryRun, engagement: {
    id: string;
    version: number;
    person_id: string;
    worker_type: string;
    employment_type: string;
    start_date: string;
    end_date: string | null;
    job_title: string;
}, policy: {
    overtimeMultiplierBps: number;
    prorationMethod: "calendar_days" | "none";
}, useSimulationAdapter: boolean) { return invoke(() => createPeoplePayrollApplication(context(write)).calculateItem(run, engagement, policy, useSimulationAdapter)); }
export async function payrollRunsPage(actor:WorkContext,cursor?:string){return invoke(()=>createPeoplePayrollApplication(context()).payrollRunsPage(actor,cursor));}
function scopeWhere(actor: WorkContext, alias = "p") {
    if (actor.departmentId)
        return { sql: `${alias}.company_id=? AND ${alias}.department_id=?`, args: [actor.companyId, actor.departmentId] };
    if (actor.branchId)
        return { sql: `${alias}.company_id=? AND ${alias}.branch_id=?`, args: [actor.companyId, actor.branchId] };
    return { sql: `${alias}.company_id=?`, args: [actor.companyId] };
}
