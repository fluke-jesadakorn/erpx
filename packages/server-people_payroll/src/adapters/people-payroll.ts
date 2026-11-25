import {ppRd42IdVersionLiteralGuard} from './pp-read-tail-r42-sources';
import {ppRd63CompRevisionEngagementGuard} from './pp-read-tail-r63-sources';
import {ppRd64TimeEntryIdGuard} from './pp-read-tail-r64-sources';
import {ppRd45RunByIdProjection} from './pp-read-tail-r45-sources';
import {ppRd38SinglePredTail} from './pp-read-tail-r38-sources';
import {ppRd37StatusGuard5} from './pp-read-tail-r37-sources';
import {ppRd54RunEvidenceGuard} from './pp-read-tail-r54-sources';
import {ppRd56EngagementGuard, ppRd56PayrollItemRow} from './pp-read-tail-r56-sources';
import {ppRd36SinglePred, ppRd36ThreePred, ppRd36TwoPredTail} from './pp-read-tail-r36-sources';
import { PP_RD23_HR_COMP_REVISIONS_SQL, PP_RD23_HR_PAYROLL_COST_ASSIGNMENTS_SQL, PP_RD23_HR_PAY_GROUPS_ACTIVE_CURRENCY_GUARD, PP_RD23_HR_TIME_ENTRIES_OVERTIME_SQL, PP_RD23_OVERTIME_PAYROLL_HANDOFFS_SQL, PP_RD23_POSTED_PAYROLL_RUN_GUARD } from './pp-read-tail-r23-sources';
import { ppRd30Exists4Guard } from './pp-read-tail-r30-sources';
import { ppRd31StatusGuard } from './pp-read-tail-r31-sources';
import { ppRd32TwoColProjection } from './pp-read-tail-r32-sources';
import { ppRd33FourColByIdCompany } from './pp-read-tail-r33-sources';
import { ppRd34ActivePayGroupProjection } from './pp-read-tail-r34-sources';
import { ppRd35PayrollItemsByRun } from './pp-read-tail-r35-sources';
import { ppRd26IdCompanyVersionStatusGuard, ppRd26CommandReceiptPriorSql, ppRd26RunIdGuard } from './pp-read-tail-r26-sources';
import { ppRd27IdByIdCompanySql } from './pp-read-tail-r27-sources';
import { PP_RD28_ADVANCE_REPAYMENTS_SQL } from './pp-read-tail-r28-sources';
import { PP_RD24_TIME_ENTRY_REVIEWABLE_GUARD, PP_RD24_STATUTORY_PACK_GUARD, PP_RD24_APPROVED_RUN_EVIDENCE_GUARD } from './pp-read-tail-r24-sources';
import { ppRd21IdCompanyStatusVersionGuard } from './pp-read-tail-r21-sources';
import { nativePpStatement } from '@erpx/server-people_payroll/adapters/native-people-payroll-command';
import {activePayee} from './pp-read-tail-r53-native';
import {PP_RD19_ACTIVE_ENGAGEMENT_EXISTS_SQL} from './pp-read-tail-r19-sources';
import { nativePeoplePayrollReadAll, PEOPLE_PAYROLL_RD_PAYROLL_OPTIONS_GROUPS } from './native-people-payroll-read';
import {hireHandoffPayGroupSchema,type HireHandoffPayGroupResult} from '@erpx/server-people_payroll/domain/hire-handoff-pay-group';
import {withHireHandoffPayGroupCommand,hireHandoffPayGroupEnvelope} from '@erpx/server-people_payroll/adapters/hire-handoff-pay-group';
import {workforcePurchaseControls,workforcePurchasedHistorySql,withWorkforcePurchasedCommand} from "@erpx/server-people_payroll/adapters/workforce-purchases";
import {withPeoplePurchasedRead,withPeoplePurchasedCommand,peopleOverviewPurchases} from "@erpx/server-people_payroll/adapters/people-purchases";
import { withPayrollPurchasedCommand, withPayrollPurchasedRead, withPayrollPaymentCommand, withPayrollPaymentRead } from "@erpx/server-people_payroll/adapters/payroll-purchases";
import {peopleVerifiedContextProjections as organizationFinalOwnerProjections} from "@erpx/server-organization/public/people-payroll-verified-context-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import {payrollDecisionNotificationEffects} from '@erpx/server-collaboration/adapters/native-payroll-decision-notification';
import { readListPage } from "@erpx/server-work_projection/public/list-page";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import {organizationGuard} from '@erpx/server-organization/public/organization-graph-guard';
import {validateStaffOrganization} from '@erpx/server-organization/public/organization';
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources, runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import { ApiError } from "@erpx/server-runtime/public/http";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { calculateThaiHotelPayrollFixture, isThaiHotelPayrollMockEnabled, thaiHotelPayrollMockId } from "@erpx/server-runtime/public/simulation-only-adapters";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { directoryPerson, canReadCompensation, canManagePeople } from "@erpx/server-people_payroll/adapters/people-directory";
import {prepareJournal} from '@erpx/server-finance/public/journal-planning';
import { type PayrollPostingItem } from "@erpx/server-people_payroll/adapters/payroll-accounting";
import { assertPayrollSourceFresh, payrollSourceGuards } from "@erpx/server-people_payroll/adapters/payroll-readiness";
import { prepareAttachmentBinding } from "@erpx/server-files/public/attachments/service";
import {snapshotMoney} from '@erpx/server-finance/public/company-money';
import { assertStatutoryRunReady, recognizedThaiPack, thaiEvidenceForEmployee, thaiEvidenceGuards, thaiPackForRun, type PayrollStatutoryRun } from "@erpx/server-people_payroll/adapters/payroll-statutory";
import { preparePayslipIssue } from "@erpx/server-people_payroll/adapters/payroll-payslips";
import { assertPayrollRecoveryDeductionLimits } from "@erpx/server-people_payroll/adapters/payroll-recoveries";
import { preparePayrollDeductionConsumption } from "@erpx/server-people_payroll/adapters/payroll-deductions";
import type { FileStore } from '@erpx/server-runtime/public/runtime/types';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { createPersonSchema, createEngagementSchema, compensationSchema, compensationTransitionSchema, engagementTransitionSchema, payrollCostVerificationSchema, payGroupSchema, countryPackSchema, countryPackTransitionSchema, payrollRunSchema, paymentBatchSchema, timeEntrySchema, paymentConfirmationSchema, payrollPostSchema, payrollAccountMappingSchema, payrollCorrectionRequestSchema, payrollCorrectionDecisionSchema, finalPayPreparationSchema, payrollAdvanceRequestSchema, payrollAdvanceDecisionSchema, assignPayGroupSchema, timeEntryUpdateSchema, timeEntryReviewSchema } from '@erpx/server-people_payroll/domain/people-payroll';
import { createPeoplePayrollApplication, type PeoplePayrollRepository, type PeoplePayrollPorts } from '@erpx/server-people_payroll/application/people-payroll';
import { payrollWorkflowFacts } from '@erpx/server-workflow/public/payroll-owner-facts';
import { payrollOrganizationFacts } from '@erpx/server-organization/public/payroll-owner-facts';
import { payrollIdentityFacts } from '@erpx/server-identity_access/public/payroll-owner-facts';
import { payrollBenefitOwnerFacts } from '@erpx/server-benefits/public/payroll-owner-facts';
import { payrollFinanceFacts } from '@erpx/server-finance/public/payroll-owner-facts';
export { createPersonSchema, createEngagementSchema, compensationSchema, compensationTransitionSchema, engagementTransitionSchema, payrollCostVerificationSchema, payGroupSchema, countryPackSchema, countryPackTransitionSchema, payrollRunSchema, paymentBatchSchema, timeEntrySchema, paymentConfirmationSchema, payrollPostSchema, payrollAccountMappingSchema, payrollCorrectionRequestSchema, payrollCorrectionDecisionSchema, finalPayPreparationSchema, payrollAdvanceRequestSchema, payrollAdvanceDecisionSchema, assignPayGroupSchema, timeEntryUpdateSchema, timeEntryReviewSchema } from '@erpx/server-people_payroll/domain/people-payroll';
export { payrollCancelSchema } from "@erpx/server-people_payroll/domain/payroll-cancellation";
export { createPayrollAdjustment, editPayrollAdjustment, payrollAdjustmentCreateSchema, payrollAdjustmentEditSchema } from "@erpx/server-people_payroll/adapters/payroll-adjustments";
function context(provided?: SqlDatabase, actor?: WorkContext): PeoplePayrollPorts<SqlStatement> {
    let database = provided!, files: FileStore;
    const repository: PeoplePayrollRepository<SqlStatement> = { readPayrollCostAssignmentsEngagement(engagementId: string, companyId: string) { return database.prepare(ppRd27IdByIdCompanySql('hr_engagements')).bind(engagementId, companyId).first(); },
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
        readVerifyPayrollCostAssignmentHistory(engagement_id: string, companyId: string) { return database.prepare(ppRd36TwoPredTail("1","staff_organization_history","engagement_id","company_id"," LIMIT 1")).bind(engagement_id, companyId).first(); },
        guard5(commandId: string, value: "assignment-version", assignmentId: string, companyId: string, expectedVersion: number) { return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM hr_payroll_cost_assignments a JOIN hr_engagements e ON e.id=a.engagement_id AND e.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.status='active' AND a.evidence_status='legacy_unverified' AND a.source='migration' AND a.version=? AND a.branch_id IS NOT DISTINCT FROM e.branch_id AND a.department_id IS NOT DISTINCT FROM e.department_id AND NOT EXISTS(SELECT 1 FROM staff_organization_history h WHERE h.engagement_id=a.engagement_id AND h.company_id=a.company_id))", [assignmentId, companyId, expectedVersion]); },
        stageVerifyPayrollCostAssignmentEffect(now: number, assignmentId: string, companyId: string, expectedVersion: number) { return nativePpStatement('people_payroll_stmt_1').bind(now, assignmentId, companyId, expectedVersion); },
        stageVerifyPayrollCostAssignmentEffect2(value: string, companyId: string, assignmentId: string, userId: string, evidenceRef: string, expectedVersion: number, now: number) { return nativePpStatement('people_payroll_stmt_2').bind(value, companyId, assignmentId, userId, evidenceRef, expectedVersion, now); },
        stageVerifyPayrollCostAssignmentEffect3(assignmentId: string) { return database.prepare("SELECT set_config('erpx.payroll_cost_repair',?,true)").bind(assignmentId); },
        stageVerifyPayrollCostAssignmentEffect4(now: number, assignmentId: string, companyId: string) {
            return nativePpStatement('people_payroll_stmt_3').bind(now, assignmentId, companyId);
        },
        readCommandCurrent(...args) { return payrollWorkflowFacts(database).readCommandCurrent(...args); },
        readCommandOld(id: string) { return database.prepare(ppRd26CommandReceiptPriorSql('hr_command_receipts')).bind(id).first(); },
        stageReceiptEffect(id: string, operation: string, digest: string, value: string, now: number) {
            return nativePpStatement('people_payroll_stmt_4')
                .bind(id, operation, digest, value, now);
        },
        readListPeopleRows(...args) { return payrollOrganizationFacts(database).readListPeopleRows(...args); },
        readGetPersonCompensation(engagement_id: string | number | null) { return database.prepare("SELECT base_amount_minor,hourly_rate_minor,pay_frequency,effective_from AS compensation_effective_from FROM hr_compensation_revisions WHERE engagement_id=? AND status='approved' AND effective_from<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (effective_to IS NULL OR effective_to>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) ORDER BY effective_from DESC LIMIT 1").bind(engagement_id).first(); },
        async readGetPersonLeavetime(personId: string, companyId: string, branchId: string | null, branchId2: string | null) { return database.prepare(`SELECT l.id,l.number,l.start_date,l.days,l.state,l.decision_note FROM leave_requests l JOIN employees e ON e.id=l.employee_id WHERE e.person_id=? AND l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND ${await workforcePurchasedHistorySql("l.company_id")} ORDER BY l.start_date DESC LIMIT 24`).bind(personId, companyId, branchId, branchId2).all(); },
        async readGetPersonLeavetime2(engagement_id: string | number | null) { return database.prepare(`SELECT t.id,t.version,t.submitted_by,t.approved_by,t.work_date,t.regular_hours,t.overtime_hours,t.status,t.note,t.rejection_reason FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id AND e.company_id=t.company_id WHERE t.engagement_id=? AND ${await workforcePurchasedHistorySql("t.company_id")} ORDER BY t.work_date DESC LIMIT 24`).bind(engagement_id).all(); },
        readGetPersonRevisions(engagement_id: string | number | null) { return database.prepare(ppRd38SinglePredTail('*', 'hr_compensation_revisions', 'engagement_id', ' ORDER BY effective_from DESC,created_at DESC')).bind(engagement_id).all(); },
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
        stageCreatePersonEffect(personId: string, value: null, companyId: string, legalName: string, value2: string | null, value3: string | null, value4: string | null, countryCode: string, userId: string, now: number, now2: number) { return nativePpStatement('people_payroll_stmt_5').bind(personId, value, companyId, legalName, value2, value3, value4, countryCode, userId, now, now2); },
        readCreateEngagementPerson(personId: string, companyId: string) { return database.prepare("SELECT id FROM hr_people WHERE id=? AND company_id=? AND status!='archived'").bind(personId, companyId).first(); },
        stageCreateEngagementEffect(engagementId: string, personId: string, companyId: string, value: string | null, value2: string | null, workerType: "employee" | "contractor", employmentType: "contractor" | "salaried" | "daily" | "hourly" | "shift", value3: "onboarding", startDate: string, value4: string | null, value5: string | null, jobTitle: string, currency: string, value6: string | null, userId: string, now: number, now2: number) { return nativePpStatement('people_payroll_stmt_6').bind(engagementId, personId, companyId, value, value2, workerType, employmentType, value3, startDate, value4, value5, jobTitle, currency, value6, userId, now, now2); },
        readCreateCompensationE(engagementId: string, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) { return database.prepare("SELECT e.* FROM hr_engagements e WHERE e.id=? AND e.company_id=? AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)").bind(engagementId, companyId, branchId, branchId2, departmentId, departmentId2).first<Record<string, string | number | null>>(); },
        guard31(key: string, value: "engagement", engagementId: string, version: string | number | null, currency: string) { return assertion(database, key, value, ppRd42IdVersionLiteralGuard("hr_engagements","currency"), [engagementId, version, currency]); },
        stageCreateCompensationEffect(id: string, engagementId: string, effectiveFrom: string, value: string | null, payFrequency: "weekly" | "fortnightly" | "semimonthly" | "monthly" | "off_cycle", base: number, hourly: number, daily: number, standardDailyHours: number, expectedHours: number, currency: string, value2: string, userId: string, now: number, now2: number) { return nativePpStatement('people_payroll_stmt_7').bind(id, engagementId, effectiveFrom, value, payFrequency, base, hourly, daily, standardDailyHours, expectedHours, currency, value2, userId, now, now2); },
        async readPayrollOptionsEffect(companyId: string) { return nativePeoplePayrollReadAll<Record<string, unknown>>(PEOPLE_PAYROLL_RD_PAYROLL_OPTIONS_GROUPS, [companyId], database); },
        readAssignPayGroupMissing(...args) { return payrollOrganizationFacts(database).readAssignPayGroupMissing(...args); },
        guard35(key: string, value: "engagement", engagementId: string, companyId: string, expectedVersion: number, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) { return assertion(database, key, value, ppRd56EngagementGuard("version=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)"), [engagementId, companyId, expectedVersion, branchId, branchId2, departmentId, departmentId2]); },
        guard36(...args) { return payrollOrganizationFacts(database).guard36(...args); },
        guard37(key: string, value: "pay-group", payGroupId: string | null, engagementId: string, payGroupId2: string | null) { return assertion(database, key, value, "?::text IS NULL OR EXISTS(SELECT 1 FROM hr_pay_groups p JOIN hr_engagements e ON e.id=? WHERE p.id=? AND p.company_id=e.company_id AND p.currency=e.currency AND p.status='active')", [payGroupId, engagementId, payGroupId2]); },
        stageAssignPayGroupEffect(payGroupId: string | null, now: number, engagementId: string) { return nativePpStatement('people_payroll_stmt_8').bind(payGroupId, now, engagementId); },
        readTransitionCompensationEffect(compensationId: string, companyId: string) { return database.prepare(ppRd32TwoColProjection("1","hr_evaluation_compensation_drafts","compensation_id","company_id")).bind(compensationId, companyId).first(); },
        readTransitionCompensationRow(compensationId: string, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) { return database.prepare("SELECT c.*,e.person_id,e.branch_id,e.department_id FROM hr_compensation_revisions c JOIN hr_engagements e ON e.id=c.engagement_id WHERE c.id=? AND e.company_id=? AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)").bind(compensationId, companyId, branchId, branchId2, departmentId, departmentId2).first<Record<string, string | number | null>>(); },
        guard41(key: string, value: "revision", compensationId: string, expectedVersion: number, value2: "approved" | "draft") { return assertion(database, key, value, ppRd42IdVersionLiteralGuard("hr_compensation_revisions","status"), [compensationId, expectedVersion, value2]); },
        guard42(key: string, value: "overlap", engagement_id: string | number | null, compensationId: string, effective_to: string | number | null, effective_from: string | number | null) { return assertion(database, key, value, ppRd63CompRevisionEngagementGuard(" AND id!=? AND status IN ('active','approved') AND effective_from<=COALESCE(?,'9999-12-31') AND COALESCE(effective_to,'9999-12-31')>=?"), [engagement_id, compensationId, effective_to, effective_from]); },
        stageTransitionCompensationEffect(status: "approved" | "superseded", userId: string, now: number, compensationId: string) { return nativePpStatement('people_payroll_stmt_9').bind(status, userId, now, compensationId); },
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
        guard46(key: string, value: "engagement", engagementId: string, companyId: string, expectedVersion: number, status: string) { return assertion(database, key, value, ppRd26IdCompanyVersionStatusGuard('hr_engagements'), [engagementId, companyId, expectedVersion, status]); },
        guard47(...args) { return payrollOrganizationFacts(database).guard47(...args); },
        stageTransitionEngagementEffect(status: "active" | "suspended" | "ended", status2: "active" | "suspended" | "ended", value: string | null, now: number, engagementId: string) { return nativePpStatement('people_payroll_stmt_10').bind(status, status2, value, now, engagementId); },
        stageTransitionEngagementEffect2(person_id: string, person_id2: string) { return nativePpStatement('people_payroll_stmt_11').bind(person_id, person_id2); },
        stageTransitionEngagementEffect3(...args) { return payrollIdentityFacts(database).stageTransitionEngagementEffect3(...args); },
        readCreateTimeEntryE(engagementId: string, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) { return database.prepare("SELECT e.*,p.user_id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.id=? AND e.company_id=? AND e.status='active' AND p.status='active' AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)").bind(engagementId, companyId, branchId, branchId2, departmentId, departmentId2).first<Record<string, string | number | null>>(); },
        guard52(key: string, value: "engagement", engagementId: string, version: string | number | null) { return assertion(database, key, value, PP_RD19_ACTIVE_ENGAGEMENT_EXISTS_SQL, [engagementId, version]); },
        stageCreateTimeEntryEffect(id: string, engagementId: string, companyId: string, workDate: string, regularHours: number, overtimeHours: number, note: string, userId: string, now: number, now2: number) { return nativePpStatement('people_payroll_stmt_12').bind(id, engagementId, companyId, workDate, regularHours, overtimeHours, note, userId, now, now2); },
        readUpdateTimeEntryEntry(entryId: string, companyId: string, userId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) { return database.prepare("SELECT t.*,e.start_date,e.end_date FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id WHERE t.id=? AND t.company_id=? AND t.submitted_by=? AND e.status='active' AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)").bind(entryId, companyId, userId, branchId, branchId2, departmentId, departmentId2).first<Record<string, string | number | null>>(); },
        guard55(key: string, value: "editable-time", entryId: string, expectedVersion: number, userId: string) { return assertion(database, key, value, ppRd64TimeEntryIdGuard(" AND version=? AND status IN ('draft','submitted','rejected') AND submitted_by=?"), [entryId, expectedVersion, userId]); },
        stageUpdateTimeEntryEffect(workDate: string, regularHours: number, overtimeHours: number, note: string, now: number, entryId: string) { return nativePpStatement('people_payroll_stmt_13').bind(workDate, regularHours, overtimeHours, note, now, entryId); },
        guard57(key: string, value: "reviewable-time", entryId: string, companyId: string, expectedVersion: number, userId: string, userId2: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) { return assertion(database, key, value, PP_RD24_TIME_ENTRY_REVIEWABLE_GUARD, [entryId, companyId, expectedVersion, userId, userId2, branchId, branchId2, departmentId, departmentId2]); },
        stageRejectTimeEntryEffect(reason: string, now: number, entryId: string) { return nativePpStatement('people_payroll_stmt_14').bind(reason, now, entryId); },
        stageRejectTimeEntryEffect2(...args) { return payrollWorkflowFacts(database).stageRejectTimeEntryEffect2(...args); },
        guard60(key: string, value: "time", entryId: string, companyId: string, expectedVersion: number, userId: string, userId2: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) { return assertion(database, key, value, PP_RD24_TIME_ENTRY_REVIEWABLE_GUARD, [entryId, companyId, expectedVersion, userId, userId2, branchId, branchId2, departmentId, departmentId2]); },
        stageApproveTimeEntryEffect(userId: string, now: number, entryId: string) { return nativePpStatement('people_payroll_stmt_15').bind(userId, now, entryId); },
        stageApproveTimeEntryEffect2(...args) { return payrollWorkflowFacts(database).stageApproveTimeEntryEffect2(...args); },
        stageCreatePayGroupEffect(groupId: string, companyId: string, name: string, countryCode: string, currency: string, frequency: "weekly" | "fortnightly" | "semimonthly" | "monthly", timezone: string, overtimeMultiplierBps: number, prorationMethod: "calendar_days" | "none", userId: string, now: number, now2: number) { return nativePpStatement('people_payroll_stmt_16').bind(groupId, companyId, name, countryCode, currency, frequency, timezone, overtimeMultiplierBps, prorationMethod, userId, now, now2); },
        stageCreateCountryPackEffect(packId: string, countryCode: string, version: string, value: "testing", value2: string, value3: string, userId: string, now: number, now2: number, value4: string | null, value5: string | null, value6: string | null, value7: string | null) { return nativePpStatement('people_payroll_stmt_17').bind(packId, countryCode, version, value, value2, value3, userId, now, now2, value4, value5, value6, value7); },
        readListCountryPacksEffect() { return database.prepare("SELECT id,country_code,version,status,effective_from,effective_to,adapter_id,source_digest,record_version,export_formats_json,validated_by,updated_at FROM hr_country_packs ORDER BY country_code,version DESC").all(); },
        readTransitionCountryPackPack(packId: string) {
            return database.prepare(ppRd36SinglePred("status,country_code,version,rules_json,created_by,record_version,source_digest,adapter_id", "hr_country_packs", "id")).bind(packId).first<{
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
        guard67(commandId: string, value: "pack-status", packId: string, status: string, record_version: number, source_digest: string) { return assertion(database, commandId, value, ppRd30Exists4Guard('hr_country_packs','id','status','record_version','source_digest'), [packId, status, record_version, source_digest]); },
        stageTransitionCountryPackEffect(status: "superseded" | "active" | "validated", proof: string, userId: string, now: number, packId: string) { return nativePpStatement('people_payroll_stmt_18').bind(status, proof, userId, now, packId); },
        stageTransitionCountryPackEffect2(...args) { return payrollWorkflowFacts(database).stageTransitionCountryPackEffect2(...args); },
        readCalculateItemPayee(person_id: string, company_id: string) {
            return activePayee<{
                id: string;
                version: number;
                legal_name: string;
                preferred_name: string | null;
            }>(database, person_id, company_id);
        },
        readCalculateItemCompensation(id: string, period_end: string, period_start: string) { return database.prepare(`${PP_RD23_HR_COMP_REVISIONS_SQL}`).bind(id, period_end, period_start).first<Record<string, string | number | null>>(); },
        readCalculateItemTime(id: string, period_start: string, period_end: string) {
            return database.prepare(PP_RD23_HR_TIME_ENTRIES_OVERTIME_SQL).bind(id, period_start, period_end).all<{
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
            return database.prepare(PP_RD23_OVERTIME_PAYROLL_HANDOFFS_SQL).bind(id, period_start, period_end).all<{
                id: string;
                request_id: string;
                request_version: number;
                minutes: number;
                work_date: string;
            }>();
        },
        readCalculateItemBenefitInputs(...args) { return payrollBenefitOwnerFacts(database).readCalculateItemBenefitInputs(...args); },
        readCalculateItemAdvanceRepayments(company_id: string, id: string, period_end: string, period_start: string, id2: string) {
            return database.prepare(PP_RD28_ADVANCE_REPAYMENTS_SQL).bind(company_id, id, period_end, period_start, id2).all<{
                id: string;
                version: number;
                amount_minor: number;
                due_period_start: string;
                due_period_end: string;
                reason: string;
            }>();
        },
        readCalculateItemCostAssignments(id: string, company_id: string, activeEnd: string, activeStart: string) {
            return database.prepare(PP_RD23_HR_PAYROLL_COST_ASSIGNMENTS_SQL).bind(id, company_id, activeEnd, activeStart).all<{
                id: string;
                effective_from: string;
                effective_to: string | null;
                branch_id: string | null;
                department_id: string | null;
                evidence_status: string;
                version: number;
            }>();
        },
        guard77(key: string, value: "pay-group", payGroupId: string, companyId: string, currency: string) { return assertion(database, key, value, PP_RD23_HR_PAY_GROUPS_ACTIVE_CURRENCY_GUARD, [payGroupId, companyId, currency]); },
        guard78(key: string, value: "duplicate-period", companyId: string, payGroupId: string, periodEnd: string, periodStart: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_payroll_runs WHERE company_id=? AND pay_group_id=? AND mode!='final_pay' AND adjustment_of_run_id IS NULL AND status!='cancelled' AND period_start<=? AND period_end>=?)", [companyId, payGroupId, periodEnd, periodStart]); },
        guard79(key: string, value: "statutory-pack", id: string, record_version: number, sourceDigest: string) { return assertion(database, key, value, PP_RD24_STATUTORY_PACK_GUARD, [id, record_version, sourceDigest]); },        readPrepareFinalPayTask(taskId: string, companyId: string) {
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
        guard83(key: string, value: "engagement-pay-group", engagement_id: string, companyId: string, payGroupId: string, currency: string) { return assertion(database, key, value, ppRd30Exists4Guard('hr_engagements','id','company_id','pay_group_id','currency'), [engagement_id, companyId, payGroupId, currency]); },
        guard84(key: string, value: "one-final-pay", taskId: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_payroll_final_pay_preparations WHERE task_id=? AND status!='cancelled')", [taskId]); },
        guard85(key: string, value: "statutory-pack", id: string, record_version: number, sourceDigest: string) { return assertion(database, key, value, PP_RD24_STATUTORY_PACK_GUARD, [id, record_version, sourceDigest]); },
        stagePrepareFinalPayEffect(runId: string, companyId: string, payGroupId: string, value: string | null, engagement_id: string, periodStart: string, periodEnd: string, payDate: string, currency: string, userId: string, now: number, now2: number) { return nativePpStatement('people_payroll_stmt_19').bind(runId, companyId, payGroupId, value, engagement_id, periodStart, periodEnd, payDate, currency, userId, now, now2); },
        stagePrepareFinalPayEffect2(prepId: string, companyId: string, taskId: string, case_id: string, engagement_id: string, runId: string, reason: string, userId: string, now: number, now2: number) { return nativePpStatement('people_payroll_stmt_20').bind(prepId, companyId, taskId, case_id, engagement_id, runId, reason, userId, now, now2); },
        stagePrepareFinalPayEffect3(value: string, prepId: string, taskId: string, runId: string, companyId: string, userId: string, expectedTaskVersion: number, value2: string, now: number) { return nativePpStatement('people_payroll_stmt_21').bind(value, prepId, taskId, runId, companyId, userId, expectedTaskVersion, value2, now); },
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
        guard91(key: string, value: "no-pending-advance", engagementId: string) { return assertion(database, key, value, ppRd31StatusGuard(true, 'hr_payroll_advances', 'engagement_id', 'pending'), [engagementId]); },
        stageRequestPayrollAdvanceEffect(id: string, companyId: string, id2: string, person_id: string, amountMinor: number, installmentCount: number, firstDeductionPeriod: string, currency: string, reason: string, userId: string, now: number, now2: number) { return nativePpStatement('people_payroll_stmt_22').bind(id, companyId, id2, person_id, amountMinor, installmentCount, firstDeductionPeriod, currency, reason, userId, now, now2); },
        stageRequestPayrollAdvanceEffect2(value: string, id: string, companyId: string, userId: string, value2: 1, value3: string, now: number) { return nativePpStatement('people_payroll_stmt_23').bind(value, id, companyId, userId, value2, value3, now); },
        stageRequestPayrollAdvanceEffect3(...args) { return payrollIdentityFacts(database, actor).stageRequestPayrollAdvanceEffect3(...args); },
        readDecidePayrollAdvanceRow(advanceId: string, companyId: string) { return database.prepare("SELECT a.*,p.user_id FROM hr_payroll_advances a JOIN hr_people p ON p.id=a.person_id WHERE a.id=? AND a.company_id=?").bind(advanceId, companyId).first<Record<string, string | number | null>>(); },
        guard96(key: string, value: "advance-version", advanceId: string, companyId: string, expectedVersion: number) { return assertion(database, key, value, ppRd21IdCompanyStatusVersionGuard('hr_payroll_advances', 'pending'), [advanceId, companyId, expectedVersion]); },
        stageDecidePayrollAdvanceStatements(status: "approved" | "rejected" | "cancelled", status2: "approved" | "rejected" | "cancelled", userId: string, reason: string, now: number, now2: number, advanceId: string) { return nativePpStatement('people_payroll_stmt_24').bind(status, status2, userId, reason, now, now2, advanceId); },
        stageDecidePayrollAdvanceStatements2(value: string, advanceId: string, companyId: string, value2: "approved" | "rejected" | "cancelled", userId: string, version: number, value3: string, now: number) { return nativePpStatement('people_payroll_stmt_25').bind(value, advanceId, companyId, value2, userId, version, value3, now); },
        ...payrollDecisionNotificationEffects(()=>database, actor!),
        stageDecidePayrollAdvanceEffect(value: string, advanceId: string, companyId: string, value2: number, value3: string, value4: string, value5: number, now: number) { return nativePpStatement('people_payroll_stmt_26').bind(value, advanceId, companyId, value2, value3, value4, value5, now); },
        readPayrollAdvancePageRows(value: string[], privileged: boolean) { return database.prepare(`SELECT a.*,p.legal_name,e.job_title,e.currency,e.pay_group_id FROM hr_payroll_advances a JOIN hr_people p ON p.id=a.person_id JOIN hr_engagements e ON e.id=a.engagement_id WHERE a.company_id=? AND (${privileged ? "TRUE" : "p.user_id=?"}) ORDER BY a.created_at DESC,a.id DESC`).bind(...value).all(); },
        readPayrollAdvancePageOptions(value: string[], privileged: boolean) { return database.prepare(`SELECT e.id engagement_id,p.legal_name,e.currency,e.pay_group_id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.status IN ('active','suspended') AND e.pay_group_id IS NOT NULL AND p.status='active' AND ${privileged ? "TRUE" : "p.user_id=?"} ORDER BY p.legal_name,e.id`).bind(...value).all(); },
        readPayrollAdvanceAlertTargetEffect(...args) { return payrollIdentityFacts(database).readPayrollAdvanceAlertTargetEffect(...args); },
        readPayrollPostingOptionsChartsperiodsmappingsgroups(...args) { return payrollFinanceFacts(database).readPayrollPostingOptionsChartsperiodsmappingsgroups(...args); },
        readPayrollPostingOptionsChartsperiodsmappingsgroups2(...args) { return payrollFinanceFacts(database).readPayrollPostingOptionsChartsperiodsmappingsgroups2(...args); },
        readPayrollPostingOptionsChartsperiodsmappingsgroups3(companyId: string) { return database.prepare("SELECT id,pay_group_id,chart_id,accounts_json,status,version,created_by,approved_by FROM hr_payroll_account_mappings WHERE company_id=? AND status IN ('draft','approved') ORDER BY pay_group_id,created_at DESC").bind(companyId).all(); },
        async readPayrollPostingOptionsChartsperiodsmappingsgroups4(companyId: string) { return nativePeoplePayrollReadAll<Record<string, unknown>>(PEOPLE_PAYROLL_RD_PAYROLL_OPTIONS_GROUPS, [companyId], database); },
        readPayrollPostingOptionsAccounts(...args) { return payrollFinanceFacts(database).readPayrollPostingOptionsAccounts(...args); },
        readResolvePayrollPostRun(runId: string, companyId: string) {
            return database.prepare(ppRd32TwoColProjection('pay_group_id,period_end', 'hr_payroll_runs', 'id', 'company_id')).bind(runId, companyId).first<{
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
            return database.prepare(ppRd34ActivePayGroupProjection('id,currency')).bind(payGroupId, companyId).first<{
                id: string;
                currency: string;
            }>();
        },
        readSavePayrollAccountMappingChart(...args) { return payrollFinanceFacts(database).readSavePayrollAccountMappingChart(...args); },
        readSavePayrollAccountMappingAccounts(...args) { return payrollFinanceFacts(database).readSavePayrollAccountMappingAccounts(...args); },
        stageSavePayrollAccountMappingEffect(id: string, companyId: string, payGroupId: string, chartId: string, value: string, userId: string, now: number, now2: number) { return nativePpStatement('people_payroll_stmt_27').bind(id, companyId, payGroupId, chartId, value, userId, now, now2); },
        readApprovePayrollAccountMappingRow(mappingId: string, companyId: string) {
            return database.prepare(ppRd32TwoColProjection("id,pay_group_id,created_by,status,version","hr_payroll_account_mappings","id","company_id")).bind(mappingId, companyId).first<{
                id: string;
                pay_group_id: string;
                created_by: string;
                status: string;
                version: number;
            }>();
        },
        guard141(...args) { return payrollIdentityFacts(database).guard141(...args); },
        stageApprovePayrollAccountMappingEffect(now: number, companyId: string, pay_group_id: string) { return nativePpStatement('people_payroll_stmt_28').bind(now, companyId, pay_group_id); },
        stageApprovePayrollAccountMappingEffect2(userId: string, now: number, mappingId: string) { return nativePpStatement('people_payroll_stmt_29').bind(userId, now, mappingId); },
        readPreparePayrollAccrualRun(runId: string, companyId: string) {
            return database.prepare(`SELECT r.*, COUNT(i.id) item_count,
      COUNT(i.id) FILTER (WHERE i.status NOT IN ('calculated','approved','posted')) invalid_items
    FROM hr_payroll_runs r LEFT JOIN hr_payroll_items i ON i.run_id=r.id
    WHERE r.id=? AND r.company_id=? GROUP BY r.id`).bind(runId, companyId).first<Record<string, string | number | null>>();
        },
        readPreparePayrollAccrualItems(runId: string) {
            return database.prepare(ppRd35PayrollItemsByRun('id,worker_type,gross_minor,deductions_minor,employer_cost_minor,net_minor,input_snapshot_json,result_json', ' ORDER BY id')).bind(runId).all<PayrollPostingItem & {
                worker_type: string;
            }>();
        },
        readPreparePayrollAccrualValid(...args) { return payrollFinanceFacts(database).readPreparePayrollAccrualValid(...args); },
        guard147(key: string, value: "payroll-run", runId: string, companyId: string, expectedVersion: number) { return assertion(database, key, value, ppRd54RunEvidenceGuard("status='approved' AND version=? AND input_digest", " IS NOT NULL"), [runId, companyId, expectedVersion]); },
        guard148(key: string, value: "payroll-mapping", resolvedMappingId: string, companyId: string, runId: string, resolvedMappingVersion: number | null) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_account_mappings WHERE id=? AND company_id=? AND pay_group_id=(SELECT pay_group_id FROM hr_payroll_runs WHERE id=?) AND status='approved' AND version=?)", [resolvedMappingId, companyId, runId, resolvedMappingVersion]); },
        guard149(key: string, value: "payroll-items", runId: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_payroll_items WHERE run_id=? AND status NOT IN ('calculated','approved','posted'))", [runId]); },
        guard150(key: string, value: "payroll-posting-unique", runId: string) { return assertion(database, key, value, ppRd26RunIdGuard('hr_payroll_postings'), [runId]); },
        readApprovePayrollRunSubmitted(runId: string, companyId: string) {
            return database.prepare(ppRd32TwoColProjection("submitted_by","hr_payroll_runs","id","company_id")).bind(runId, companyId).first<{
                submitted_by: string | null;
            }>();
        },
        guard155(...args) { return payrollIdentityFacts(database).guard155(...args); },
        stageApprovePayrollRunEffect(userId: string, now: number, runId: string) { return nativePpStatement('people_payroll_stmt_30').bind(userId, now, runId); },
        stageApprovePayrollRunEffect2(...args) { return payrollWorkflowFacts(database).stageApprovePayrollRunEffect2(...args); },
        readRequestPayrollCorrectionRun(runId: string, companyId: string) {
            return database.prepare(ppRd32TwoColProjection("id,status,version,revision,input_digest,totals_json","hr_payroll_runs","id","company_id")).bind(runId, companyId).first<{
                id: string;
                status: string;
                version: number;
                revision: number;
                input_digest: string | null;
                totals_json: string;
            }>();
        },
        guard159(key: string, value: "approved-run", runId: string, companyId: string, expectedVersion: number, revision: number, input_digest: string) { return assertion(database, key, value, PP_RD24_APPROVED_RUN_EVIDENCE_GUARD, [runId, companyId, expectedVersion, revision, input_digest]); },
        stageRequestPayrollCorrectionEffect(id: string, runId: string, companyId: string, version: number, revision: number, input_digest: string, totals_json: string, reason: string, userId: string, now: number, now2: number) { return nativePpStatement('people_payroll_stmt_31').bind(id, runId, companyId, version, revision, input_digest, totals_json, reason, userId, now, now2); },
        stageRequestPayrollCorrectionEffect2(value: string, id: string, runId: string, companyId: string, userId: string, version: number, value2: string, now: number) { return nativePpStatement('people_payroll_stmt_32').bind(value, id, runId, companyId, userId, version, value2, now); },
        stageRequestPayrollCorrectionEffect3(...args) { return payrollIdentityFacts(database, actor).stageRequestPayrollCorrectionEffect3(...args); },
        readDecidePayrollCorrectionRow(correctionId: string, companyId: string) { return database.prepare("SELECT c.*,r.status run_status,r.version current_run_version,r.revision current_run_revision,r.input_digest current_input_digest FROM hr_payroll_corrections c JOIN hr_payroll_runs r ON r.id=c.run_id WHERE c.id=? AND c.company_id=?").bind(correctionId, companyId).first<Record<string, string | number | null>>(); },
        guard164(key: string, value: "correction-version", correctionId: string, companyId: string, expectedVersion: number) { return assertion(database, key, value, ppRd21IdCompanyStatusVersionGuard('hr_payroll_corrections', 'pending'), [correctionId, companyId, expectedVersion]); },
        guard165(key: string, value: "run-evidence", run_id: string | number | null, companyId: string, run_version: string | number | null, run_revision: string | number | null, input_digest: string | number | null) { return assertion(database, key, value, PP_RD24_APPROVED_RUN_EVIDENCE_GUARD, [run_id, companyId, run_version, run_revision, input_digest]); },
        stageDecidePayrollCorrectionStatements(status: "approved" | "rejected" | "cancelled", userId: string, reason: string, now: number, now2: number, correctionId: string) { return nativePpStatement('people_payroll_stmt_33').bind(status, userId, reason, now, now2, correctionId); },
        stageDecidePayrollCorrectionStatements2(value: string, correctionId: string, run_id: string | number | null, companyId: string, status: "approved" | "rejected" | "cancelled", userId: string, run_version: string | number | null, value2: string, now: number) { return nativePpStatement('people_payroll_stmt_34').bind(value, correctionId, run_id, companyId, status, userId, run_version, value2, now); },

        stageDecidePayrollCorrectionEffect(value: string, now: number, run_id: string | number | null) { return nativePpStatement('people_payroll_stmt_35').bind(value, now, run_id); },
        readPayrollCorrectionAlertTargetEffect(...args) { return payrollIdentityFacts(database).readPayrollCorrectionAlertTargetEffect(...args); },
        readExportPayrollReviewRun(runId: string, companyId: string) {
            return database.prepare(ppRd32TwoColProjection("id,status,revision,input_digest,currency,period_start,period_end","hr_payroll_runs","id","company_id")).bind(runId, companyId).first<{
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
            return database.prepare(ppRd32TwoColProjection("checksum_sha256","hr_payroll_review_exports","run_id","run_revision")).bind(runId, revision).first<{
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
        guard174(key: string, value: "run", runId: string, companyId: string, revision: number, input_digest: string) { return assertion(database, key, value, ppRd54RunEvidenceGuard("revision=? AND input_digest=? AND status IN ('preview','in_review','approved')", ""), [runId, companyId, revision, input_digest]); },
        stageExportPayrollReviewEffect(exportId: string, runId: string, revision: number, fileKey: string, checksum: string, userId: string, now: number) { return nativePpStatement('people_payroll_stmt_36').bind(exportId, runId, revision, fileKey, checksum, userId, now); },
        readGetPayrollReviewFileRow(runId: string, revision: number, companyId: string) {
            return database.prepare("SELECT e.file_key,e.checksum_sha256 FROM hr_payroll_review_exports e JOIN hr_payroll_runs r ON r.id=e.run_id WHERE e.run_id=? AND e.run_revision=? AND r.company_id=?").bind(runId, revision, companyId).first<{
                file_key: string;
                checksum_sha256: string;
            }>();
        },
        readCreatePaymentBatchDuplicate(companyId: string, sourceType: "contractor" | "payroll" | "expense", sourceId: string) { return database.prepare("SELECT id FROM hr_payment_batches WHERE company_id=? AND source_type=? AND source_id=? AND status NOT IN ('cancelled') LIMIT 1").bind(companyId, sourceType, sourceId).first(); },
        readCreatePaymentBatchSource(sourceId: string, companyId: string) {
            return database.prepare(ppRd45RunByIdProjection("id,status,currency")).bind(sourceId, companyId).first<{
                id: string;
                status: string;
                currency: string;
            }>();
        },
        readCreatePaymentBatchPosted(sourceId: string) {
            return database.prepare(ppRd56PayrollItemRow("net_minor>0 AND status='posted' ORDER BY engagement_id")).bind(sourceId).all<{
                id: string;
                engagement_id: string;
                net_minor: number;
                currency: string;
                input_snapshot_json: string;
            }>();
        },
        readCreatePaymentBatchInvoice(...args) { return payrollFinanceFacts(database).readCreatePaymentBatchInvoice(...args); },
        readCreatePaymentBatchBank(...args) { return payrollFinanceFacts(database).readCreatePaymentBatchBank(...args); },
        guard182(commandId: string, value: "posted-payroll-source", sourceId: string, companyId: string) { return assertion(database, commandId, value, PP_RD23_POSTED_PAYROLL_RUN_GUARD, [sourceId, companyId]); },
        guard183(commandId: string, value: string, id: string, sourceId: string, net_minor: number, input_snapshot_json: string) { return assertion(database, commandId, value, ppRd37StatusGuard5("hr_payroll_items", "run_id", "posted", "net_minor", "input_snapshot_json"), [id, sourceId, net_minor, input_snapshot_json]); },
        stageCreatePaymentBatchStatements(batchId: string, companyId: string, sourceType: "contractor" | "payroll", sourceId: string, currency: string, value: "draft", formatCode: string, total: number, value2: string | null, userId: string, now: number, now2: number) { return nativePpStatement('people_payroll_stmt_37').bind(batchId, companyId, sourceType, sourceId, currency, value, formatCode, total, value2, userId, now, now2); },
        guard185(...args) { return payrollFinanceFacts(database).guard185(...args); },
        guard186(commandId: string, value: "contractor-batch-unique", companyId: string, sourceId: string, batchId: string) { return assertion(database, commandId, value, "NOT EXISTS(SELECT 1 FROM hr_payment_batches WHERE company_id=? AND source_type='contractor' AND source_id=? AND status<>'cancelled' AND id<>?)", [companyId, sourceId, batchId]); },
        stageCreatePaymentBatchEffect(...args) { return payrollFinanceFacts(database).stageCreatePaymentBatchEffect(...args); },
        stageCreatePaymentBatchEffect2(value: string, batchId: string, engagement_id: string | null, person_id: string | null, value2: string, value3: string, net_minor: number, currency: string) { return nativePpStatement('people_payroll_stmt_38').bind(value, batchId, engagement_id, person_id, value2, value3, net_minor, currency); },
        readPreparePaymentRetryBatch(batchId: string, companyId: string) {
            return database.prepare(ppRd33FourColByIdCompany('source_type,source_id,status,version', 'hr_payment_batches')).bind(batchId, companyId).first<{
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
        guard192(commandId: string, value: "batch-retry-items", batchId: string) { return assertion(database, commandId, value, ppRd31StatusGuard(false, 'hr_payment_batch_items', 'batch_id', 'failed'), [batchId]); },
        stagePreparePaymentRetryEffect(userId: string, now: number, batchId: string, companyId: string, expectedVersion: number) { return nativePpStatement('people_payroll_stmt_39').bind(userId, now, batchId, companyId, expectedVersion); },
        readApprovePaymentBatchBatch(batchId: string, companyId: string) {
            return database.prepare(ppRd32TwoColProjection("status,version,source_type,created_by,retry_requested_by","hr_payment_batches","id","company_id")).bind(batchId, companyId).first<{
                status: string;
                version: number;
                source_type: string;
                created_by: string;
                retry_requested_by: string | null;
            }>();
        },
        guard195(...args) { return payrollIdentityFacts(database).guard195(...args); },
        stageApprovePaymentBatchEffect(userId: string, now: number, batchId: string, companyId: string, expectedVersion: number) { return nativePpStatement('people_payroll_stmt_40').bind(userId, now, batchId, companyId, expectedVersion); },
        stageApprovePaymentBatchEffect2(...args) { return payrollWorkflowFacts(database).stageApprovePaymentBatchEffect2(...args); },
        readExportPaymentBatchV2Batch(batchId: string, companyId: string) {
            return database.prepare(ppRd32TwoColProjection("id,status,version,source_type,source_id,format_code,retry_requested_by","hr_payment_batches","id","company_id")).bind(batchId, companyId).first<{
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
            return database.prepare(ppRd36TwoPredTail("id,reference,person_id,payee_name,amount_minor,currency,status", "hr_payment_batch_items", "batch_id", "status", " ORDER BY reference")).bind(batchId, eligibleStatus).all<{
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
            return database.prepare(ppRd56PayrollItemRow("status='posted' AND ?=('PAY-'||?||'-'||id)")).bind(source_id, reference, source_id2).first<{
                id: string;
                engagement_id: string;
                net_minor: number;
                currency: string;
                input_snapshot_json: string;
            }>();
        },
        guard201(commandId: string, value: string, source_id: string, id: string, id2: string, batchId: string, eligibleStatus: "failed" | "outstanding", reference: string, person_id: string, payee_name: string, amount_minor: number, currency: string, net_minor: number, currency2: string, input_snapshot_json: string) { return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM hr_payment_batch_items b JOIN hr_payroll_items p ON p.run_id=? AND p.id=? WHERE b.id=? AND b.batch_id=? AND b.status=? AND b.reference=? AND b.person_id=? AND b.payee_name=? AND b.amount_minor=? AND b.currency=? AND p.status='posted' AND p.net_minor=? AND p.currency=? AND p.input_snapshot_json=?)", [source_id, id, id2, batchId, eligibleStatus, reference, person_id, payee_name, amount_minor, currency, net_minor, currency2, input_snapshot_json]); },
        guard202(commandId: string, value: "batch-export-version", batchId: string, companyId: string, version: number) { return assertion(database, commandId, value, ppRd21IdCompanyStatusVersionGuard('hr_payment_batches', 'approved'), [batchId, companyId, version]); },
        guard203(commandId: string, value: "batch-export-items", batchId: string, eligibleStatus: "failed" | "outstanding", length: number) { return assertion(database, commandId, value, "(SELECT COUNT(*) FROM hr_payment_batch_items WHERE batch_id=? AND status=?)=?", [batchId, eligibleStatus, length]); },
        guard204(commandId: string, value: "posted-payroll-source", source_id: string, companyId: string) { return assertion(database, commandId, value, PP_RD23_POSTED_PAYROLL_RUN_GUARD, [source_id, companyId]); },
        stageExportPaymentBatchV2Effect(value: string, companyId: string, batchId: string, version: number, fileKey: string, checksum: string, length: number, total: number, userId: string, now: number) { return nativePpStatement('people_payroll_stmt_41').bind(value, companyId, batchId, version, fileKey, checksum, length, total, userId, now); },
        stageExportPaymentBatchV2Effect2(fileKey: string, checksum: string, now: number, now2: number, batchId: string, companyId: string, version: number) { return nativePpStatement('people_payroll_stmt_42').bind(fileKey, checksum, now, now2, batchId, companyId, version); },
        readConfirmPaymentBatchBatch(batchId: string, companyId: string) {
            return database.prepare(ppRd32TwoColProjection("id,status,version,source_type,source_id,currency,bank_account_id","hr_payment_batches","id","company_id")).bind(batchId, companyId).first<{
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
            return database.prepare(ppRd36SinglePred("id,reference,status,amount_minor,engagement_id", "hr_payment_batch_items", "batch_id")).bind(batchId).all<{
                id: string;
                reference: string;
                status: string;
                amount_minor: number;
                engagement_id: string | null;
            }>();
        },
        guard209(commandId: string, value: "payment-batch-version", batchId: string, companyId: string, version: number, status: string) { return assertion(database, commandId, value, ppRd26IdCompanyVersionStatusGuard('hr_payment_batches'), [batchId, companyId, version, status]); },
        readConfirmPaymentBatchPosting(...args) { return payrollFinanceFacts(database).readConfirmPaymentBatchPosting(...args); },
        readConfirmPaymentBatchPayables(...args) { return payrollFinanceFacts(database).readConfirmPaymentBatchPayables(...args); },
        readConfirmPaymentBatchBankAccount(...args) { return payrollFinanceFacts(database).readConfirmPaymentBatchBankAccount(...args); },
        readConfirmPaymentBatchTransaction(...args) { return payrollFinanceFacts(database).readConfirmPaymentBatchTransaction(...args); },
        readConfirmPaymentBatchPeriods(...args) { return payrollFinanceFacts(database).readConfirmPaymentBatchPeriods(...args); },
        readConfirmPaymentBatchPayrollItem(source_id: string, engagement_id: string | null, amount_minor: number) { return database.prepare(ppRd36ThreePred("id,gross_minor,deductions_minor,employer_cost_minor,net_minor,input_snapshot_json,result_json", "hr_payroll_items", "run_id", "engagement_id", "net_minor")).bind(source_id, engagement_id, amount_minor).first<PayrollPostingItem>(); },
        guard216(...args) { return payrollFinanceFacts(database).guard216(...args); },
        stageConfirmPaymentBatchEffect(value: string, companyId: string, batchId: string, id: string, id2: string, id3: string, userId: string, now: number) { return nativePpStatement('people_payroll_stmt_43').bind(value, companyId, batchId, id, id2, id3, userId, now); },
        stageConfirmPaymentBatchEffect2(...args) { return payrollFinanceFacts(database).stageConfirmPaymentBatchEffect2(...args); },
        stageConfirmPaymentBatchEffect3(value: "failed" | "settled", value2: string | null, value3: number | null, batchId: string, reference: string) { return nativePpStatement('people_payroll_stmt_44').bind(value, value2, value3, batchId, reference); },
        stageConfirmPaymentBatchEffect4(status: "partially_settled" | "settled", now: number, batchId: string, companyId: string) { return nativePpStatement('people_payroll_stmt_45').bind(status, now, batchId, companyId); },
        readGetPaymentFileBatch(batchId: string, companyId: string) {
            return database.prepare("SELECT file_key,file_checksum FROM hr_payment_batches WHERE id=? AND company_id=? AND status IN ('exported','partially_settled','settled')").bind(batchId, companyId).first<{
                file_key: string | null;
                file_checksum: string | null;
            }>();
        } };
    return { repository, pagedRuns:(companyId,cursor)=>readListPage<Record<string,string|number|null>&{id:string;page_key:string}>(database,{sql:ppRd36SinglePred("*,pay_date||':'||created_at::text page_key", "hr_payroll_runs", "company_id"),args:[companyId],cursor,descending:true,scope:[companyId,'payroll']}), clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, hash: sha256, authorize: requirePermission, prepare: async (holding) => { const resources = await holdingResources(holding); database = provided ?? resources.write; files = resources.files; }, commands: { replay: (...args) => replayCommand(database, ...args), commit: (...args) => executeCommand(database, ...args) }, environment: async () => (await runtimeEnv()).ERPX_ENVIRONMENT, simulationAdapterId: thaiHotelPayrollMockId, files: { put: (...args) => files.put(...args), get: (...args) => files.get(...args) }, organizationGuard: (...args) => fromNative(() => organizationGuard(database, ...args)), validateStaffOrganization: (...args) => fromNative(() => validateStaffOrganization(database, ...args)), prepareJournal: (...args) => fromNative(() => prepareJournal(database, ...args)), assertPayrollSourceFresh: (...args) => fromNative(() => assertPayrollSourceFresh(database, ...args)), payrollSourceGuards: (...args) => fromNative(() => payrollSourceGuards(database, ...args)), prepareAttachmentBinding: (...args) => fromNative(() => prepareAttachmentBinding(database, ...args)), snapshotMoney: (...args) => fromNative(() => snapshotMoney(database, ...args)), assertStatutoryRunReady: (...args) => fromNative(() => assertStatutoryRunReady(database, ...args)), thaiEvidenceForEmployee: (...args) => fromNative(() => thaiEvidenceForEmployee(database, ...args)), thaiEvidenceGuards: (...args) => thaiEvidenceGuards(database, ...args), thaiPackForRun: (...args) => fromNative(() => thaiPackForRun(database, ...args)), preparePayslipIssue: (...args) => fromNative(() => preparePayslipIssue(database, ...args)), assertPayrollRecoveryDeductionLimits: (...args) => fromNative(() => assertPayrollRecoveryDeductionLimits(database, ...args)), preparePayrollDeductionConsumption: (...args) => fromNative(() => preparePayrollDeductionConsumption(database, ...args)), directoryPerson: (actor, id, engagementId) => fromNative(() => directoryPerson(actor, id, engagementId, database)), canReadCompensation: canReadCompensation, canManagePeople: canManagePeople, recognizedThaiPack: recognizedThaiPack, calculateThaiHotelPayrollFixture: calculateThaiHotelPayrollFixture, isThaiHotelPayrollMockEnabled: isThaiHotelPayrollMockEnabled };
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
export async function payrollCostAssignments(actor: WorkContext, engagementId: string) { return withPayrollPurchasedRead(actor, tx => invoke(() => createPeoplePayrollApplication(context(tx)).payrollCostAssignments(actor, engagementId))); }
export async function verifyPayrollCostAssignment(actor: WorkContext, assignmentId: string, input: z.infer<typeof payrollCostVerificationSchema>, commandId: string) { return withPayrollPurchasedCommand(actor, { key: commandId, operation: "people.cost-assignment.verify", purpose: "settle-existing" }, tx => invoke(() => createPeoplePayrollApplication(context(tx)).verifyPayrollCostAssignment(actor, assignmentId, input, commandId))); }
export async function listPeople(actor: WorkContext, filters: {
    levelId?: string;
    departmentId?: string;
} = {}) { return withPeoplePurchasedRead(actor,"people.directory",(tx,current)=>invoke(()=>createPeoplePayrollApplication(context(tx)).listPeople(current,filters)),"people.manage"); }
export async function getPerson(actor: WorkContext, personId: string, engagementId?: string) { return withPeoplePurchasedRead(actor,"people.directory",async(tx,current)=>{const data=await invoke(()=>createPeoplePayrollApplication(context(tx)).getPerson(current,personId,engagementId)),workforcePurchase=await workforcePurchaseControls(current,tx);return {...data,...(workforcePurchase?{workforcePurchase}:{})};}); }
export async function peopleSummary(actor: WorkContext) { return withPeoplePurchasedRead(actor,"people.directory",async(tx,current)=>{const purchase=await peopleOverviewPurchases(tx,current);return invoke(()=>createPeoplePayrollApplication(context(tx)).peopleSummary(purchase["people.payroll"].history?current:{...current,iam:current.iam?{...current.iam,permissions:current.iam.permissions.filter(permission=>permission!=="payroll.read")}:current.iam}));}); }
export async function createPerson(actor: WorkContext, input: z.infer<typeof createPersonSchema>, id: string) { return withPeoplePurchasedCommand(actor,{key:id,operation:"people.person.create",capability:"people.directory",purpose:"new-work",permission:"people.manage",resultKind:"person"},tx=>invoke(()=>createPeoplePayrollApplication(context(tx)).createPerson(actor, input, id))); }
export async function createEngagement(actor: WorkContext, input: z.infer<typeof createEngagementSchema>, id: string) { return withPeoplePurchasedCommand(actor,{key:id,operation:"people.engagement.create",capability:"people.lifecycle",purpose:"new-work",permission:"people.manage",resultKind:"engagement",target:{kind:"person",id:input.personId}},tx=>invoke(()=>createPeoplePayrollApplication(context(tx)).createEngagement(actor, input, id))); }
export async function createCompensation(actor: WorkContext, input: z.infer<typeof compensationSchema>, key: string) { return withPeoplePurchasedCommand(actor,{key,operation:"people.compensation.create",capability:"people.directory",purpose:"new-work",permission:"people.manage",resultKind:"compensation",target:{kind:"engagement",id:input.engagementId}},tx=>invoke(()=>createPeoplePayrollApplication(context(tx)).createCompensation(actor, input, key))); }
export async function payrollOptions(actor: WorkContext) { return withPayrollPurchasedRead(actor, tx => invoke(() => createPeoplePayrollApplication(context(tx)).payrollOptions(actor))); }
export async function assignPayGroup(actor: WorkContext, engagementId: string, input: z.infer<typeof assignPayGroupSchema>, key: string) { return invoke(() => createPeoplePayrollApplication(context()).assignPayGroup(actor, engagementId, input, key)); }
export async function transitionCompensation(actor: WorkContext, compensationId: string, input: z.infer<typeof compensationTransitionSchema>, key: string) { return withPeoplePurchasedCommand(actor,{key,operation:"people.compensation.transition",capability:"people.directory",purpose:input.status==="approved"?"new-work":"settle-existing",permission:"people.compensation.approve",target:{kind:"compensation",id:compensationId}},tx=>invoke(()=>createPeoplePayrollApplication(context(tx)).transitionCompensation(actor, compensationId, input, key))); }
export async function transitionEngagement(actor: WorkContext, engagementId: string, input: z.infer<typeof engagementTransitionSchema>, key: string) { return withPeoplePurchasedCommand(actor,{key,operation:"people.engagement.transition",capability:"people.lifecycle",purpose:input.status==="active"?"new-work":"settle-existing",permission:"people.manage",target:{kind:"engagement",id:engagementId}},tx=>invoke(()=>createPeoplePayrollApplication(context(tx)).transitionEngagement(actor, engagementId, input, key))); }
export async function createTimeEntry(actor: WorkContext, input: z.infer<typeof timeEntrySchema>, key: string) { return withWorkforcePurchasedCommand(actor,{key,operation:"people.time.submit",purpose:"new-work",resultKind:"time"},tx=>invoke(() => createPeoplePayrollApplication(context(tx)).createTimeEntry(actor,input,key))); }
export async function updateTimeEntry(actor: WorkContext, entryId: string, input: z.infer<typeof timeEntryUpdateSchema>, key: string) { return withWorkforcePurchasedCommand(actor,{key,operation:"people.time.update",purpose:"new-work",target:{kind:"time",id:entryId}},tx=>invoke(() => createPeoplePayrollApplication(context(tx)).updateTimeEntry(actor,entryId,input,key))); }
export async function rejectTimeEntry(actor: WorkContext, entryId: string, input: z.infer<typeof timeEntryReviewSchema>, key: string) { return withWorkforcePurchasedCommand(actor,{key,operation:"people.time.reject",purpose:"settle-existing",permission:"people.time.manage",target:{kind:"time",id:entryId}},tx=>invoke(() => createPeoplePayrollApplication(context(tx)).rejectTimeEntry(actor,entryId,input,key))); }
export async function approveTimeEntry(actor: WorkContext, entryId: string, expectedVersion: number, key: string) { return withWorkforcePurchasedCommand(actor,{key,operation:"people.time.approve",purpose:"settle-existing",permission:"people.time.manage",target:{kind:"time",id:entryId}},tx=>invoke(() => createPeoplePayrollApplication(context(tx)).approveTimeEntry(actor,entryId,expectedVersion,key))); }
export async function createPayGroup(actor: WorkContext, input: z.infer<typeof payGroupSchema>, id: string) { return withPayrollPurchasedCommand(actor, { key: id, operation: "people.pay-group.create", purpose: "new-work" }, tx => invoke(() => createPeoplePayrollApplication(context(tx)).createPayGroup(actor, input, id))); }
export async function createCountryPack(actor: WorkContext, input: z.infer<typeof countryPackSchema>, id: string) { return withPayrollPurchasedCommand(actor, { key: id, operation: "people.country-pack.create", purpose: "new-work" }, tx => invoke(() => createPeoplePayrollApplication(context(tx)).createCountryPack(actor, input, id))); }
export async function listCountryPacks(actor: WorkContext) { return withPayrollPurchasedRead(actor, tx => invoke(() => createPeoplePayrollApplication(context(tx)).listCountryPacks(actor))); }
export async function transitionCountryPack(actor: WorkContext, packId: string, input: z.infer<typeof countryPackTransitionSchema>, commandId: string) { return withPayrollPurchasedCommand(actor, {key: commandId, operation: `people.country-pack.${input.status}`, purpose: "settle-existing"}, tx => invoke(() => createPeoplePayrollApplication(context(tx)).transitionCountryPack(actor, packId, input, commandId))); }
export async function prepareFinalPay(actor: WorkContext, input: z.infer<typeof finalPayPreparationSchema>, key: string) { return withPayrollPurchasedCommand(actor, { key: key, operation: "people.payroll.final-pay.prepare", purpose: "settle-existing" }, tx => invoke(() => createPeoplePayrollApplication(context(tx)).prepareFinalPay(actor, input, key))); }
export async function requestPayrollAdvance(actor: WorkContext, input: z.infer<typeof payrollAdvanceRequestSchema>, key: string) { return withPayrollPurchasedCommand(actor, { key: key, operation: "people.payroll.advance.request", purpose: "new-work" }, tx => invoke(() => createPeoplePayrollApplication(context(tx, actor)).requestPayrollAdvance(actor, input, key))); }
export async function decidePayrollAdvance(actor: WorkContext, advanceId: string, input: z.infer<typeof payrollAdvanceDecisionSchema>, key: string) { return withPayrollPurchasedCommand(actor, { key: key, operation: `people.payroll.advance.${input.action}`, purpose: "settle-existing" }, tx => invoke(() => createPeoplePayrollApplication(context(tx)).decidePayrollAdvance(actor, advanceId, input, key))); }
export async function payrollAdvancePage(actor: WorkContext) { return withPayrollPurchasedRead(actor, tx => invoke(() => createPeoplePayrollApplication(context(tx)).payrollAdvancePage(actor))); }
export async function payrollAdvanceAlertTarget(db: SqlDatabase, companyId: string, userId: string, advanceId: string, now = Date.now()) { return invoke(() => createPeoplePayrollApplication(context(db)).payrollAdvanceAlertTarget(companyId, userId, advanceId, now)); }
export async function payrollPostingOptions(actor: WorkContext) { return withPayrollPurchasedRead(actor, tx => invoke(() => createPeoplePayrollApplication(context(tx)).payrollPostingOptions(actor))); }
export async function savePayrollAccountMapping(actor: WorkContext, payGroupId: string, input: z.infer<typeof payrollAccountMappingSchema>, key: string) { return withPayrollPurchasedCommand(actor, { key: key, operation: "people.payroll.mapping.save", purpose: "new-work" }, tx => invoke(() => createPeoplePayrollApplication(context(tx)).savePayrollAccountMapping(actor, payGroupId, input, key))); }
export async function approvePayrollAccountMapping(actor: WorkContext, mappingId: string, expectedVersion: number, key: string) { return withPayrollPurchasedCommand(actor, { key: key, operation: "people.payroll.mapping.approve", purpose: "settle-existing" }, tx => invoke(() => createPeoplePayrollApplication(context(tx)).approvePayrollAccountMapping(actor, mappingId, expectedVersion, key))); }
export async function previewPayrollAccounting(actor: WorkContext, runId: string, input: z.infer<typeof payrollPostSchema>) { return withPayrollPurchasedRead(actor, tx => invoke(() => createPeoplePayrollApplication(context(tx)).previewPayrollAccounting(actor, runId, input))); }
export async function approvePayrollRun(actor: WorkContext, runId: string, expectedVersion: number, key: string) { return withPayrollPurchasedCommand(actor, { key: key, operation: "people.payroll.approve", purpose: "settle-existing" }, tx => invoke(() => createPeoplePayrollApplication(context(tx)).approvePayrollRun(actor, runId, expectedVersion, key))); }
export async function requestPayrollCorrection(actor: WorkContext, runId: string, input: z.infer<typeof payrollCorrectionRequestSchema>, key: string) { return withPayrollPurchasedCommand(actor, { key: key, operation: "people.payroll.correction.request", purpose: "settle-existing" }, tx => invoke(() => createPeoplePayrollApplication(context(tx, actor)).requestPayrollCorrection(actor, runId, input, key))); }
export async function decidePayrollCorrection(actor: WorkContext, correctionId: string, input: z.infer<typeof payrollCorrectionDecisionSchema>, key: string) { return withPayrollPurchasedCommand(actor, { key: key, operation: `people.payroll.correction.${input.action}`, purpose: "settle-existing" }, tx => invoke(() => createPeoplePayrollApplication(context(tx)).decidePayrollCorrection(actor, correctionId, input, key))); }
export async function payrollCorrectionAlertTarget(db: SqlDatabase, companyId: string, userId: string, correctionId: string, now = Date.now()) { return invoke(() => createPeoplePayrollApplication(context(db)).payrollCorrectionAlertTarget(companyId, userId, correctionId, now)); }
export async function exportPayrollReview(actor: WorkContext, runId: string, key: string) { return withPayrollPurchasedCommand(actor, { key: key, operation: "people.payroll.review-export", purpose: "history" }, tx => invoke(() => createPeoplePayrollApplication(context(tx)).exportPayrollReview(actor, runId, key))); }
export async function getPayrollReviewFile(actor: WorkContext, runId: string, revision: number) { return withPayrollPurchasedRead(actor, tx => invoke(() => createPeoplePayrollApplication(context(tx)).getPayrollReviewFile(actor, runId, revision))); }
export async function createPaymentBatch(actor: WorkContext, input: z.infer<typeof paymentBatchSchema>, commandId: string) { return input.sourceType === "payroll" ? withPayrollPurchasedCommand(actor, { key: commandId, operation: "people.payment-batch.create", purpose: "settle-existing" }, tx => invoke(() => createPeoplePayrollApplication(context(tx)).createPaymentBatch(actor, input, commandId))) : invoke(() => createPeoplePayrollApplication(context()).createPaymentBatch(actor, input, commandId)); }
export async function preparePaymentRetry(actor: WorkContext, batchId: string, expectedVersion: number, commandId: string) { return withPayrollPaymentCommand(actor, batchId, { key: commandId, operation: "people.payment-batch.retry", purpose: "settle-existing" }, tx => invoke(() => createPeoplePayrollApplication(context(tx)).preparePaymentRetry(actor, batchId, expectedVersion, commandId))); }
export async function approvePaymentBatch(actor: WorkContext, batchId: string, expectedVersion: number, commandId: string) { return withPayrollPaymentCommand(actor, batchId, { key: commandId, operation: "people.payment-batch.approve", purpose: "settle-existing" }, tx => invoke(() => createPeoplePayrollApplication(context(tx)).approvePaymentBatch(actor, batchId, expectedVersion, commandId))); }
export async function exportPaymentBatch(actor: WorkContext, batchId: string, commandId: string) { return withPayrollPaymentCommand(actor, batchId, { key: commandId, operation: "people.payment-batch.export", purpose: "settle-existing" }, tx => invoke(() => createPeoplePayrollApplication(context(tx)).exportPaymentBatch(actor, batchId, commandId))); }
export async function confirmPaymentBatch(actor: WorkContext, batchId: string, input: z.infer<typeof paymentConfirmationSchema>, commandId: string) { return withPayrollPaymentCommand(actor, batchId, { key: commandId, operation: "people.payment-batch.confirm", purpose: "settle-existing" }, tx => invoke(() => createPeoplePayrollApplication(context(tx)).confirmPaymentBatch(actor, batchId, input, commandId))); }
export async function importSyntheticPaymentResults(actor: WorkContext, batchId: string, csv: string, commandId: string) { return withPayrollPaymentCommand(actor, batchId, { key: commandId, operation: "people.payment-batch.confirm", purpose: "settle-existing" }, tx => invoke(() => createPeoplePayrollApplication(context(tx)).importSyntheticPaymentResults(actor, batchId, csv, commandId))); }
export async function getPaymentFile(actor: WorkContext, batchId: string) { return withPayrollPaymentRead(actor, batchId, tx => invoke(() => createPeoplePayrollApplication(context(tx)).getPaymentFile(actor, batchId))); }
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
export async function payrollRunsPage(actor:WorkContext,cursor?:string){return withPayrollPurchasedRead(actor,tx=>invoke(()=>createPeoplePayrollApplication(context(tx)).payrollRunsPage(actor,cursor)));}
function scopeWhere(actor: WorkContext, alias = "p") {
    if (actor.departmentId)
        return { sql: `${alias}.company_id=? AND ${alias}.department_id=?`, args: [actor.companyId, actor.departmentId] };
    if (actor.branchId)
        return { sql: `${alias}.company_id=? AND ${alias}.branch_id=?`, args: [actor.companyId, actor.branchId] };
    return { sql: `${alias}.company_id=?`, args: [actor.companyId] };
}

/** Accepted-hire prerequisite only. The original application retains its SQL,
 * payroll.prepare check, native identity and result. Its original receipts bind
 * this complete source envelope, including all original pay-group fields. */
export async function createHireHandoffPayGroup(actor:WorkContext,handoffId:string,raw:unknown,key:string):Promise<HireHandoffPayGroupResult>{
 const input=hireHandoffPayGroupSchema.parse(raw),envelope=hireHandoffPayGroupEnvelope(handoffId,input);
 const result=await withHireHandoffPayGroupCommand(actor,handoffId,input,key,async database=>{
  const ports=context(database),commands=ports.commands;
  ports.hash=value=>sha256(JSON.stringify([envelope,value]));
  ports.commands={
   replay<Result>(current:WorkContext,id:string,operation:string,recordId:string){if(id!==key||operation!=='people.pay-group.create')throw new ApiError(503,'HIRE_PAY_GROUP_COMPOSITION_REQUIRED','Use the original accepted-hire pay-group operation.');return commands.replay<Result>(current,id,operation,recordId,envelope);},
   commit<Result>(current:WorkContext,id:string,operation:string,_value:unknown,recordId:string,version:number,result:Result,effects:SqlStatement[]){if(id!==key||operation!=='people.pay-group.create')throw new ApiError(503,'HIRE_PAY_GROUP_COMPOSITION_REQUIRED','Use the original accepted-hire pay-group operation.');return commands.commit<Result>(current,id,operation,envelope,recordId,version,result,effects);},
  };
  return invoke(()=>createPeoplePayrollApplication(ports).createPayGroup(actor,input.input,key));
 });
 return {id:result.id,handoffId,status:'active',currency:input.input.currency};
}
