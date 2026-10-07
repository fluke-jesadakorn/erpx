import type {ListPage} from "../../work_projection/public/list-page";
import { payrollAccountingLines, payrollNetCostShares, type PayrollPostingItem } from '../domain/payroll-accounting';
import { z } from 'zod';
import { hasPermission } from "@/lib/permissions";
import { samePerson } from "@/lib/iam";
import { decimalToMinor, minorToDecimal } from "@/lib/finance-money";
import { calculatePayrollAmounts } from "@/lib/payroll-math";
import { parseSyntheticPayrollBankResultCsv } from "@/lib/payroll-bank-result";
import type { organizationGuard, validateStaffOrganization } from "@/server/modules/organization/public/organization";
import type { calculateThaiHotelPayrollFixture, isThaiHotelPayrollMockEnabled } from "@/server/modules/runtime/public/simulation-only-adapters";
import type { directoryPerson, canReadCompensation, canManagePeople } from "../public/people-directory";
import type { prepareJournal } from "@/server/modules/finance/public/finance";
import type { assertPayrollSourceFresh, payrollSourceGuards } from "../public/payroll-readiness";
import type { prepareAttachmentBinding } from "@/server/modules/files/public/attachments/service";
import type { snapshotMoney } from "@/server/modules/finance/public/company-money";
import type { assertStatutoryRunReady, recognizedThaiPack, thaiEvidenceForEmployee, thaiEvidenceGuards, thaiPackForRun, PayrollStatutoryRun } from "../public/payroll-statutory";
import type { preparePayslipIssue } from "../public/payroll-payslips";
import type { calculatePayrollAdjustmentRun } from "../public/payroll-adjustments";
import type { assertPayrollRecoveryDeductionLimits } from "../public/payroll-recoveries";
import type { preparePayrollDeductionConsumption } from "../public/payroll-deductions";
import type { WorkContext } from '../../identity_access/public/authorization';
import { BusinessRuleError, type Clock, type IdentifierSource } from '../../shared';
import type { PayrollStatutoryCommands } from './payroll-statutory';
import type { StreamedFileContent } from '../../files/public/streamed-file-contract';
import { applyThaiStatutory } from '../domain/payroll-statutory';
import { payrollCsvCell, createPersonSchema, createEngagementSchema, compensationSchema, compensationTransitionSchema, engagementTransitionSchema, payrollCostVerificationSchema, payGroupSchema, countryPackSchema, countryPackTransitionSchema, payrollRunSchema, paymentBatchSchema, timeEntrySchema, paymentConfirmationSchema, payrollPostSchema, payrollAccountMappingSchema, payrollCorrectionRequestSchema, payrollCorrectionDecisionSchema, finalPayPreparationSchema, payrollAdvanceRequestSchema, payrollAdvanceDecisionSchema, assignPayGroupSchema, timeEntryUpdateSchema, timeEntryReviewSchema, advancePeriodStart, advancePeriodEnd, type ResolvedPayrollPost } from '../domain/people-payroll';
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
type PortResult<Value, Effect> = Value extends Promise<infer Result> ? Promise<PortResult<Result, Effect>> : Value extends {
    bind(...args: never[]): unknown;
    run(): unknown;
} ? Effect : Value extends (...args: never[]) => unknown ? Value : Value extends readonly unknown[] ? {
    [Index in keyof Value]: PortResult<Value[Index], Effect>;
} : Value extends object ? {
    [Key in keyof Value]: PortResult<Value[Key], Effect>;
} : Value;
export interface PeoplePayrollRepository<Effect> {
    readPayrollCostAssignmentsEngagement(engagementId: string, companyId: string): Promise<Record<string, unknown> | null>;
    readPayrollCostAssignmentsEffect(engagementId: string, companyId: string): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readVerifyPayrollCostAssignmentAssignment(assignmentId: string, companyId: string): Promise<{
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
    } | null>;
    readVerifyPayrollCostAssignmentHistory(engagement_id: string, companyId: string): Promise<Record<string, unknown> | null>;
    guard5(commandId: string, value: "assignment-version", assignmentId: string, companyId: string, expectedVersion: number): Effect;
    stageVerifyPayrollCostAssignmentEffect(now: number, assignmentId: string, companyId: string, expectedVersion: number): Effect;
    stageVerifyPayrollCostAssignmentEffect2(value: string, companyId: string, assignmentId: string, userId: string, evidenceRef: string, expectedVersion: number, now: number): Effect;
    stageVerifyPayrollCostAssignmentEffect3(assignmentId: string): Effect;
    stageVerifyPayrollCostAssignmentEffect4(now: number, assignmentId: string, companyId: string): Effect;
    readCommandCurrent(id: string): Promise<{
        record_id: string;
    } | null>;
    readCommandOld(id: string): Promise<Record<string, unknown> | null>;
    stageReceiptEffect(id: string, operation: string, digest: string, value: string, now: number): Effect;
    readListPeopleRows(companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null, value: string | null, value2: string | null, value3: string | null, value4: string | null, value5: string | null): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readGetPersonCompensation(engagement_id: string | number | null): Promise<Record<string, unknown> | null>;
    readGetPersonLeavetime(personId: string, companyId: string, branchId: string | null, branchId2: string | null): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readGetPersonLeavetime2(engagement_id: string | number | null): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readGetPersonRevisions(engagement_id: string | number | null): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readGetPersonRecommendedPay(companyId: string, position_id: string | number): Promise<{
        currency: string;
        monthly_amount_minor: number;
        version: number;
    } | null>;
    readGetPersonRecommendedLeave(companyId: string, level_id: string | number): Promise<{
        name: string;
        version: number;
        annual_days: number;
        sick_days: number;
    } | null>;
    readGetPersonLeaveAssignment(companyId: string, engagement_id: string | number): Promise<{
        name: string;
        version: number;
        effective_from: string;
        effective_to: string | null;
    } | null>;
    readPeopleSummaryPeopleactivecontractorsrunspending(actor: WorkContext): Promise<{
        count: number;
    } | null>;
    readPeopleSummaryPeopleactivecontractorsrunspending2(actor: WorkContext): Promise<{
        count: number;
    } | null>;
    readPeopleSummaryPeopleactivecontractorsrunspending3(actor: WorkContext): Promise<{
        count: number;
    } | null>;
    readPeopleSummaryPeopleactivecontractorsrunspending4(companyId: string): Promise<{
        count: number;
    } | null>;
    readPeopleSummaryPeopleactivecontractorsrunspending5(actor: WorkContext, value: 0 | 1, userId: string): Promise<{
        count: number;
    } | null>;
    readPayrollSummaryEffect(companyId: string, value: string | null, value2: string | null): Promise<{
        results: ({
            pay_month: string;
            currency: string;
            run_count: number;
            headcount: number;
            gross_minor: number;
            deductions_minor: number;
            employer_cost_minor: number;
            net_minor: number;
        })[];
    }>;
    stageCreatePersonEffect(personId: string, value: null, companyId: string, legalName: string, value2: string | null, value3: string | null, value4: string | null, countryCode: string, userId: string, now: number, now2: number): Effect;
    readCreateEngagementPerson(personId: string, companyId: string): Promise<Record<string, unknown> | null>;
    stageCreateEngagementEffect(engagementId: string, personId: string, companyId: string, value: string | null, value2: string | null, workerType: "employee" | "contractor", employmentType: "contractor" | "salaried" | "daily" | "hourly" | "shift", value3: "onboarding", startDate: string, value4: string | null, value5: string | null, jobTitle: string, currency: string, value6: string | null, userId: string, now: number, now2: number): Effect;
    readCreateCompensationE(engagementId: string, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null): Promise<Record<string, string | number | null> | null>;
    guard31(key: string, value: "engagement", engagementId: string, version: string | number | null, currency: string): Effect;
    stageCreateCompensationEffect(id: string, engagementId: string, effectiveFrom: string, value: string | null, payFrequency: "weekly" | "fortnightly" | "semimonthly" | "monthly" | "off_cycle", base: number, hourly: number, daily: number, standardDailyHours: number, expectedHours: number, currency: string, value2: string, userId: string, now: number, now2: number): Effect;
    readPayrollOptionsEffect(companyId: string): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readAssignPayGroupMissing(engagementId: string, companyId: string): Promise<Record<string, unknown> | null>;
    guard35(key: string, value: "engagement", engagementId: string, companyId: string, expectedVersion: number, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null): Effect;
    guard36(key: string, value: "active-level", engagementId: string): Effect;
    guard37(key: string, value: "pay-group", payGroupId: string | null, engagementId: string, payGroupId2: string | null): Effect;
    stageAssignPayGroupEffect(payGroupId: string | null, now: number, engagementId: string): Effect;
    readTransitionCompensationEffect(compensationId: string, companyId: string): Promise<Record<string, unknown> | null>;
    readTransitionCompensationRow(compensationId: string, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null): Promise<Record<string, string | number | null> | null>;
    guard41(key: string, value: "revision", compensationId: string, expectedVersion: number, value2: "approved" | "draft"): Effect;
    guard42(key: string, value: "overlap", engagement_id: string | number | null, compensationId: string, effective_to: string | number | null, effective_from: string | number | null): Effect;
    stageTransitionCompensationEffect(status: "approved" | "superseded", userId: string, now: number, compensationId: string): Effect;
    stageTransitionCompensationEffect2(value: string, key: string, compensationId: string, value2: string, userId: string, assignmentId: string, expectedVersion: number, value3: string, now: number): Effect;
    readTransitionEngagementRow(engagementId: string, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null): Promise<{
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
    } | null>;
    guard46(key: string, value: "engagement", engagementId: string, companyId: string, expectedVersion: number, status: string): Effect;
    guard47(key: string, value: "active-level", engagementId: string): Effect;
    stageTransitionEngagementEffect(status: "active" | "suspended" | "ended", status2: "active" | "suspended" | "ended", value: string | null, now: number, engagementId: string): Effect;
    stageTransitionEngagementEffect2(person_id: string, person_id2: string): Effect;
    stageTransitionEngagementEffect3(now: number, user_id: string, companyId: string, person_id: string): Effect;
    readCreateTimeEntryE(engagementId: string, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null): Promise<Record<string, string | number | null> | null>;
    guard52(key: string, value: "engagement", engagementId: string, version: string | number | null): Effect;
    stageCreateTimeEntryEffect(id: string, engagementId: string, companyId: string, workDate: string, regularHours: number, overtimeHours: number, note: string, userId: string, now: number, now2: number): Effect;
    readUpdateTimeEntryEntry(entryId: string, companyId: string, userId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null): Promise<Record<string, string | number | null> | null>;
    guard55(key: string, value: "editable-time", entryId: string, expectedVersion: number, userId: string): Effect;
    stageUpdateTimeEntryEffect(workDate: string, regularHours: number, overtimeHours: number, note: string, now: number, entryId: string): Effect;
    guard57(key: string, value: "reviewable-time", entryId: string, companyId: string, expectedVersion: number, userId: string, userId2: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null): Effect;
    stageRejectTimeEntryEffect(reason: string, now: number, entryId: string): Effect;
    stageRejectTimeEntryEffect2(value: string, key: string, entryId: string, value2: "people.time.reject", userId: string, assignmentId: string, expectedVersion: number, value3: string, now: number): Effect;
    guard60(key: string, value: "time", entryId: string, companyId: string, expectedVersion: number, userId: string, userId2: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null): Effect;
    stageApproveTimeEntryEffect(userId: string, now: number, entryId: string): Effect;
    stageApproveTimeEntryEffect2(value: string, key: string, entryId: string, value2: "people.time.approve", userId: string, assignmentId: string, expectedVersion: number, key2: string, now: number): Effect;
    stageCreatePayGroupEffect(groupId: string, companyId: string, name: string, countryCode: string, currency: string, frequency: "weekly" | "fortnightly" | "semimonthly" | "monthly", timezone: string, overtimeMultiplierBps: number, prorationMethod: "calendar_days" | "none", userId: string, now: number, now2: number): Effect;
    stageCreateCountryPackEffect(packId: string, countryCode: string, version: string, value: "testing", value2: string, value3: string, userId: string, now: number, now2: number, value4: string | null, value5: string | null, value6: string | null, value7: string | null): Effect;
    readListCountryPacksEffect(): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readTransitionCountryPackPack(packId: string): Promise<{
        status: string;
        country_code: string;
        version: string;
        rules_json: string;
        created_by: string;
        record_version: number;
        source_digest: string | null;
        adapter_id: string | null;
    } | null>;
    guard67(commandId: string, value: "pack-status", packId: string, status: string, record_version: number, source_digest: string): Effect;
    stageTransitionCountryPackEffect(status: "superseded" | "active" | "validated", proof: string, userId: string, now: number, packId: string): Effect;
    stageTransitionCountryPackEffect2(value: string, commandId: string, packId: string, value2: string, userId: string, assignmentId: string, record_version: number, sourceDigest: string, now: number): Effect;
    readCalculateItemPayee(person_id: string, company_id: string): Promise<{
        id: string;
        version: number;
        legal_name: string;
        preferred_name: string | null;
    } | null>;
    readCalculateItemCompensation(id: string, period_end: string, period_start: string): Promise<Record<string, string | number | null> | null>;
    readCalculateItemTime(id: string, period_start: string, period_end: string): Promise<{
        results: ({
            id: string;
            version: number;
            work_date: string;
            regular_hours: number;
            overtime_hours: number;
            cost_assignment_id: string | null;
            cost_branch_id: string | null;
            cost_department_id: string | null;
        })[];
    }>;
    readCalculateItemHandoffs(id: string, period_start: string, period_end: string): Promise<{
        results: ({
            id: string;
            request_id: string;
            request_version: number;
            minutes: number;
            work_date: string;
        })[];
    }>;
    readCalculateItemBenefitInputs(id: string, currency: string, period_end: string, period_start: string): Promise<{
        results: ({
            id: string;
            plan_name: string;
            employee_deduction_minor: number;
            employer_cost_minor: number;
            source_election_version: number;
        })[];
    }>;
    readCalculateItemAdvanceRepayments(company_id: string, id: string, period_end: string, period_start: string, id2: string): Promise<{
        results: ({
            id: string;
            version: number;
            amount_minor: number;
            due_period_start: string;
            due_period_end: string;
            reason: string;
        })[];
    }>;
    readCalculateItemCostAssignments(id: string, company_id: string, activeEnd: string, activeStart: string): Promise<{
        results: ({
            id: string;
            effective_from: string;
            effective_to: string | null;
            branch_id: string | null;
            department_id: string | null;
            evidence_status: string;
            version: number;
        })[];
    }>;
    guard77(key: string, value: "pay-group", payGroupId: string, companyId: string, currency: string): Effect;
    guard78(key: string, value: "duplicate-period", companyId: string, payGroupId: string, periodEnd: string, periodStart: string): Effect;
    guard79(key: string, value: "statutory-pack", id: string, record_version: number, sourceDigest: string): Effect;
    stageCreatePayrollRunEffect(id: string, companyId: string, payGroupId: string, value: string | null, periodStart: string, periodEnd: string, payDate: string, currency: string, mode: "preview" | "production", userId: string, now: number, now2: number): Effect;
    readPrepareFinalPayTask(taskId: string, companyId: string): Promise<{
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
    } | null>;
    guard82(key: string, value: "task-version", taskId: string, companyId: string, expectedTaskVersion: number, userId: string): Effect;
    guard83(key: string, value: "engagement-pay-group", engagement_id: string, companyId: string, payGroupId: string, currency: string): Effect;
    guard84(key: string, value: "one-final-pay", taskId: string): Effect;
    guard85(key: string, value: "statutory-pack", id: string, record_version: number, sourceDigest: string): Effect;
    stagePrepareFinalPayEffect(runId: string, companyId: string, payGroupId: string, value: string | null, engagement_id: string, periodStart: string, periodEnd: string, payDate: string, currency: string, userId: string, now: number, now2: number): Effect;
    stagePrepareFinalPayEffect2(prepId: string, companyId: string, taskId: string, case_id: string, engagement_id: string, runId: string, reason: string, userId: string, now: number, now2: number): Effect;
    stagePrepareFinalPayEffect3(value: string, prepId: string, taskId: string, runId: string, companyId: string, userId: string, expectedTaskVersion: number, value2: string, now: number): Effect;
    readRequestPayrollAdvanceEngagement(engagementId: string, companyId: string): Promise<{
        id: string;
        company_id: string;
        currency: string;
        status: string;
        pay_group_id: string | null;
        person_id: string;
        user_id: string;
        legal_name: string;
    } | null>;
    guard90(key: string, value: "engagement", engagementId: string, companyId: string, currency: string): Effect;
    guard91(key: string, value: "no-pending-advance", engagementId: string): Effect;
    stageRequestPayrollAdvanceEffect(id: string, companyId: string, id2: string, person_id: string, amountMinor: number, installmentCount: number, firstDeductionPeriod: string, currency: string, reason: string, userId: string, now: number, now2: number): Effect;
    stageRequestPayrollAdvanceEffect2(value: string, id: string, companyId: string, userId: string, value2: 1, value3: string, now: number): Effect;
    stageRequestPayrollAdvanceEffect3(id: string, companyId: string, id2: string, now: number, companyId2: string, now2: number, userId: string): Effect;
    readDecidePayrollAdvanceRow(advanceId: string, companyId: string): Promise<Record<string, string | number | null> | null>;
    guard96(key: string, value: "advance-version", advanceId: string, companyId: string, expectedVersion: number): Effect;
    stageDecidePayrollAdvanceStatements(status: "approved" | "rejected" | "cancelled", status2: "approved" | "rejected" | "cancelled", userId: string, reason: string, now: number, now2: number, advanceId: string): Effect;
    stageDecidePayrollAdvanceStatements2(value: string, advanceId: string, companyId: string, value2: "approved" | "rejected" | "cancelled", userId: string, version: number, value3: string, now: number): Effect;
    stageDecidePayrollAdvanceStatements3(value: string, companyId: string, requested_by: string | number | null, value2: "payroll_advance", advanceId: string, version: number, value3: "Payroll advance approved; deductions will appear in payroll" | "Payroll advance decision recorded", now: number): Effect;
    stageDecidePayrollAdvanceEffect(value: string, advanceId: string, companyId: string, value2: number, value3: string, value4: string, value5: number, now: number): Effect;
    readPayrollAdvancePageRows(value: string[], privileged: boolean): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readPayrollAdvancePageOptions(value: string[], privileged: boolean): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readPayrollAdvanceAlertTargetEffect(advanceId: string, companyId: string, userId: string, userId2: string, now: number): Promise<string | null>;
    readCalculatePayrollRunRun(runId: string, companyId: string): Promise<{
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
    } | null>;
    readCalculatePayrollRunEngagements(companyId: string, pay_group_id: string, currency: string, period_end: string, period_start: string, value: unknown | null, value2: unknown | null): Promise<{
        results: ({
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
        })[];
    }>;
    readCalculatePayrollRunGroup(pay_group_id: string, companyId: string): Promise<{
        overtime_multiplier_bps: number;
        proration_method: "calendar_days" | "none";
    } | null>;
    guard107(key: string, value: "run", runId: string, version: number): Effect;
    guard108(key: string, value: string, engagementId: string, version: number): Effect;
    guard109(key: string, value: string, engagementId: string, period_start: string, period_end: string, length: number): Effect;
    guard110(key: string, value: string, engagementId: string, period_start: string, period_end: string, length: number): Effect;
    guard111(key: string, value: string, id: string, version: number): Effect;
    guard112(key: string, value: string, id: string, version: number, engagementId: string, companyId: string): Effect;
    guard113(key: string, value: string, id: string | number | null, version: string | number | null): Effect;
    stageCalculatePayrollRunEffect(runId: string): Effect;
    stageCalculatePayrollRunEffect2(value: string, digest: string, now: number, runId: string): Effect;
    stageCalculatePayrollRunEffect3(value: string, runId: string, engagementId: string, workerType: string, value2: string, value3: string, gross: number, deductions: number, employer: number, net: number, currency: string, now: number): Effect;
    readWorkPayrollRowsEffect(companyId: string): Promise<{
        results: (Record<string, string | number | null>)[];
    }>;
    readGetPayrollRunRun(runId: string, companyId: string): Promise<Record<string, unknown> | null>;
    readGetPayrollRunItemsexportscorrectionspaymentBatch(runId: string): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readGetPayrollRunItemsexportscorrectionspaymentBatch2(runId: string): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readGetPayrollRunItemsexportscorrectionspaymentBatch3(runId: string, companyId: string): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readGetPayrollRunItemsexportscorrectionspaymentBatch4(companyId: string, runId: string): Promise<Record<string, unknown> | null>;
    readGetPayrollRunPaymentItems(value: string): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readListPayrollRunsEffect(companyId: string): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readListPayrollRunsEffect2(companyId: string): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readPayrollPostingOptionsChartsperiodsmappingsgroups(companyId: string): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readPayrollPostingOptionsChartsperiodsmappingsgroups2(companyId: string): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readPayrollPostingOptionsChartsperiodsmappingsgroups3(companyId: string): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readPayrollPostingOptionsChartsperiodsmappingsgroups4(companyId: string): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readPayrollPostingOptionsAccounts(companyId: string, chartIds: string[]): Promise<{
        results: (Record<string, unknown>)[];
    }>;
    readResolvePayrollPostRun(runId: string, companyId: string): Promise<{
        pay_group_id: string;
        period_end: string;
    } | null>;
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
    }): Promise<{
        id: string;
        chart_id: string;
        accounts_json: string;
        version: number;
    } | null>;
    readResolvePayrollPostPeriods(companyId: string, dateHint: string, dateHint2: string): Promise<{
        results: ({
            id: string;
            start_date: string;
        })[];
    }>;
    readResolvePayrollPostNext(companyId: string, period_end: string): Promise<{
        results: ({
            id: string;
            start_date: string;
        })[];
    }>;
    readResolvePayrollPostEffect(companyId: string, entryDate: string, entryDate2: string): Promise<{
        results: ({
            id: string;
            start_date: string;
        })[];
    }>;
    readSavePayrollAccountMappingGroup(payGroupId: string, companyId: string): Promise<{
        id: string;
        currency: string;
    } | null>;
    readSavePayrollAccountMappingChart(chartId: string, companyId: string): Promise<{
        id: string;
        base_currency: string;
    } | null>;
    readSavePayrollAccountMappingAccounts(companyId: string, chartId: string, ids: string[]): Promise<{
        results: ({
            id: string;
            currency: string;
            category: string;
        })[];
    }>;
    stageSavePayrollAccountMappingEffect(id: string, companyId: string, payGroupId: string, chartId: string, value: string, userId: string, now: number, now2: number): Effect;
    readApprovePayrollAccountMappingRow(mappingId: string, companyId: string): Promise<{
        id: string;
        pay_group_id: string;
        created_by: string;
        status: string;
        version: number;
    } | null>;
    guard141(key: string, value: "mapping-review", mappingId: string, companyId: string, expectedVersion: number, userId: string): Effect;
    stageApprovePayrollAccountMappingEffect(now: number, companyId: string, pay_group_id: string): Effect;
    stageApprovePayrollAccountMappingEffect2(userId: string, now: number, mappingId: string): Effect;
    readPreparePayrollAccrualRun(runId: string, companyId: string): Promise<Record<string, string | number | null> | null>;
    readPreparePayrollAccrualItems(runId: string): Promise<{
        results: (PayrollPostingItem & {
            worker_type: string;
        })[];
    }>;
    readPreparePayrollAccrualValid(companyId: string, chartId: string, ids: string[]): Promise<{
        results: ({
            id: string;
            category: string;
            currency: string;
        })[];
    }>;
    guard147(key: string, value: "payroll-run", runId: string, companyId: string, expectedVersion: number): Effect;
    guard148(key: string, value: "payroll-mapping", resolvedMappingId: string, companyId: string, runId: string, resolvedMappingVersion: number | null): Effect;
    guard149(key: string, value: "payroll-items", runId: string): Effect;
    guard150(key: string, value: "payroll-posting-unique", runId: string): Effect;
    stagePostPayrollEffect(userId: string, now: number, runId: string, companyId: string, expectedVersion: number): Effect;
    stagePostPayrollEffect2(runId: string): Effect;
    stagePostPayrollEffect3(value: string, runId: string, id: string, resolvedMappingId: string | null, resolvedMappingVersion: number | null, now: number): Effect;
    readApprovePayrollRunSubmitted(runId: string, companyId: string): Promise<{
        submitted_by: string | null;
    } | null>;
    guard155(key: string, value: "run", runId: string, companyId: string, expectedVersion: number, userId: string): Effect;
    stageApprovePayrollRunEffect(userId: string, now: number, runId: string): Effect;
    stageApprovePayrollRunEffect2(value: string, key: string, runId: string, value2: "people.payroll.approve", userId: string, assignmentId: string, expectedVersion: number, now: number, runId2: string): Effect;
    readRequestPayrollCorrectionRun(runId: string, companyId: string): Promise<{
        id: string;
        status: string;
        version: number;
        revision: number;
        input_digest: string | null;
        totals_json: string;
    } | null>;
    guard159(key: string, value: "approved-run", runId: string, companyId: string, expectedVersion: number, revision: number, input_digest: string): Effect;
    stageRequestPayrollCorrectionEffect(id: string, runId: string, companyId: string, version: number, revision: number, input_digest: string, totals_json: string, reason: string, userId: string, now: number, now2: number): Effect;
    stageRequestPayrollCorrectionEffect2(value: string, id: string, runId: string, companyId: string, userId: string, version: number, value2: string, now: number): Effect;
    stageRequestPayrollCorrectionEffect3(id: string, companyId: string, id2: string, now: number, companyId2: string, now2: number, userId: string): Effect;
    readDecidePayrollCorrectionRow(correctionId: string, companyId: string): Promise<Record<string, string | number | null> | null>;
    guard164(key: string, value: "correction-version", correctionId: string, companyId: string, expectedVersion: number): Effect;
    guard165(key: string, value: "run-evidence", run_id: string | number | null, companyId: string, run_version: string | number | null, run_revision: string | number | null, input_digest: string | number | null): Effect;
    stageDecidePayrollCorrectionStatements(status: "approved" | "rejected" | "cancelled", userId: string, reason: string, now: number, now2: number, correctionId: string): Effect;
    stageDecidePayrollCorrectionStatements2(value: string, correctionId: string, run_id: string | number | null, companyId: string, status: "approved" | "rejected" | "cancelled", userId: string, run_version: string | number | null, value2: string, now: number): Effect;
    stageDecidePayrollCorrectionStatements3(value: string, companyId: string, requested_by: string | number | null, value2: "payroll_correction", correctionId: string, version: number, value3: "Payroll correction approved; recalculate the run with current inputs" | "Payroll correction rejected; review the decision reason" | "Payroll correction request cancelled", now: number): Effect;
    stageDecidePayrollCorrectionEffect(value: string, now: number, run_id: string | number | null): Effect;
    readPayrollCorrectionAlertTargetEffect(userId: string, correctionId: string, companyId: string, now: number): Promise<string | null>;
    readExportPayrollReviewRun(runId: string, companyId: string): Promise<{
        id: string;
        status: string;
        revision: number;
        input_digest: string | null;
        currency: string;
        period_start: string;
        period_end: string;
    } | null>;
    readExportPayrollReviewExisting(runId: string, revision: number): Promise<{
        checksum_sha256: string;
    } | null>;
    readExportPayrollReviewItems(runId: string): Promise<{
        results: ({
            legal_name: string;
            worker_type: string;
            gross_minor: number;
            deductions_minor: number;
            employer_cost_minor: number;
            net_minor: number;
            currency: string;
            input_snapshot_json: string;
        })[];
    }>;
    guard174(key: string, value: "run", runId: string, companyId: string, revision: number, input_digest: string): Effect;
    stageExportPayrollReviewEffect(exportId: string, runId: string, revision: number, fileKey: string, checksum: string, userId: string, now: number): Effect;
    readGetPayrollReviewFileRow(runId: string, revision: number, companyId: string): Promise<{
        file_key: string;
        checksum_sha256: string;
    } | null>;
    readCreatePaymentBatchDuplicate(companyId: string, sourceType: "contractor" | "payroll" | "expense", sourceId: string): Promise<Record<string, unknown> | null>;
    readCreatePaymentBatchSource(sourceId: string, companyId: string): Promise<{
        id: string;
        status: string;
        currency: string;
    } | null>;
    readCreatePaymentBatchPosted(sourceId: string): Promise<{
        results: ({
            id: string;
            engagement_id: string;
            net_minor: number;
            currency: string;
            input_snapshot_json: string;
        })[];
    }>;
    readCreatePaymentBatchInvoice(sourceId: string, companyId: string): Promise<{
        id: string;
        status: string;
        currency: string;
        total_minor: number;
        legal_name: string;
    } | null>;
    readCreatePaymentBatchBank(bankAccountId: string, companyId: string, currency: string): Promise<Record<string, unknown> | null>;
    guard182(commandId: string, value: "posted-payroll-source", sourceId: string, companyId: string): Effect;
    guard183(commandId: string, value: string, id: string, sourceId: string, net_minor: number, input_snapshot_json: string): Effect;
    stageCreatePaymentBatchStatements(batchId: string, companyId: string, sourceType: "contractor" | "payroll", sourceId: string, currency: string, value: "draft", formatCode: string, total: number, value2: string | null, userId: string, now: number, now2: number): Effect;
    guard185(commandId: string, value: "supplier-payment-reservation", sourceId: string): Effect;
    guard186(commandId: string, value: "contractor-batch-unique", companyId: string, sourceId: string, batchId: string): Effect;
    stageCreatePaymentBatchEffect(now: number, sourceId: string, companyId: string): Effect;
    stageCreatePaymentBatchEffect2(value: string, batchId: string, engagement_id: string | null, person_id: string | null, value2: string, value3: string, net_minor: number, currency: string): Effect;
    readPreparePaymentRetryBatch(batchId: string, companyId: string): Promise<{
        source_type: string;
        source_id: string;
        status: string;
        version: number;
    } | null>;
    readPreparePaymentRetryFailed(batchId: string): Promise<{
        count: number;
    } | null>;
    guard191(commandId: string, value: "batch-retry-version", batchId: string, companyId: string, expectedVersion: number): Effect;
    guard192(commandId: string, value: "batch-retry-items", batchId: string): Effect;
    stagePreparePaymentRetryEffect(userId: string, now: number, batchId: string, companyId: string, expectedVersion: number): Effect;
    readApprovePaymentBatchBatch(batchId: string, companyId: string): Promise<{
        status: string;
        version: number;
        source_type: string;
        created_by: string;
        retry_requested_by: string | null;
    } | null>;
    guard195(commandId: string, value: "batch-version", batchId: string, companyId: string, expectedVersion: number, userId: string): Effect;
    stageApprovePaymentBatchEffect(userId: string, now: number, batchId: string, companyId: string, expectedVersion: number): Effect;
    stageApprovePaymentBatchEffect2(value: string, commandId: string, batchId: string, value2: "people.payment-batch.approve", userId: string, assignmentId: string, expectedVersion: number, commandId2: string, now: number): Effect;
    readExportPaymentBatchV2Batch(batchId: string, companyId: string): Promise<{
        id: string;
        status: string;
        version: number;
        source_type: string;
        source_id: string;
        format_code: string;
        retry_requested_by: string | null;
    } | null>;
    readExportPaymentBatchV2Items(batchId: string, eligibleStatus: "failed" | "outstanding"): Promise<{
        results: ({
            id: string;
            reference: string;
            person_id: string | null;
            payee_name: string;
            amount_minor: number;
            currency: string;
            status: string;
        })[];
    }>;
    readExportPaymentBatchV2Posted(source_id: string, reference: string, source_id2: string): Promise<{
        id: string;
        engagement_id: string;
        net_minor: number;
        currency: string;
        input_snapshot_json: string;
    } | null>;
    guard201(commandId: string, value: string, source_id: string, id: string, id2: string, batchId: string, eligibleStatus: "failed" | "outstanding", reference: string, person_id: string, payee_name: string, amount_minor: number, currency: string, net_minor: number, currency2: string, input_snapshot_json: string): Effect;
    guard202(commandId: string, value: "batch-export-version", batchId: string, companyId: string, version: number): Effect;
    guard203(commandId: string, value: "batch-export-items", batchId: string, eligibleStatus: "failed" | "outstanding", length: number): Effect;
    guard204(commandId: string, value: "posted-payroll-source", source_id: string, companyId: string): Effect;
    stageExportPaymentBatchV2Effect(value: string, companyId: string, batchId: string, version: number, fileKey: string, checksum: string, length: number, total: number, userId: string, now: number): Effect;
    stageExportPaymentBatchV2Effect2(fileKey: string, checksum: string, now: number, now2: number, batchId: string, companyId: string, version: number): Effect;
    readConfirmPaymentBatchBatch(batchId: string, companyId: string): Promise<{
        id: string;
        status: string;
        version: number;
        source_type: string;
        source_id: string;
        currency: string;
        bank_account_id: string | null;
    } | null>;
    readConfirmPaymentBatchItems(batchId: string): Promise<{
        results: ({
            id: string;
            reference: string;
            status: string;
            amount_minor: number;
            engagement_id: string | null;
        })[];
    }>;
    guard209(commandId: string, value: "payment-batch-version", batchId: string, companyId: string, version: number, status: string): Effect;
    readConfirmPaymentBatchPosting(source_id: string, companyId: string): Promise<{
        journal_id: string;
        chart_id: string;
        pay_date: string;
    } | null>;
    readConfirmPaymentBatchPayables(journal_id: string): Promise<{
        results: ({
            account_id: string;
        })[];
    }>;
    readConfirmPaymentBatchBankAccount(bank_account_id: string | null, companyId: string, chart_id: string, currency: string): Promise<Record<string, unknown> | null>;
    readConfirmPaymentBatchTransaction(bankTransactionId: string, companyId: string): Promise<{
        id: string;
        version: number;
        amount_minor: number;
        currency: string;
        match_status: string;
        transaction_date: string;
        external_id: string;
        bank_account: string;
    } | null>;
    readConfirmPaymentBatchPeriods(companyId: string, transaction_date: string, transaction_date2: string): Promise<{
        results: ({
            id: string;
        })[];
    }>;
    readConfirmPaymentBatchPayrollItem(source_id: string, engagement_id: string | null, amount_minor: number): Promise<PayrollPostingItem | null>;
    guard216(commandId: string, value: string, id: string, companyId: string, bankVersion: number, bank_account_id: string, currency: string, value2: number, confirmationRef: string): Effect;
    stageConfirmPaymentBatchEffect(value: string, companyId: string, batchId: string, id: string, id2: string, id3: string, userId: string, now: number): Effect;
    stageConfirmPaymentBatchEffect2(id: string, id2: string, bankVersion: number): Effect;
    stageConfirmPaymentBatchEffect3(value: "failed" | "settled", value2: string | null, value3: number | null, batchId: string, reference: string): Effect;
    stageConfirmPaymentBatchEffect4(status: "partially_settled" | "settled", now: number, batchId: string, companyId: string): Effect;
    readGetPaymentFileBatch(batchId: string, companyId: string): Promise<{
        file_key: string | null;
        file_checksum: string | null;
    } | null>;
}
export interface PeoplePayrollPorts<Effect> {
    pagedRuns(companyId:string,cursor?:string):Promise<ListPage<Record<string,string|number|null>&{id:string;page_key:string}>>;
    repository: PeoplePayrollRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    hash(value: string): Promise<string>;
    authorize(actor: WorkContext, permission: string): WorkContext;
    prepare(holdingId: string): Promise<void>;
    commands: PayrollStatutoryCommands<Effect>;
    environment(): Promise<string | undefined>;
    simulationAdapterId: string;
    files: StreamedFileContent;
    organizationGuard(...args: Tail<Parameters<typeof organizationGuard>>): PortResult<ReturnType<typeof organizationGuard>, Effect>;
    validateStaffOrganization(...args: Tail<Parameters<typeof validateStaffOrganization>>): PortResult<ReturnType<typeof validateStaffOrganization>, Effect>;
    prepareJournal(...args: Tail<Parameters<typeof prepareJournal>>): PortResult<ReturnType<typeof prepareJournal>, Effect>;
    assertPayrollSourceFresh(...args: Tail<Parameters<typeof assertPayrollSourceFresh>>): PortResult<ReturnType<typeof assertPayrollSourceFresh>, Effect>;
    payrollSourceGuards(...args: Tail<Parameters<typeof payrollSourceGuards>>): PortResult<ReturnType<typeof payrollSourceGuards>, Effect>;
    prepareAttachmentBinding(...args: Tail<Parameters<typeof prepareAttachmentBinding>>): PortResult<ReturnType<typeof prepareAttachmentBinding>, Effect>;
    snapshotMoney(...args: Tail<Parameters<typeof snapshotMoney>>): PortResult<ReturnType<typeof snapshotMoney>, Effect>;
    assertStatutoryRunReady(...args: Tail<Parameters<typeof assertStatutoryRunReady>>): PortResult<ReturnType<typeof assertStatutoryRunReady>, Effect>;
    thaiEvidenceForEmployee(...args: Tail<Parameters<typeof thaiEvidenceForEmployee>>): PortResult<ReturnType<typeof thaiEvidenceForEmployee>, Effect>;
    thaiEvidenceGuards(...args: Tail<Parameters<typeof thaiEvidenceGuards>>): PortResult<ReturnType<typeof thaiEvidenceGuards>, Effect>;
    thaiPackForRun(...args: Tail<Parameters<typeof thaiPackForRun>>): PortResult<ReturnType<typeof thaiPackForRun>, Effect>;
    preparePayslipIssue(...args: Tail<Parameters<typeof preparePayslipIssue>>): PortResult<ReturnType<typeof preparePayslipIssue>, Effect>;
    calculatePayrollAdjustmentRun(...args: Tail<Parameters<typeof calculatePayrollAdjustmentRun>>): PortResult<ReturnType<typeof calculatePayrollAdjustmentRun>, Effect>;
    assertPayrollRecoveryDeductionLimits(...args: Tail<Parameters<typeof assertPayrollRecoveryDeductionLimits>>): PortResult<ReturnType<typeof assertPayrollRecoveryDeductionLimits>, Effect>;
    preparePayrollDeductionConsumption(...args: Tail<Parameters<typeof preparePayrollDeductionConsumption>>): PortResult<ReturnType<typeof preparePayrollDeductionConsumption>, Effect>;
    directoryPerson(actor: WorkContext, id: string, engagementId?: string): PortResult<ReturnType<typeof directoryPerson>, Effect>;
    canReadCompensation(...args: Parameters<typeof canReadCompensation>): PortResult<ReturnType<typeof canReadCompensation>, Effect>;
    canManagePeople(...args: Parameters<typeof canManagePeople>): PortResult<ReturnType<typeof canManagePeople>, Effect>;
    recognizedThaiPack(...args: Parameters<typeof recognizedThaiPack>): PortResult<ReturnType<typeof recognizedThaiPack>, Effect>;
    calculateThaiHotelPayrollFixture(...args: Parameters<typeof calculateThaiHotelPayrollFixture>): PortResult<ReturnType<typeof calculateThaiHotelPayrollFixture>, Effect>;
    isThaiHotelPayrollMockEnabled(...args: Parameters<typeof isThaiHotelPayrollMockEnabled>): PortResult<ReturnType<typeof isThaiHotelPayrollMockEnabled>, Effect>;
}
export function createPeoplePayrollApplication<Effect>(ports: PeoplePayrollPorts<Effect>) {
    const repository = ports.repository;
    async function payrollCostAssignments(actor: WorkContext, engagementId: string) {
        actor = ports.authorize(actor, "payroll.prepare");
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Use a Company Payroll assignment to review dated cost units.");
        await ports.prepare(actor.holdingId);
        const engagement = await repository.readPayrollCostAssignmentsEngagement(engagementId, actor.companyId);
        if (!engagement)
            throw new BusinessRuleError(404, "ENGAGEMENT_NOT_FOUND", "Engagement not found.");
        return (await repository.readPayrollCostAssignmentsEffect(engagementId, actor.companyId)).results;
    }
    async function verifyPayrollCostAssignment(actor: WorkContext, assignmentId: string, input: z.infer<typeof payrollCostVerificationSchema>, commandId: string) {
        actor = ports.authorize(actor, "payroll.prepare");
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Use a Company Payroll assignment to verify historical cost units.");
        await ports.prepare(actor.holdingId);
        const prior = await ports.commands.replay(actor, commandId, "people.cost-assignment.verify", assignmentId, input);
        if (prior)
            return prior;
        const assignment = await repository.readVerifyPayrollCostAssignmentAssignment(assignmentId, actor.companyId);
        if (!assignment)
            throw new BusinessRuleError(404, "PAYROLL_COST_ASSIGNMENT_NOT_FOUND", "Dated cost assignment not found.");
        if (assignment.evidence_status !== "legacy_unverified" || assignment.source !== "migration" || assignment.version !== input.expectedVersion)
            throw new BusinessRuleError(409, "PAYROLL_COST_ASSIGNMENT_CHANGED", "Only an unchanged imported interval can be verified here.");
        const history = await repository.readVerifyPayrollCostAssignmentHistory(assignment.engagement_id, actor.companyId);
        if (history || assignment.branch_id !== assignment.current_branch_id || assignment.department_id !== assignment.current_department_id)
            throw new BusinessRuleError(409, "PAYROLL_COST_HISTORY_RECONSTRUCTION_REQUIRED", "Recorded organization changes need dated reconstruction before this interval can be verified.");
        const now = ports.clock.now(), result = { id: assignmentId, status: "verified", version: input.expectedVersion + 1 };
        return ports.commands.commit(actor, commandId, "people.cost-assignment.verify", input, assignmentId, input.expectedVersion + 1, result, [
            repository.guard5(commandId, "assignment-version", assignmentId, actor.companyId, input.expectedVersion),
            repository.stageVerifyPayrollCostAssignmentEffect(now, assignmentId, actor.companyId, input.expectedVersion),
            repository.stageVerifyPayrollCostAssignmentEffect2(`cost-verification-${commandId}`, actor.companyId, assignmentId, actor.userId, input.evidenceRef, input.expectedVersion, now),
            repository.stageVerifyPayrollCostAssignmentEffect3(assignmentId),
            repository.stageVerifyPayrollCostAssignmentEffect4(now, assignmentId, actor.companyId),
        ]);
    }
    async function command<Result = Record<string,unknown>>(actor: WorkContext, id: string, operation: string, input: unknown) {
        const digest = await ports.hash(JSON.stringify({ input, userId: actor.userId, assignmentId: actor.assignmentId, role: actor.role, authority: actor.authority }));
        const current = await repository.readCommandCurrent(id);
        if (current)
            return { digest, result: await ports.commands.replay<Result>(actor, id, operation, current.record_id, input) };
        const old = await repository.readCommandOld(id);
        if (old)
            throw new BusinessRuleError(409, "IDEMPOTENCY_CONFLICT", "This legacy command has no work context. Review its result before retrying.");
        return { digest, result: null };
    }
    function receipt(id: string, operation: string, digest: string, result: unknown) {
        return repository.stageReceiptEffect(id, operation, digest, JSON.stringify(result), ports.clock.now());
    }
    async function listPeople(actor: WorkContext, filters: {
        levelId?: string;
        departmentId?: string;
    } = {}) {
        actor = ports.authorize(actor, "people.manage");
        await ports.prepare(actor.holdingId);
        const rows = await repository.readListPeopleRows(actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, filters.levelId ?? null, filters.levelId ?? null, filters.levelId ?? null, filters.departmentId ?? null, filters.departmentId ?? null);
        return rows.results;
    }
    async function getPerson(actor: WorkContext, personId: string, engagementId?: string) {
        await ports.prepare(actor.holdingId);
        const person = await ports.directoryPerson(actor, personId, engagementId);
        const privateAccess = ports.canReadCompensation(actor, person.user_id);
        if (!ports.canManagePeople(actor) && !privateAccess)
            throw new BusinessRuleError(403, "PERSON_DETAILS_FORBIDDEN", "Open the Holding directory to view this person's work contact details.");
        const compensation = privateAccess ? await repository.readGetPersonCompensation(person.engagement_id) : null;
        const [leave, time] = privateAccess ? await Promise.all([
            repository.readGetPersonLeavetime(personId, actor.companyId, actor.branchId, actor.branchId),
            repository.readGetPersonLeavetime2(person.engagement_id),
        ]) : [{ results: [] }, { results: [] }];
        if (!privateAccess) {
            person.phone = null;
        }
        const revisions = privateAccess ? (await repository.readGetPersonRevisions(person.engagement_id)).results : [];
        const recommendedPay = privateAccess && person.position_id ? (await repository.readGetPersonRecommendedPay(actor.companyId, person.position_id)) : null;
        const recommendedLeave = person.level_id ? (await repository.readGetPersonRecommendedLeave(actor.companyId, person.level_id)) : null;
        const leaveAssignment = person.engagement_id ? (await repository.readGetPersonLeaveAssignment(actor.companyId, person.engagement_id)) : null;
        return { person: { ...person, ...compensation }, leave: leave.results, time: time.results, privateAccess, revisions, recommendedPay, recommendedLeave, leaveAssignment };
    }
    async function peopleSummary(actor: WorkContext) {
        await ports.prepare(actor.holdingId);
        const [people, active, contractors, runs, pending] = await Promise.all([
            repository.readPeopleSummaryPeopleactivecontractorsrunspending(actor),
            repository.readPeopleSummaryPeopleactivecontractorsrunspending2(actor),
            repository.readPeopleSummaryPeopleactivecontractorsrunspending3(actor),
            (!actor.branchId && !actor.departmentId && hasPermission(actor, "payroll.read")) ? repository.readPeopleSummaryPeopleactivecontractorsrunspending4(actor.companyId) : Promise.resolve(null),
            repository.readPeopleSummaryPeopleactivecontractorsrunspending5(actor, ports.canManagePeople(actor) ? 1 : 0, actor.userId),
        ]);
        return { people: people?.count ?? 0, active: active?.count ?? 0, contractors: contractors?.count ?? 0, payrollRuns: runs?.count ?? 0, pendingLeave: pending?.count ?? 0 };
    }
    async function payrollSummary(actor: WorkContext, payMonth?: string) {
        actor = ports.authorize(actor, "payroll.read");
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Use a Company payroll assignment for the payroll summary.");
        if (payMonth && !/^\d{4}-(0[1-9]|1[0-2])$/.test(payMonth))
            throw new BusinessRuleError(422, "INVALID_PAY_MONTH", "Use a calendar payroll month.");
        await ports.prepare(actor.holdingId);
        return (await repository.readPayrollSummaryEffect(actor.companyId, payMonth ?? null, payMonth ?? null)).results;
    }
    async function createPerson(actor: WorkContext, input: z.infer<typeof createPersonSchema>, id: string) {
        actor = ports.authorize(actor, "people.manage");
        await ports.prepare(actor.holdingId);
        const c = await command<{id:string;legalName:string;status:string;version:number}>(actor, id, "people.person.create", input);
        if (c.result)
            return c.result;
        const personId = `person-${ports.identifiers.next()}`;
        const now = ports.clock.now();
        const result = { id: personId, legalName: input.legalName, status: "active", version: 1 };
        await ports.commands.commit(actor, id, "people.person.create", input, personId, 1, result, [
            repository.stageCreatePersonEffect(personId, input.userId ?? null, actor.companyId, input.legalName, input.preferredName ?? null, input.email ?? null, input.phone ?? null, input.countryCode, actor.userId, now, now),
            receipt(id, "people.person.create", c.digest, result),
        ]);
        return result;
    }
    async function createEngagement(actor: WorkContext, input: z.infer<typeof createEngagementSchema>, id: string) {
        actor = ports.authorize(actor, "people.manage");
        await ports.prepare(actor.holdingId);
        const c = await command<{id:string;personId:string;status:string;version:number}>(actor, id, "people.engagement.create", input);
        if (c.result)
            return c.result;
        const person = await repository.readCreateEngagementPerson(input.personId, actor.companyId);
        if (!person)
            throw new BusinessRuleError(404, "PERSON_NOT_FOUND", "Person not found.");
        if (input.endDate && input.endDate < input.startDate)
            throw new BusinessRuleError(422, "INVALID_DATES", "End date must be on or after start date.");
        if (input.workerType === "employee" && input.employmentType === "contractor")
            throw new BusinessRuleError(422, "INVALID_WORKER_TYPE", "Employee engagements cannot use contractor employment type.");
        const engagementId = `eng-${ports.identifiers.next()}`;
        const now = ports.clock.now();
        const result = { id: engagementId, personId: input.personId, status: "onboarding", version: 1 };
        await ports.commands.commit(actor, id, "people.engagement.create", input, engagementId, 1, result, [
            repository.stageCreateEngagementEffect(engagementId, input.personId, actor.companyId, input.branchId ?? actor.branchId, input.departmentId ?? actor.departmentId, input.workerType, input.employmentType, "onboarding", input.startDate, input.endDate ?? null, input.managerPersonId ?? null, input.jobTitle, input.currency, input.payGroupId ?? null, actor.userId, now, now),
            receipt(id, "people.engagement.create", c.digest, result),
        ]);
        return result;
    }
    async function createCompensation(actor: WorkContext, input: z.infer<typeof compensationSchema>, key: string) {
        actor = ports.authorize(actor, "people.manage");
        await ports.prepare(actor.holdingId);
        const id = `comp-${key}`;
        const previous = await ports.commands.replay(actor, key, "people.compensation.create", id, input);
        if (previous)
            return previous;
        const e = await repository.readCreateCompensationE(input.engagementId, actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId);
        if (!e)
            throw new BusinessRuleError(404, "ENGAGEMENT_NOT_FOUND", "Engagement not found in your scope.");
        if (e.currency !== input.currency)
            throw new BusinessRuleError(422, "CURRENCY_MISMATCH", "Compensation must use the engagement currency.");
        if (input.effectiveTo && input.effectiveTo < input.effectiveFrom)
            throw new BusinessRuleError(422, "INVALID_DATES", "Effective end must follow the start date.");
        const base = decimalToMinor(input.baseAmount), hourly = decimalToMinor(input.hourlyRate), daily = decimalToMinor(input.dailyRate);
        for (const c of input.components)
            decimalToMinor(c.amount);
        if (new Set(input.components.map(c => c.code)).size !== input.components.length)
            throw new BusinessRuleError(422, "DUPLICATE_COMPONENT", "Each compensation component needs a distinct code.");
        const amounts = [{ kind: "base", amount: base }, { kind: "hourly_rate", amount: hourly }, { kind: "daily_rate", amount: daily }, ...input.components.map(component => ({ kind: `component:${component.code}`, amount: decimalToMinor(component.amount) }))];
        const snapshots = await Promise.all(amounts.map(amount => ports.snapshotMoney(actor, { sourceType: "hr_compensation_revision", sourceId: id, amountKind: amount.kind, transactionDate: input.effectiveFrom, currency: input.currency, currencyScale: 2, amountUnits: String(amount.amount) })));
        const now = ports.clock.now();
        return ports.commands.commit(actor, key, "people.compensation.create", input, id, 1, { id, status: "draft", version: 1 }, [
            repository.guard31(key, "engagement", input.engagementId, e.version, input.currency),
            repository.stageCreateCompensationEffect(id, input.engagementId, input.effectiveFrom, input.effectiveTo ?? null, input.payFrequency, base, hourly, daily, input.standardDailyHours, input.expectedHours, input.currency, JSON.stringify(input.components), actor.userId, now, now),
            ...snapshots.map(snapshot => snapshot.statement),
        ]);
    }
    async function payrollOptions(actor: WorkContext) { actor = ports.authorize(actor, "people.payroll.details"); await ports.prepare(actor.holdingId); return (await repository.readPayrollOptionsEffect(actor.companyId)).results; }
    async function assignPayGroup(actor: WorkContext, engagementId: string, input: z.infer<typeof assignPayGroupSchema>, key: string) {
        actor = ports.authorize(actor, "people.manage");
        await ports.prepare(actor.holdingId);
        const previous = await ports.commands.replay(actor, key, "people.pay-group.assign", engagementId, input);
        if (previous)
            return previous;
        const missing = await repository.readAssignPayGroupMissing(engagementId, actor.companyId);
        if (missing)
            throw new BusinessRuleError(422, "LEVEL_REQUIRED", "Assign an active staff level before updating this engagement.");
        return ports.commands.commit(actor, key, "people.pay-group.assign", input, engagementId, input.expectedVersion + 1, { id: engagementId, version: input.expectedVersion + 1 }, [
            repository.guard35(key, "engagement", engagementId, actor.companyId, input.expectedVersion, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId),
            repository.guard36(key, "active-level", engagementId),
            repository.guard37(key, "pay-group", input.payGroupId, engagementId, input.payGroupId),
            repository.stageAssignPayGroupEffect(input.payGroupId, ports.clock.now(), engagementId),
        ]);
    }
    async function transitionCompensation(actor: WorkContext, compensationId: string, input: z.infer<typeof compensationTransitionSchema>, key: string) {
        actor = ports.authorize(actor, "people.compensation.approve");
        await ports.prepare(actor.holdingId);
        const previous = await ports.commands.replay(actor, key, "people.compensation.transition", compensationId, input);
        if (previous)
            return previous;
        if (await repository.readTransitionCompensationEffect(compensationId, actor.companyId))
            throw new BusinessRuleError(409, "EVALUATION_COMPENSATION_WORKFLOW_REQUIRED", "Approve this draft in People evaluation so the original and new pay intervals commit together.");
        const row = await repository.readTransitionCompensationRow(compensationId, actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId);
        if (!row)
            throw new BusinessRuleError(404, "COMPENSATION_NOT_FOUND", "Compensation not found in your scope.");
        if (samePerson(actor, row.created_by))
            throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "A different person must approve compensation.");
        return ports.commands.commit(actor, key, "people.compensation.transition", input, compensationId, input.expectedVersion + 1, { id: compensationId, status: input.status, version: input.expectedVersion + 1 }, [
            repository.guard41(key, "revision", compensationId, input.expectedVersion, input.status === "approved" ? "draft" : "approved"),
            ...(input.status === "approved" ? [repository.guard42(key, "overlap", row.engagement_id, compensationId, row.effective_to, row.effective_from)] : []),
            repository.stageTransitionCompensationEffect(input.status, actor.userId, ports.clock.now(), compensationId),
            repository.stageTransitionCompensationEffect2(ports.identifiers.next(), key, compensationId, `people.compensation.${input.status}`, actor.userId, actor.assignmentId, input.expectedVersion, await ports.hash(JSON.stringify(input)), ports.clock.now()),
        ]);
    }
    async function transitionEngagement(actor: WorkContext, engagementId: string, input: z.infer<typeof engagementTransitionSchema>, key: string) {
        actor = ports.authorize(actor, "people.manage");
        if (!actor.beta)
            throw new BusinessRuleError(409, "LIFECYCLE_REQUIRED", "Use the person lifecycle workflow to review verified accounts and the effective time.");
        await ports.prepare(actor.holdingId);
        const previous = await ports.commands.replay(actor, key, "people.engagement.transition", engagementId, input);
        if (previous)
            return previous;
        const guard = await ports.organizationGuard(actor.companyId, key);
        const row = await repository.readTransitionEngagementRow(engagementId, actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId);
        if (!row)
            throw new BusinessRuleError(404, "ENGAGEMENT_NOT_FOUND", "Engagement not found in your scope.");
        if (input.status === "active")
            await ports.validateStaffOrganization(actor.companyId, row, true);
        if (row.user_id === actor.userId && input.status !== "active")
            throw new BusinessRuleError(409, "SELF_DEACTIVATION", "Another HR administrator must deactivate your engagement.");
        const valid = (row.status === "onboarding" && input.status === "active") || (["active", "suspended"].includes(row.status) && input.status !== row.status);
        if (!valid)
            throw new BusinessRuleError(409, "INVALID_ENGAGEMENT_TRANSITION", "Engagement cannot move through that transition.");
        if (input.status === "ended" && !input.endDate)
            throw new BusinessRuleError(422, "END_DATE_REQUIRED", "Enter the final date of engagement.");
        if (input.endDate && input.endDate < row.start_date)
            throw new BusinessRuleError(422, "INVALID_DATES", "End date must be on or after start date.");
        const now = ports.clock.now();
        return ports.commands.commit(actor, key, "people.engagement.transition", input, engagementId, input.expectedVersion + 1, { id: engagementId, status: input.status, version: input.expectedVersion + 1 }, [
            guard,
            repository.guard46(key, "engagement", engagementId, actor.companyId, input.expectedVersion, row.status),
            ...(input.status === "active" ? [repository.guard47(key, "active-level", engagementId)] : []),
            repository.stageTransitionEngagementEffect(input.status, input.status, input.endDate ?? null, now, engagementId),
            repository.stageTransitionEngagementEffect2(row.person_id, row.person_id),
            ...(input.status === "active" || !row.user_id ? [] : [repository.stageTransitionEngagementEffect3(now, row.user_id, actor.companyId, row.person_id)]),
        ]);
    }
    async function createTimeEntry(actor: WorkContext, input: z.infer<typeof timeEntrySchema>, key: string) {
        await ports.prepare(actor.holdingId);
        const id = `time-${key}`;
        const previous = await ports.commands.replay(actor, key, "people.time.submit", id, input);
        if (previous)
            return previous;
        const e = await repository.readCreateTimeEntryE(input.engagementId, actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId);
        if (!e)
            throw new BusinessRuleError(404, "ENGAGEMENT_NOT_FOUND", "Active employee placement not found in your scope.");
        if (e.user_id !== actor.userId && !ports.canManagePeople(actor))
            throw new BusinessRuleError(403, "TIME_OWNER_REQUIRED", "Submit time for your own employee record.");
        if (input.regularHours + input.overtimeHours > 24 || input.regularHours + input.overtimeHours <= 0)
            throw new BusinessRuleError(422, "INVALID_HOURS", "Enter a positive total of at most 24 hours per day.");
        if (input.workDate < String(e.start_date) || (e.end_date && input.workDate > String(e.end_date)))
            throw new BusinessRuleError(422, "DATE_OUTSIDE_ENGAGEMENT", "Work date must fall within the engagement.");
        return ports.commands.commit(actor, key, "people.time.submit", input, id, 1, { id, status: "submitted", version: 1 }, [
            repository.guard52(key, "engagement", input.engagementId, e.version),
            repository.stageCreateTimeEntryEffect(id, input.engagementId, actor.companyId, input.workDate, input.regularHours, input.overtimeHours, input.note, actor.userId, ports.clock.now(), ports.clock.now()),
        ]);
    }
    async function updateTimeEntry(actor: WorkContext, entryId: string, input: z.infer<typeof timeEntryUpdateSchema>, key: string) {
        await ports.prepare(actor.holdingId);
        const previous = await ports.commands.replay(actor, key, "people.time.update", entryId, input);
        if (previous)
            return previous;
        const entry = await repository.readUpdateTimeEntryEntry(entryId, actor.companyId, actor.userId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId);
        if (!entry)
            throw new BusinessRuleError(404, "TIME_ENTRY_NOT_FOUND", "Time entry not found in your editable records.");
        if (input.attachmentSetIds.length && entry.status !== "rejected")
            throw new BusinessRuleError(409, "TIME_CORRECTION_EVIDENCE_STATE", "Correction proof can be bound only while resubmitting a returned time entry.");
        if (input.regularHours + input.overtimeHours > 24 || input.regularHours + input.overtimeHours <= 0)
            throw new BusinessRuleError(422, "INVALID_HOURS", "Enter a positive total of at most 24 hours per day.");
        if (input.workDate < String(entry.start_date) || (entry.end_date && input.workDate > String(entry.end_date)))
            throw new BusinessRuleError(422, "DATE_OUTSIDE_ENGAGEMENT", "Work date must fall within the engagement.");
        const attachmentBinding = input.attachmentSetIds.length ? await ports.prepareAttachmentBinding(actor, { attachmentSetIds: input.attachmentSetIds, bindingContract: { targetType: "time_entry", bindingType: "time_entry_correction", contexts: [{ purposeKey: "corrected_entry_proof", componentKind: "action", componentRef: "correction-resubmission" }] }, targetId: entryId, expectedTargetVersion: input.expectedVersion, commandId: key, bindingId: entryId }) : null;
        return ports.commands.commit(actor, key, "people.time.update", input, entryId, input.expectedVersion + 1, { id: entryId, status: "submitted", version: input.expectedVersion + 1 }, [
            ...(attachmentBinding?.guards ?? []),
            repository.guard55(key, "editable-time", entryId, input.expectedVersion, actor.userId),
            repository.stageUpdateTimeEntryEffect(input.workDate, input.regularHours, input.overtimeHours, input.note, ports.clock.now(), entryId),
            ...(attachmentBinding?.steps ?? []),
        ]);
    }
    async function rejectTimeEntry(actor: WorkContext, entryId: string, input: z.infer<typeof timeEntryReviewSchema>, key: string) {
        actor = ports.authorize(actor, "people.time.manage");
        await ports.prepare(actor.holdingId);
        return ports.commands.commit(actor, key, "people.time.reject", input, entryId, input.expectedVersion + 1, { id: entryId, status: "rejected", version: input.expectedVersion + 1 }, [
            repository.guard57(key, "reviewable-time", entryId, actor.companyId, input.expectedVersion, actor.userId, actor.userId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId),
            repository.stageRejectTimeEntryEffect(input.reason, ports.clock.now(), entryId),
            repository.stageRejectTimeEntryEffect2(ports.identifiers.next(), key, entryId, "people.time.reject", actor.userId, actor.assignmentId, input.expectedVersion, await ports.hash(JSON.stringify(input)), ports.clock.now()),
        ]);
    }
    async function approveTimeEntry(actor: WorkContext, entryId: string, expectedVersion: number, key: string) {
        actor = ports.authorize(actor, "people.time.manage");
        await ports.prepare(actor.holdingId);
        return ports.commands.commit(actor, key, "people.time.approve", { expectedVersion }, entryId, expectedVersion + 1, { id: entryId, status: "approved", version: expectedVersion + 1 }, [
            repository.guard60(key, "time", entryId, actor.companyId, expectedVersion, actor.userId, actor.userId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId),
            repository.stageApproveTimeEntryEffect(actor.userId, ports.clock.now(), entryId),
            repository.stageApproveTimeEntryEffect2(ports.identifiers.next(), key, entryId, "people.time.approve", actor.userId, actor.assignmentId, expectedVersion, key, ports.clock.now()),
        ]);
    }
    async function createPayGroup(actor: WorkContext, input: z.infer<typeof payGroupSchema>, id: string) {
        actor = ports.authorize(actor, "payroll.prepare");
        await ports.prepare(actor.holdingId);
        const c = await command<z.infer<typeof payGroupSchema> & {id:string;status:string}>(actor, id, "people.pay-group.create", input);
        if (c.result)
            return c.result;
        const groupId = `paygrp-${ports.identifiers.next()}`;
        const now = ports.clock.now();
        const result = { id: groupId, ...input, status: "active" };
        await ports.commands.commit(actor, id, "people.pay-group.create", input, groupId, 1, result, [repository.stageCreatePayGroupEffect(groupId, actor.companyId, input.name, input.countryCode, input.currency, input.frequency, input.timezone, input.overtimeMultiplierBps, input.prorationMethod, actor.userId, now, now), receipt(id, "people.pay-group.create", c.digest, result)]);
        return result;
    }
    async function createCountryPack(actor: WorkContext, input: z.infer<typeof countryPackSchema>, id: string) {
        actor = ports.authorize(actor, "payroll.prepare");
        await ports.prepare(actor.holdingId);
        const c = await command<{id:string;countryCode:string;version:string;status:string;recordVersion:number;adapterId:string|null}>(actor, id, "people.country-pack.create", input);
        if (c.result)
            return c.result;
        const packId = `pack-${ports.identifiers.next()}`, now = ports.clock.now();
        const recognized = await ports.recognizedThaiPack(input.rules, input.countryCode, input.version).catch(error => {
            if (error instanceof BusinessRuleError && error.code === "PAYROLL_ADAPTER_RULES_UNSUPPORTED")
                return null;
            throw error;
        });
        const result = { id: packId, countryCode: input.countryCode, version: input.version, status: "testing", recordVersion: 1, adapterId: recognized?.adapterId ?? null };
        await ports.commands.commit(actor, id, "people.country-pack.create", input, packId, 1, result, [repository.stageCreateCountryPackEffect(packId, input.countryCode, input.version, "testing", JSON.stringify(input.rules), JSON.stringify(input.exportFormats), actor.userId, now, now, recognized?.adapterId ?? null, recognized?.sourceDigest ?? null, recognized?.rules.effectiveFrom ?? null, recognized?.rules.effectiveTo ?? null), receipt(id, "people.country-pack.create", c.digest, result)]);
        return result;
    }
    async function listCountryPacks(actor: WorkContext) { actor = ports.authorize(actor, "payroll.configuration.read"); await ports.prepare(actor.holdingId); return (await repository.readListCountryPacksEffect()).results; }
    async function transitionCountryPack(actor: WorkContext, packId: string, input: z.infer<typeof countryPackTransitionSchema>, commandId: string) {
        actor = ports.authorize(actor, "people.compensation.approve");
        await ports.prepare(actor.holdingId);
        const prior = await ports.commands.replay(actor, commandId, `people.country-pack.${input.status}`, packId, input);
        if (prior)
            return prior;
        const pack = await repository.readTransitionCountryPackPack(packId);
        if (!pack)
            throw new BusinessRuleError(404, "COUNTRY_PACK_NOT_FOUND", "Country pack not found.");
        if (pack.record_version !== input.expectedVersion)
            throw new BusinessRuleError(409, "COUNTRY_PACK_CHANGED", "The rule pack changed. Reload before deciding.");
        if (samePerson(actor, pack.created_by))
            throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "A different person must review the statutory rule pack.");
        const allowed = (pack.status === "testing" && input.status === "validated") || (pack.status === "validated" && input.status === "active") || (pack.status === "active" && input.status === "superseded");
        if (!allowed)
            throw new BusinessRuleError(409, "INVALID_COUNTRY_PACK_TRANSITION", "Country pack status must progress testing → validated → active.");
        const recognized = await ports.recognizedThaiPack(JSON.parse(pack.rules_json), pack.country_code, pack.version);
        if (pack.source_digest !== recognized.sourceDigest || pack.adapter_id !== recognized.adapterId)
            throw new BusinessRuleError(409, "PAYROLL_RULE_EVIDENCE_MISSING", "Create a new rule revision with its retained source digest.");
        const now = ports.clock.now(), recordVersion = pack.record_version + 1, result = { id: packId, status: input.status, recordVersion };
        await ports.commands.commit(actor, commandId, `people.country-pack.${input.status}`, input, packId, recordVersion, result, [repository.guard67(commandId, "pack-status", packId, pack.status, pack.record_version, pack.source_digest), repository.stageTransitionCountryPackEffect(input.status, input.proof, actor.userId, now, packId), repository.stageTransitionCountryPackEffect2(ports.identifiers.next(), commandId, packId, `people.country-pack.${input.status}`, actor.userId, actor.assignmentId, pack.record_version, recognized.sourceDigest, now)]);
        return result;
    }
    async function calculateItem(run: PayrollStatutoryRun, engagement: {
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
    }, useSimulationAdapter: boolean) {
        const payee = await repository.readCalculateItemPayee(engagement.person_id, run.company_id);
        if (!payee)
            throw new BusinessRuleError(422, "PAYROLL_PAYEE_INVALID", "Verify the employee identity before calculating payroll.");
        const compensation = await repository.readCalculateItemCompensation(engagement.id, run.period_end, run.period_start);
        const time = await repository.readCalculateItemTime(engagement.id, run.period_start, run.period_end);
        const handoffs = (await repository.readCalculateItemHandoffs(engagement.id, run.period_start, run.period_end)).results;
        const hours = { handoffs, regular: time.results.reduce((sum, t) => sum + t.regular_hours, 0), overtime: time.results.reduce((sum, t) => sum + t.overtime_hours, 0) + handoffs.reduce((sum, h) => sum + h.minutes / 60, 0), entries: time.results };
        const base = Number(compensation?.base_amount_minor ?? 0);
        const hourly = Number(compensation?.hourly_rate_minor ?? 0);
        const regular = Number(hours?.regular ?? 0);
        const overtime = Number(hours?.overtime ?? 0);
        const daily = Number(compensation?.daily_rate_minor ?? 0), dailyHours = Number(compensation?.standard_daily_hours ?? 8), regularByDay = new Map<string, number>();
        for (const entry of time.results)
            regularByDay.set(entry.work_date, (regularByDay.get(entry.work_date) ?? 0) + entry.regular_hours);
        if (engagement.employment_type === "daily" && (daily <= 0 || [...regularByDay.values()].some(value => value > dailyHours)))
            throw new BusinessRuleError(422, "DAILY_PAYROLL_TIME_INVALID", "Daily pay requires a positive approved rate and regular hours within the reviewed daily schedule; classify additional hours as overtime.");
        const regularDays = [...regularByDay.values()].reduce((sum, value) => sum + value / dailyHours, 0);
        const components = compensation?.components_json ? JSON.parse(String(compensation.components_json)) as Array<{
            code: string;
            label: string;
            kind: string;
            amount: string;
            taxable?: boolean;
            incomeTiming?: "regular" | "irregular" | "non_taxable";
            socialSecurityWage?: boolean;
        }> : [];
        const benefitInputs = (await repository.readCalculateItemBenefitInputs(engagement.id, run.currency, run.period_end, run.period_start)).results;
        for (const benefit of benefitInputs) {
            if (Number(benefit.employee_deduction_minor) > 0)
                components.push({ code: `benefit-deduction:${benefit.id}`, label: `Benefit · ${benefit.plan_name}`, kind: "deduction", amount: minorToDecimal(Number(benefit.employee_deduction_minor)) });
            if (Number(benefit.employer_cost_minor) > 0)
                components.push({ code: `benefit-employer:${benefit.id}`, label: `Benefit employer cost · ${benefit.plan_name}`, kind: "employer", amount: minorToDecimal(Number(benefit.employer_cost_minor)) });
        }
        const advanceRepayments = (await repository.readCalculateItemAdvanceRepayments(run.company_id, engagement.id, run.period_end, run.period_start, run.id)).results;
        for (const repayment of advanceRepayments) {
            components.push({ code: `payroll-advance:${repayment.id}`, label: `Payroll advance repayment · ${repayment.due_period_start}`, kind: "deduction", amount: minorToDecimal(Number(repayment.amount_minor)) });
        }
        const monthlyFinalSalary = run.mode === "final_pay" && engagement.employment_type === "salaried" && compensation?.pay_frequency === "monthly" && !useSimulationAdapter;
        if (monthlyFinalSalary && run.period_start.slice(0, 7) !== run.period_end.slice(0, 7))
            throw new BusinessRuleError(422, "FINAL_PAY_CALENDAR_MONTH_REQUIRED", "Prepare monthly salary final pay within one calendar month using the reviewed calendar-day proration policy.");
        const day = 86400000, periodDays = monthlyFinalSalary ? new Date(Date.UTC(Number(run.period_start.slice(0, 4)), Number(run.period_start.slice(5, 7)), 0)).getUTCDate() : Math.floor((Date.parse(run.period_end) - Date.parse(run.period_start)) / day) + 1, activeStart = engagement.start_date > run.period_start ? engagement.start_date : run.period_start, activeEnd = engagement.end_date && engagement.end_date < run.period_end ? engagement.end_date : run.period_end, activeDays = Math.floor((Date.parse(activeEnd) - Date.parse(activeStart)) / day) + 1, partial = activeDays !== periodDays;
        const costAssignments = (await repository.readCalculateItemCostAssignments(engagement.id, run.company_id, activeEnd, activeStart)).results;
        const assignmentOn = (workDate: string) => costAssignments.filter(row => row.effective_from <= workDate && (row.effective_to === null || row.effective_to >= workDate));
        for (let dateMs = Date.parse(activeStart); dateMs <= Date.parse(activeEnd); dateMs += day) {
            const rows = assignmentOn(new Date(dateMs).toISOString().slice(0, 10));
            if (rows.length !== 1 || rows[0].evidence_status !== "verified")
                throw new BusinessRuleError(422, "PAYROLL_COST_ALLOCATION_INCOMPLETE", "Verify dated branch and department assignments for every work day before payroll review.");
        }
        for (const entry of time.results) {
            const rows = assignmentOn(entry.work_date);
            if (rows.length !== 1 || rows[0].id !== entry.cost_assignment_id || rows[0].branch_id !== entry.cost_branch_id || rows[0].department_id !== entry.cost_department_id)
                throw new BusinessRuleError(422, "PAYROLL_TIME_COST_EVIDENCE_MISSING", "Approve time again after its dated cost unit is verified.");
        }
        if (partial && policy.prorationMethod === "none")
            throw new BusinessRuleError(422, "PRORATION_REQUIRED", "This pay group does not permit automatic proration for partial-period employment.");
        const normalizedComponents = components.filter((component): component is {
            code: string;
            label: string;
            kind: "earning" | "deduction" | "employer";
            amount: string;
        } => ["earning", "deduction", "employer"].includes(component.kind));
        const mockContext = useSimulationAdapter
            ? components
                .filter((component) => component.code.startsWith("MOCK_FIXTURE:") && component.kind === "ytd_context")
                .map((component) => ({
                ...component,
                label: `MOCK ONLY · ${component.label}`,
                amountMinor: decimalToMinor(component.amount),
                includedInTotals: false,
            }))
            : [];
        const payrollInput = { employmentType: engagement.employment_type as "salaried" | "daily" | "hourly" | "shift" | "contractor", baseMinor: base, hourlyMinor: hourly, dailyMinor: daily, regularDays, regularHours: regular, overtimeHours: overtime, overtimeMultiplierBps: policy.overtimeMultiplierBps, ...(partial && policy.prorationMethod === "calendar_days" ? { prorationNumerator: activeDays, prorationDenominator: periodDays } : {}) };
        const mockComponents = useSimulationAdapter
            ? normalizedComponents
                .filter((component) => component.code.startsWith("MOCK_FIXTURE:"))
                .map((component) => ({ ...component, fixtureId: component.code }))
            : [];
        const ordinaryComponents = useSimulationAdapter
            ? normalizedComponents.filter((component) => !component.code.startsWith("MOCK_FIXTURE:"))
            : normalizedComponents;
        const grossCalculation = useSimulationAdapter
            ? ports.calculateThaiHotelPayrollFixture({ payroll: { ...payrollInput, components: ordinaryComponents }, fixtures: mockComponents })
            : calculatePayrollAmounts({ ...payrollInput, components: ordinaryComponents });
        await ports.assertPayrollRecoveryDeductionLimits(run, advanceRepayments, grossCalculation.gross, grossCalculation.deductions);
        const thaiEvidence = (run.mode === "production" || (run.mode === "final_pay" && run.country_pack_id)) ? await ports.thaiEvidenceForEmployee(run, engagement.id) : null;
        const calculated = thaiEvidence ? applyThaiStatutory(grossCalculation, thaiEvidence, run, grossCalculation.overtimePay) : grossCalculation;
        const { gross, deductions, employer, net } = calculated;
        const inputSnapshot = { engagement, payee, compensation, hours, benefitInputs, advanceRepayments, costAssignments, ...(thaiEvidence ? { thaiStatutory: thaiEvidence } : {}), period: { start: run.period_start, end: run.period_end }, policy: { ...policy, activeDays, periodDays, ...(useSimulationAdapter ? { calculationAdapter: ports.simulationAdapterId, statutoryCalculation: false } : thaiEvidence ? { calculationAdapter: "thai-payroll-v1", statutoryCalculation: true } : {}) } };
        const result = { grossMinor: gross, deductionsMinor: deductions, employerCostMinor: employer, netMinor: net, ...("statutory" in calculated ? { statutory: calculated.statutory } : {}), ...("adapter" in calculated ? { adapter: calculated.adapter } : {}), explanation: { baseMinor: base, hourlyRateMinor: hourly, dailyRateMinor: daily, standardDailyHours: dailyHours, regularDays, regularHours: regular, overtimeHours: overtime, overtimeMinor: grossCalculation.overtimePay, overtimeMultiplierBps: policy.overtimeMultiplierBps, activeDays, periodDays, components: [...calculated.applied, ...mockContext] } };
        return { itemId: `payitem-${ports.identifiers.next()}`, engagementId: engagement.id, personId: engagement.person_id, workerType: engagement.worker_type, inputSnapshot, result, gross, deductions, employer, net };
    }
    async function createPayrollRun(actor: WorkContext, input: z.infer<typeof payrollRunSchema>, key: string) {
        actor = ports.authorize(actor, "payroll.prepare");
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Company-wide payroll requires a holding-level assignment.");
        await ports.prepare(actor.holdingId);
        const id = `payrun-${key}`;
        const previous = await ports.commands.replay<{id:string;status:string;mode:string;version:number}>(actor, key, "people.payroll.create", id, input);
        if (previous)
            return previous;
        if (input.mode === "final_pay")
            throw new BusinessRuleError(422, "FINAL_PAY_CLEARANCE_REQUIRED", "Prepare final payroll from the assigned HR clearance task.");
        const selectedPack = input.mode === "production" ? await ports.thaiPackForRun({ id, company_id: actor.companyId, pay_group_id: input.payGroupId, country_pack_id: input.countryPackId, period_start: input.periodStart, period_end: input.periodEnd, pay_date: input.payDate, currency: input.currency, mode: input.mode }) : null;
        if (input.periodEnd < input.periodStart || input.payDate < input.periodEnd)
            throw new BusinessRuleError(422, "INVALID_PAYROLL_PERIOD", "Review the period and pay date.");
        const now = ports.clock.now();
        return ports.commands.commit(actor, key, "people.payroll.create", input, id, 1, { id, status: "calculating", mode: input.mode, version: 1 }, [
            repository.guard77(key, "pay-group", input.payGroupId, actor.companyId, input.currency),
            repository.guard78(key, "duplicate-period", actor.companyId, input.payGroupId, input.periodEnd, input.periodStart),
            ...(selectedPack ? [repository.guard79(key, "statutory-pack", selectedPack.row.id, selectedPack.row.record_version, selectedPack.sourceDigest)] : []),
            repository.stageCreatePayrollRunEffect(id, actor.companyId, input.payGroupId, input.countryPackId ?? null, input.periodStart, input.periodEnd, input.payDate, input.currency, input.mode, actor.userId, now, now),
        ]);
    }
    async function prepareFinalPay(actor: WorkContext, input: z.infer<typeof finalPayPreparationSchema>, key: string) {
        actor = ports.authorize(actor, "payroll.prepare");
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Final payroll requires a holding-level assignment.");
        await ports.prepare(actor.holdingId);
        const prepId = `final-pay-${key}`, runId = `payrun-final-${key}`, previous = await ports.commands.replay(actor, key, "people.payroll.final-pay.prepare", prepId, input);
        if (previous)
            return previous;
        if (input.periodEnd < input.periodStart || input.payDate < input.periodEnd)
            throw new BusinessRuleError(422, "INVALID_PAYROLL_PERIOD", "Review the final-pay period and pay date.");
        const task = await repository.readPrepareFinalPayTask(input.taskId, actor.companyId);
        if (!task)
            throw new BusinessRuleError(404, "CLEARANCE_TASK_NOT_FOUND", "Final payroll clearance task not found.");
        if (task.clearance_type !== "payroll" || task.status !== "pending" || task.assignee_id !== actor.userId)
            throw new BusinessRuleError(403, "FINAL_PAY_OWNER_REQUIRED", "Only the assigned Payroll clearance owner can prepare final pay.");
        if (task.version !== input.expectedTaskVersion)
            throw new BusinessRuleError(409, "CLEARANCE_TASK_CHANGED", "Refresh the clearance task before preparing final pay.");
        if (task.pay_group_id !== input.payGroupId || task.currency !== input.currency)
            throw new BusinessRuleError(422, "FINAL_PAY_POLICY_MISMATCH", "Final pay must use the engagement pay group and currency.");
        const definition = typeof task.definition === "string" ? JSON.parse(task.definition) as {
            lastWorkingDate?: string;
        } : task.definition;
        const lastWorking = definition.lastWorkingDate ?? task.end_date;
        if (!lastWorking || input.periodEnd > lastWorking)
            throw new BusinessRuleError(422, "FINAL_PAY_AFTER_LAST_DAY", "Final-pay period cannot extend beyond the last working day.");
        const selectedPack = input.countryPackId ? await ports.thaiPackForRun({ id: runId, company_id: actor.companyId, pay_group_id: input.payGroupId, country_pack_id: input.countryPackId, period_start: input.periodStart, period_end: input.periodEnd, pay_date: input.payDate, currency: input.currency, mode: "final_pay" }) : null;
        const now = ports.clock.now(), result = { id: prepId, taskId: input.taskId, runId, status: "prepared", version: 1 };
        return ports.commands.commit(actor, key, "people.payroll.final-pay.prepare", input, prepId, 1, result, [
            repository.guard82(key, "task-version", input.taskId, actor.companyId, input.expectedTaskVersion, actor.userId),
            repository.guard83(key, "engagement-pay-group", task.engagement_id, actor.companyId, input.payGroupId, input.currency),
            repository.guard84(key, "one-final-pay", input.taskId),
            ...(selectedPack ? [repository.guard85(key, "statutory-pack", selectedPack.row.id, selectedPack.row.record_version, selectedPack.sourceDigest)] : []),
            repository.stagePrepareFinalPayEffect(runId, actor.companyId, input.payGroupId, input.countryPackId ?? null, task.engagement_id, input.periodStart, input.periodEnd, input.payDate, input.currency, actor.userId, now, now),
            repository.stagePrepareFinalPayEffect2(prepId, actor.companyId, input.taskId, task.case_id, task.engagement_id, runId, input.reason, actor.userId, now, now),
            repository.stagePrepareFinalPayEffect3(`final-pay-event-${key}`, prepId, input.taskId, runId, actor.companyId, actor.userId, input.expectedTaskVersion, JSON.stringify({ periodStart: input.periodStart, periodEnd: input.periodEnd, payDate: input.payDate, currency: input.currency, reason: input.reason }), now),
        ]);
    }
    async function requestPayrollAdvance(actor: WorkContext, input: z.infer<typeof payrollAdvanceRequestSchema>, key: string) {
        await ports.prepare(actor.holdingId);
        const id = `payroll-advance-${key}`, previous = await ports.commands.replay(actor, key, "people.payroll.advance.request", id, input);
        if (previous)
            return previous;
        const engagement = await repository.readRequestPayrollAdvanceEngagement(input.engagementId, actor.companyId);
        if (!engagement)
            throw new BusinessRuleError(404, "ENGAGEMENT_NOT_FOUND", "Employee engagement not found in this Company.");
        const payrollOwner = hasPermission(actor, "payroll.prepare"), ownRequest = hasPermission(actor, "self.requests") && samePerson(actor, engagement.user_id);
        if (!payrollOwner && !ownRequest)
            throw new BusinessRuleError(403, "PAYROLL_ADVANCE_REQUEST_FORBIDDEN", "Request your own advance or use an authorized Payroll preparation role.");
        if (!["active", "suspended"].includes(engagement.status))
            throw new BusinessRuleError(409, "ENGAGEMENT_NOT_ACTIVE", "Only an active employee can request a payroll advance.");
        if (!engagement.pay_group_id)
            throw new BusinessRuleError(409, "PAY_GROUP_REQUIRED", "Assign a pay group before requesting an advance.");
        if (input.currency !== engagement.currency)
            throw new BusinessRuleError(422, "CURRENCY_MISMATCH", "Advance currency must match the engagement payroll currency.");
        let amountMinor: number;
        try {
            amountMinor = decimalToMinor(input.amount);
        }
        catch {
            throw new BusinessRuleError(422, "INVALID_ADVANCE_AMOUNT", "Enter a valid advance amount.");
        }
        if (amountMinor <= 0)
            throw new BusinessRuleError(422, "INVALID_ADVANCE_AMOUNT", "Advance amount must be greater than zero.");
        const now = ports.clock.now(), result = { id, engagementId: engagement.id, personId: engagement.person_id, status: "pending", version: 1, amountMinor };
        return ports.commands.commit(actor, key, "people.payroll.advance.request", input, id, 1, result, [
            repository.guard90(key, "engagement", input.engagementId, actor.companyId, input.currency),
            repository.guard91(key, "no-pending-advance", input.engagementId),
            repository.stageRequestPayrollAdvanceEffect(id, actor.companyId, engagement.id, engagement.person_id, amountMinor, input.installmentCount, input.firstDeductionPeriod, input.currency, input.reason, actor.userId, now, now),
            repository.stageRequestPayrollAdvanceEffect2(`payroll-advance-event-${key}`, id, actor.companyId, actor.userId, 1, JSON.stringify({ amountMinor, installmentCount: input.installmentCount, firstDeductionPeriod: input.firstDeductionPeriod, currency: input.currency, reason: input.reason }), now),
            repository.stageRequestPayrollAdvanceEffect3(id, actor.companyId, id, now, actor.companyId, now, actor.userId),
        ]);
    }
    async function decidePayrollAdvance(actor: WorkContext, advanceId: string, input: z.infer<typeof payrollAdvanceDecisionSchema>, key: string) {
        if (input.action !== "cancel")
            actor = ports.authorize(actor, "payroll.approve");
        else if (!hasPermission(actor, "self.requests") && !hasPermission(actor, "payroll.prepare"))
            throw new BusinessRuleError(403, "PAYROLL_ADVANCE_CANCEL_FORBIDDEN", "Only the requester or Payroll owner can cancel this advance.");
        await ports.prepare(actor.holdingId);
        const previous = await ports.commands.replay(actor, key, `people.payroll.advance.${input.action}`, advanceId, input);
        if (previous)
            return previous;
        const row = await repository.readDecidePayrollAdvanceRow(advanceId, actor.companyId);
        if (!row)
            throw new BusinessRuleError(404, "PAYROLL_ADVANCE_NOT_FOUND", "Payroll advance not found.");
        if (row.status !== "pending" || Number(row.version) !== input.expectedVersion)
            throw new BusinessRuleError(409, "PAYROLL_ADVANCE_CHANGED", "This advance changed. Reload before deciding.");
        if (input.action === "cancel" && !samePerson(actor, row.requested_by) && !hasPermission(actor, "payroll.prepare"))
            throw new BusinessRuleError(403, "PAYROLL_ADVANCE_OWNER_REQUIRED", "Only the requester or Payroll owner can cancel this advance.");
        if (input.action !== "cancel" && samePerson(actor, row.requested_by))
            throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "A different person must decide the payroll advance.");
        const status = input.action === "approve" ? "approved" : input.action === "reject" ? "rejected" : "cancelled", version = input.expectedVersion + 1, now = ports.clock.now();
        const statements: Effect[] = [
            repository.guard96(key, "advance-version", advanceId, actor.companyId, input.expectedVersion),
            repository.stageDecidePayrollAdvanceStatements(status, status, actor.userId, input.reason, now, now, advanceId),
            repository.stageDecidePayrollAdvanceStatements2(`payroll-advance-event-${key}`, advanceId, actor.companyId, input.action === "approve" ? "approved" : status, actor.userId, version, JSON.stringify({ reason: input.reason, status }), now),
            repository.stageDecidePayrollAdvanceStatements3(`payroll-advance-result-${key}`, actor.companyId, row.requested_by, "payroll_advance", advanceId, version, status === "approved" ? "Payroll advance approved; deductions will appear in payroll" : "Payroll advance decision recorded", now),
        ];
        if (input.action === "approve") {
            const total = Number(row.amount_minor), count = Number(row.installment_count), base = Math.floor(total / count), remainder = total % count;
            for (let index = 0; index < count; index++)
                statements.push(repository.stageDecidePayrollAdvanceEffect(`payroll-advance-repayment-${key}-${index + 1}`, advanceId, actor.companyId, index + 1, advancePeriodStart(String(row.first_deduction_period), index), advancePeriodEnd(String(row.first_deduction_period), index), base + (index < remainder ? 1 : 0), now));
        }
        return ports.commands.commit(actor, key, `people.payroll.advance.${input.action}`, input, advanceId, version, { id: advanceId, status, version }, statements);
    }
    async function payrollAdvancePage(actor: WorkContext) {
        const privileged = hasPermission(actor, "payroll.read");
        if (!privileged && !hasPermission(actor, "self.requests"))
            throw new BusinessRuleError(403, "PAYROLL_ADVANCE_READ_FORBIDDEN", "Payroll advance access is not available for this role.");
        await ports.prepare(actor.holdingId);
        const rows = (await repository.readPayrollAdvancePageRows((privileged ? [actor.companyId] : [actor.companyId, actor.userId]), privileged)).results;
        const options = (await repository.readPayrollAdvancePageOptions((privileged ? [actor.companyId] : [actor.companyId, actor.userId]), privileged)).results;
        return { items: rows, options };
    }
    async function payrollAdvanceAlertTarget(companyId: string, userId: string, advanceId: string, now = ports.clock.now()) {
        return (await repository.readPayrollAdvanceAlertTargetEffect(advanceId, companyId, userId, userId, now)) ?? null;
    }
    async function calculatePayrollRun(actor: WorkContext, runId: string, key = ports.identifiers.next()) {
        actor = ports.authorize(actor, "payroll.prepare");
        await ports.prepare(actor.holdingId);
        const previous = await ports.commands.replay<{id:string;status:string;version:number;totals:{gross:number;deductions:number;employer:number;net:number;headcount:number}}>(actor, key, "people.payroll.calculate", runId, {});
        if (previous)
            return previous;
        const run = await repository.readCalculatePayrollRunRun(runId, actor.companyId);
        if (!run)
            throw new BusinessRuleError(404, "PAYROLL_RUN_NOT_FOUND", "Payroll run not found.");
        if (!["draft", "calculating", "preview", "failed"].includes(run.status) || !["preview", "final_pay", "production"].includes(run.mode))
            throw new BusinessRuleError(409, "PAYROLL_LOCKED", "Only an unapproved payroll preview can be recalculated.");
        // A run is company-wide; a branch preparer may not read or calculate other branches.
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Use a holding-level payroll assignment for company-wide runs.");
        if (run.adjustment_of_run_id)
            return ports.calculatePayrollAdjustmentRun(actor, run, key);
        const engagements = await repository.readCalculatePayrollRunEngagements(actor.companyId, run.pay_group_id, run.currency, run.period_end, run.period_start, (run as Record<string, unknown>).scope_engagement_id ?? null, (run as Record<string, unknown>).scope_engagement_id ?? null);
        if (!engagements.results.length)
            throw new BusinessRuleError(422, "PAYROLL_EMPTY", "Assign active employees to this pay group before calculating.");
        const group = await repository.readCalculatePayrollRunGroup(run.pay_group_id, actor.companyId);
        if (!group)
            throw new BusinessRuleError(409, "PAY_GROUP_CHANGED", "The pay group is no longer active.");
        const policy = { overtimeMultiplierBps: Number(group.overtime_multiplier_bps), prorationMethod: group.proration_method };
        const useSimulationAdapter = run.mode === "preview" && ports.isThaiHotelPayrollMockEnabled(actor.holdingSlug);
        const items = await Promise.all(engagements.results.map(e => calculateItem(run, e, policy, useSimulationAdapter)));
        if (items.some(i => !i.inputSnapshot.compensation || i.inputSnapshot.compensation.currency !== run.currency))
            throw new BusinessRuleError(422, "PAYROLL_INPUT_INCOMPLETE", "Every employee needs approved compensation in the pay-group currency covering the full period.");
        if (items.some(i => i.net < 0))
            throw new BusinessRuleError(422, "NEGATIVE_NET_PAY", "Review deductions exceeding gross pay.");
        const total = items.reduce((a, i) => ({ gross: a.gross + i.gross, deductions: a.deductions + i.deductions, employer: a.employer + i.employer, net: a.net + i.net }), { gross: 0, deductions: 0, employer: 0, net: 0 });
        if (Object.values(total).some(v => !Number.isSafeInteger(v)))
            throw new BusinessRuleError(422, "MONEY_OVERFLOW", "Payroll exceeds supported precision.");
        const digest = await ports.hash(JSON.stringify(items.map(i => i.inputSnapshot))), now = ports.clock.now();
        return ports.commands.commit(actor, key, "people.payroll.calculate", {}, runId, run.version + 1, { id: runId, status: "preview", version: run.version + 1, totals: { ...total, headcount: items.length } }, [
            repository.guard107(key, "run", runId, run.version),
            ...items.flatMap(i => i.inputSnapshot.thaiStatutory ? ports.thaiEvidenceGuards(key, i.inputSnapshot.thaiStatutory, run, i.engagementId) : []),
            ...items.flatMap((i, index) => [
                repository.guard108(key, `engagement-${index}`, i.engagementId, i.inputSnapshot.engagement.version),
                repository.guard109(key, `time-count-${index}`, i.engagementId, run.period_start, run.period_end, i.inputSnapshot.hours.entries.length),
                repository.guard110(key, `handoff-count-${index}`, i.engagementId, run.period_start, run.period_end, i.inputSnapshot.hours.handoffs.length),
                ...i.inputSnapshot.hours.entries.map(t => repository.guard111(key, `time-${index}-${t.id}`, t.id, t.version)),
                ...i.inputSnapshot.advanceRepayments.map(repayment => repository.guard112(key, `advance-repayment-${index}-${repayment.id}`, repayment.id, repayment.version, i.engagementId, actor.companyId)),
            ]),
            ...items.map((i, index) => repository.guard113(key, `compensation-${index}`, i.inputSnapshot.compensation!.id, i.inputSnapshot.compensation!.version)),
            repository.stageCalculatePayrollRunEffect(runId),
            repository.stageCalculatePayrollRunEffect2(JSON.stringify({ ...total, headcount: items.length }), digest, now, runId),
            ...items.map((i, index) => repository.stageCalculatePayrollRunEffect3(`payitem-${key}-${index}`, runId, i.engagementId, i.workerType, JSON.stringify(i.inputSnapshot), JSON.stringify(i.result), i.gross, i.deductions, i.employer, i.net, run.currency, now)),
        ]);
    }
    async function workPayrollRows(actor: WorkContext) {
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Company-wide payroll requires a holding-level assignment.");
        actor = ports.authorize(actor, "payroll.read");
        await ports.prepare(actor.holdingId);
        return (await repository.readWorkPayrollRowsEffect(actor.companyId)).results;
    }
    async function getPayrollRun(actor: WorkContext, runId: string) {
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Company-wide payroll requires a holding-level assignment.");
        const canReadPayroll = hasPermission(actor, "payroll.read"), canReadPayment = hasPermission(actor, "payroll.payment.read");
        if (!canReadPayroll && !canReadPayment)
            ports.authorize(actor, "payroll.read");
        const paymentOnly = !canReadPayroll;
        await ports.prepare(actor.holdingId);
        const run = await repository.readGetPayrollRunRun(runId, actor.companyId);
        if (!run)
            throw new BusinessRuleError(404, "PAYROLL_RUN_NOT_FOUND", "Payroll run not found.");
        const [items, exports, corrections, paymentBatch] = await Promise.all([repository.readGetPayrollRunItemsexportscorrectionspaymentBatch(runId), repository.readGetPayrollRunItemsexportscorrectionspaymentBatch2(runId), repository.readGetPayrollRunItemsexportscorrectionspaymentBatch3(runId, actor.companyId), repository.readGetPayrollRunItemsexportscorrectionspaymentBatch4(actor.companyId, runId)]);
        const paymentItems = paymentBatch ? (await repository.readGetPayrollRunPaymentItems(String(paymentBatch.id))).results : [];
        return { run: paymentOnly ? { ...run, totals_json: "{}", input_digest: null, error_message: null } : run, items: paymentOnly ? [] : items.results, exports: paymentOnly ? [] : exports.results, corrections: paymentOnly ? [] : corrections.results, paymentBatch, paymentItems };
    }
    async function listPayrollRuns(actor: WorkContext) {
        const canReadPayroll = hasPermission(actor, "payroll.read"), canReadPayment = hasPermission(actor, "payroll.payment.read");
        if (!canReadPayroll && !canReadPayment)
            ports.authorize(actor, "payroll.read");
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Company-wide payroll requires a holding-level assignment.");
        await ports.prepare(actor.holdingId);
        if (!canReadPayroll)
            return (await repository.readListPayrollRunsEffect(actor.companyId)).results;
        return (await repository.readListPayrollRunsEffect2(actor.companyId)).results;
    }
    async function payrollPostingOptions(actor: WorkContext) {
        actor = ports.authorize(actor, "finance.approve");
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Company-wide payroll requires a holding-level assignment.");
        await ports.prepare(actor.holdingId);
        const [charts, periods, mappings, groups] = await Promise.all([
            repository.readPayrollPostingOptionsChartsperiodsmappingsgroups(actor.companyId),
            repository.readPayrollPostingOptionsChartsperiodsmappingsgroups2(actor.companyId),
            repository.readPayrollPostingOptionsChartsperiodsmappingsgroups3(actor.companyId),
            repository.readPayrollPostingOptionsChartsperiodsmappingsgroups4(actor.companyId),
        ]);
        const chartIds = charts.results.map(row => String((row as Record<string, unknown>).id));
        const accounts = chartIds.length ? await repository.readPayrollPostingOptionsAccounts(actor.companyId, chartIds) : { results: [] };
        return { charts: charts.results, periods: periods.results, accounts: accounts.results, mappings: mappings.results, groups: groups.results };
    }
    async function resolvePayrollPost(actor: WorkContext, runId: string, input: z.infer<typeof payrollPostSchema>): Promise<ResolvedPayrollPost> {
        const run = await repository.readResolvePayrollPostRun(runId, actor.companyId);
        if (!run)
            throw new BusinessRuleError(404, "PAYROLL_RUN_NOT_FOUND", "Payroll run not found.");
        const mapping = await repository.readResolvePayrollPostMapping((input.accountMappingId ? [actor.companyId, run.pay_group_id, input.accountMappingId] : [actor.companyId, run.pay_group_id]), input);
        if (!mapping)
            throw new BusinessRuleError(409, "PAYROLL_ACCOUNT_MAPPING_REQUIRED", "An independently approved pay-group account mapping is required before payroll posting.");
        const saved = JSON.parse(mapping.accounts_json) as z.infer<typeof payrollAccountMappingSchema>;
        if (input.chartId !== undefined && input.chartId !== mapping.chart_id)
            throw new BusinessRuleError(422, "PAYROLL_MAPPING_OVERRIDE_FORBIDDEN", "The chart must match the approved pay-group mapping.");
        for (const [name, value] of Object.entries(input))
            if (name.endsWith("AccountId") && name !== "accountMappingId" && value !== undefined && value !== saved[name as keyof typeof saved])
                throw new BusinessRuleError(422, "PAYROLL_MAPPING_OVERRIDE_FORBIDDEN", "Payroll accounts must match the approved pay-group mapping.");
        const merged = { ...saved, ...input };
        const chartId = mapping.chart_id;
        if (!merged.expenseAccountId || !merged.netPayableAccountId)
            throw new BusinessRuleError(422, "PAYROLL_ACCOUNT_MAPPING_REQUIRED", "The approved mapping needs wage expense and net payable accounts.");
        const dateHint = input.entryDate ?? run.period_end;
        let entryDate = dateHint, periodId = input.periodId;
        if (!periodId) {
            let periods = (await repository.readResolvePayrollPostPeriods(actor.companyId, dateHint, dateHint)).results;
            if (!periods.length && !input.entryDate) {
                const next = (await repository.readResolvePayrollPostNext(actor.companyId, run.period_end)).results;
                if (next.length) {
                    entryDate = next[0].start_date;
                    periods = (await repository.readResolvePayrollPostEffect(actor.companyId, entryDate, entryDate)).results;
                }
            }
            if (periods.length !== 1)
                throw new BusinessRuleError(409, "PAYROLL_ACCOUNTING_PERIOD_REQUIRED", "Select one open accounting period for payroll accrual.");
            periodId = periods[0].id;
        }
        return { ...merged, chartId, periodId, entryDate, expenseAccountId: merged.expenseAccountId, netPayableAccountId: merged.netPayableAccountId, resolvedMappingId: mapping.id, resolvedMappingVersion: mapping.version };
    }
    async function savePayrollAccountMapping(actor: WorkContext, payGroupId: string, input: z.infer<typeof payrollAccountMappingSchema>, key: string) {
        actor = ports.authorize(actor, "finance.approve");
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Use a Company finance assignment for payroll mapping.");
        await ports.prepare(actor.holdingId);
        const id = `payroll-mapping-${key}`, previous = await ports.commands.replay(actor, key, "people.payroll.mapping.save", id, input);
        if (previous)
            return previous;
        const group = await repository.readSavePayrollAccountMappingGroup(payGroupId, actor.companyId);
        if (!group)
            throw new BusinessRuleError(404, "PAY_GROUP_NOT_FOUND", "Active pay group not found.");
        const chart = await repository.readSavePayrollAccountMappingChart(input.chartId, actor.companyId);
        if (!chart || chart.base_currency !== group.currency)
            throw new BusinessRuleError(422, "PAYROLL_CHART_CURRENCY", "Payroll chart must use the pay-group currency.");
        const ids = [...new Set(Object.entries(input).filter(([name]) => name.endsWith("AccountId")).map(([, value]) => value).filter((value): value is string => typeof value === "string"))];
        const accounts = await repository.readSavePayrollAccountMappingAccounts(actor.companyId, input.chartId, ids);
        if (accounts.results.length !== ids.length || accounts.results.some(account => account.currency !== group.currency))
            throw new BusinessRuleError(422, "PAYROLL_ACCOUNT_INVALID", "Every mapped account must be active in the selected chart and currency.");
        const categories = new Map(accounts.results.map(row => [row.id, row.category]));
        for (const [name, value] of Object.entries(input))
            if (name.endsWith("AccountId") && typeof value === "string") {
                const required = name === "advanceReceivableAccountId" ? "asset" : name.endsWith("ExpenseAccountId") || name === "expenseAccountId" ? "expense" : "liability";
                if (categories.get(value) !== required)
                    throw new BusinessRuleError(422, "PAYROLL_ACCOUNT_INVALID", `${name} must point to an active ${required} account.`);
            }
        const now = ports.clock.now();
        return ports.commands.commit(actor, key, "people.payroll.mapping.save", input, id, 1, { id, payGroupId, status: "draft", version: 1 }, [repository.stageSavePayrollAccountMappingEffect(id, actor.companyId, payGroupId, input.chartId, JSON.stringify(input), actor.userId, now, now)]);
    }
    async function approvePayrollAccountMapping(actor: WorkContext, mappingId: string, expectedVersion: number, key: string) {
        actor = ports.authorize(actor, "finance.approve");
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Use a Company finance assignment for payroll mapping.");
        await ports.prepare(actor.holdingId);
        const input = { expectedVersion }, previous = await ports.commands.replay(actor, key, "people.payroll.mapping.approve", mappingId, input);
        if (previous)
            return previous;
        const row = await repository.readApprovePayrollAccountMappingRow(mappingId, actor.companyId);
        if (!row)
            throw new BusinessRuleError(404, "PAYROLL_MAPPING_NOT_FOUND", "Payroll account mapping not found.");
        if (row.status !== "draft" || row.version !== expectedVersion)
            throw new BusinessRuleError(409, "PAYROLL_MAPPING_CHANGED", "Refresh this mapping before approval.");
        if (samePerson(actor, row.created_by))
            throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "A different Finance approver must approve the mapping.");
        const now = ports.clock.now();
        return ports.commands.commit(actor, key, "people.payroll.mapping.approve", input, mappingId, expectedVersion + 1, { id: mappingId, status: "approved", version: expectedVersion + 1 }, [
            repository.guard141(key, "mapping-review", mappingId, actor.companyId, expectedVersion, actor.userId),
            repository.stageApprovePayrollAccountMappingEffect(now, actor.companyId, row.pay_group_id),
            repository.stageApprovePayrollAccountMappingEffect2(actor.userId, now, mappingId),
        ]);
    }
    async function preparePayrollAccrual(actor: WorkContext, runId: string, requested: z.infer<typeof payrollPostSchema>, key: string) {
        const input = await resolvePayrollPost(actor, runId, requested);
        const run = await repository.readPreparePayrollAccrualRun(runId, actor.companyId);
        if (!run)
            throw new BusinessRuleError(404, "PAYROLL_RUN_NOT_FOUND", "Payroll run not found.");
        if (String(run.status) !== "approved")
            throw new BusinessRuleError(409, "PAYROLL_NOT_APPROVED", "An independently approved payroll run is required before GL posting.");
        if (Number(run.version) !== input.expectedVersion)
            throw new BusinessRuleError(409, "VERSION_CONFLICT", "The payroll run changed. Refresh before posting.");
        if (Number(run.item_count) <= 0 || Number(run.invalid_items) > 0)
            throw new BusinessRuleError(409, "PAYROLL_ITEMS_INVALID", "Every payroll item must be calculated or approved before posting.");
        await ports.assertPayrollSourceFresh(runId);
        if (input.entryDate < String(run.period_end))
            throw new BusinessRuleError(422, "PAYROLL_ACCRUAL_DATE_INVALID", "Recognize payroll expense at period end or later in an open adjustment period.");
        const items = (await repository.readPreparePayrollAccrualItems(runId)).results;
        if (items.some(item => item.worker_type !== "employee"))
            throw new BusinessRuleError(409, "CONTRACTOR_PAYABLE_ROUTE", "Contractor amounts must be handled through Finance payables, not posted as payroll.");
        const accrual = payrollAccountingLines(items, input);
        const expected = JSON.parse(String(run.totals_json)) as {
            gross: number;
            deductions: number;
            employer: number;
            net: number;
        };
        if (accrual.totals.grossMinor !== expected.gross || accrual.totals.deductionsMinor !== expected.deductions || accrual.totals.employerMinor !== expected.employer || accrual.totals.netMinor !== expected.net)
            throw new BusinessRuleError(409, "PAYROLL_TOTALS_INVALID", "Approved run totals changed after review.");
        const journal = await ports.prepareJournal(actor, {
            chartId: input.chartId, periodId: input.periodId, entryDate: input.entryDate,
            currency: String(run.currency),
            memo: `Payroll ${String(run.period_start)} to ${String(run.period_end)}${input.entryDate > String(run.period_end) ? " · adjustment for closed work period" : ""}`,
            sourceType: "payroll", sourceId: runId, isOpening: false, lines: accrual.lines,
        }, key, "posted");
        const roles = new Map<string, string>();
        const setRole = (id: string, role: string) => {
            const previous = roles.get(id);
            if (previous && previous !== role)
                throw new BusinessRuleError(422, "PAYROLL_ACCOUNT_INVALID", "An account cannot serve incompatible payroll expense, liability or asset roles.");
            roles.set(id, role);
        };
        for (const id of [input.expenseAccountId, input.employerExpenseAccountId, input.overtimeExpenseAccountId, input.bonusExpenseAccountId, input.serviceChargeExpenseAccountId].filter((id): id is string => !!id))
            setRole(id, "expense");
        for (const id of [input.netPayableAccountId, input.taxPayableAccountId, input.employerContributionAccountId, input.socialSecurityPayableAccountId, input.benefitPayableAccountId, input.otherDeductionPayableAccountId].filter((id): id is string => !!id))
            setRole(id, "liability");
        if (input.advanceReceivableAccountId)
            setRole(input.advanceReceivableAccountId, "asset");
        const ids = [...roles.keys()];
        const valid = await repository.readPreparePayrollAccrualValid(actor.companyId, input.chartId, ids);
        if (valid.results.length !== ids.length || valid.results.some(row => row.category !== roles.get(row.id) || row.currency !== run.currency))
            throw new BusinessRuleError(422, "PAYROLL_ACCOUNT_INVALID", "Payroll mapping needs active wage expense, payable liability, and advance asset accounts in the run currency.");
        return { run, accrual, journal, input };
    }
    async function previewPayrollAccounting(actor: WorkContext, runId: string, input: z.infer<typeof payrollPostSchema>) {
        actor = ports.authorize(actor, "finance.approve");
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Company-wide payroll requires a holding-level assignment.");
        await ports.prepare(actor.holdingId);
        const { run, accrual, input: resolved } = await preparePayrollAccrual(actor, runId, input, `preview-${ports.identifiers.next()}`);
        return { runId, runVersion: Number(run.version), entryDate: resolved.entryDate, periodId: resolved.periodId, accountMappingId: resolved.resolvedMappingId, accountMappingVersion: resolved.resolvedMappingVersion, lines: accrual.lines, totals: accrual.totals, categories: accrual.categories };
    }
    async function postPayroll(actor: WorkContext, runId: string, input: z.infer<typeof payrollPostSchema>, key: string) {
        actor = ports.authorize(actor, "finance.approve");
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Company-wide payroll requires a holding-level assignment.");
        await ports.prepare(actor.holdingId);
        const previous = await ports.commands.replay<{id:string;status:string;version:number;journalId:string;netPayableMinor:number;payslipIds:string[]}>(actor, key, "people.payroll.post", runId, input);
        if (previous)
            return previous;
        const { run, accrual, journal, input: resolved } = await preparePayrollAccrual(actor, runId, input, `${key}-journal`);
        if (run.mode === "production" || run.country_pack_id || (await ports.environment()) === "production")
            await ports.assertStatutoryRunReady(runId);
        const moneySnapshot = (await ports.snapshotMoney(actor, { sourceType: "payroll_run", sourceId: runId, amountKind: "net_payable", transactionDate: String(run.pay_date), currency: String(run.currency), currencyScale: 2, amountUnits: String(accrual.totals.netMinor) })).statement;
        const payslips = await ports.preparePayslipIssue(actor, runId, String(journal.result.id), key, input.expectedVersion + 1);
        const deductions = await ports.preparePayrollDeductionConsumption(actor, runId, String(journal.result.id), resolved.advanceReceivableAccountId, key);
        const now = ports.clock.now(), result = { id: runId, status: "posted", version: input.expectedVersion + 1, journalId: journal.result.id, netPayableMinor: accrual.totals.netMinor, payslipIds: payslips.ids };
        return ports.commands.commit(actor, key, "people.payroll.post", input, runId, input.expectedVersion + 1, result, [
            repository.guard147(key, "payroll-run", runId, actor.companyId, input.expectedVersion),
            ...(resolved.resolvedMappingId ? [repository.guard148(key, "payroll-mapping", resolved.resolvedMappingId, actor.companyId, runId, resolved.resolvedMappingVersion)] : []),
            ...deductions.guards,
            ...await ports.payrollSourceGuards(key, runId),
            repository.guard149(key, "payroll-items", runId),
            repository.guard150(key, "payroll-posting-unique", runId),
            ...payslips.guards,
            ...journal.steps,
            repository.stagePostPayrollEffect(actor.userId, now, runId, actor.companyId, input.expectedVersion),
            repository.stagePostPayrollEffect2(runId),
            repository.stagePostPayrollEffect3(`payroll-posting-${key}`, runId, journal.result.id, resolved.resolvedMappingId, resolved.resolvedMappingVersion, now),
            ...deductions.steps,
            ...payslips.steps,
            ...(moneySnapshot ? [moneySnapshot] : []),
        ]);
    }
    async function approvePayrollRun(actor: WorkContext, runId: string, expectedVersion: number, key: string) {
        actor = ports.authorize(actor, "payroll.approve");
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Company-wide payroll requires a holding-level assignment.");
        await ports.prepare(actor.holdingId);
        const input = { expectedVersion };
        const previous = await ports.commands.replay(actor, key, "people.payroll.approve", runId, input);
        if (previous)
            return previous;
        const submitted = await repository.readApprovePayrollRunSubmitted(runId, actor.companyId);
        if (submitted?.submitted_by && samePerson(actor, submitted.submitted_by))
            throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "A different person must approve the submitted payroll revision.");
        await ports.assertPayrollSourceFresh(runId);
        return ports.commands.commit(actor, key, "people.payroll.approve", input, runId, expectedVersion + 1, { id: runId, status: "approved", version: expectedVersion + 1 }, [
            repository.guard155(key, "run", runId, actor.companyId, expectedVersion, actor.userId),
            ...await ports.payrollSourceGuards(key, runId),
            repository.stageApprovePayrollRunEffect(actor.userId, ports.clock.now(), runId),
            repository.stageApprovePayrollRunEffect2(ports.identifiers.next(), key, runId, "people.payroll.approve", actor.userId, actor.assignmentId, expectedVersion, ports.clock.now(), runId),
        ]);
    }
    async function requestPayrollCorrection(actor: WorkContext, runId: string, input: z.infer<typeof payrollCorrectionRequestSchema>, key: string) {
        actor = ports.authorize(actor, "payroll.prepare");
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Company-wide payroll requires a holding-level assignment.");
        await ports.prepare(actor.holdingId);
        const id = `payroll-correction-${key}`, previous = await ports.commands.replay(actor, key, "people.payroll.correction.request", id, input);
        if (previous)
            return previous;
        const run = await repository.readRequestPayrollCorrectionRun(runId, actor.companyId);
        if (!run)
            throw new BusinessRuleError(404, "PAYROLL_RUN_NOT_FOUND", "Payroll run not found.");
        if (run.status !== "approved" || run.version !== input.expectedVersion || !run.input_digest)
            throw new BusinessRuleError(409, "PAYROLL_CORRECTION_SOURCE_CHANGED", "Only the current approved payroll revision can be returned for correction.");
        const attachmentBinding = input.attachmentSetIds.length ? await ports.prepareAttachmentBinding(actor, { attachmentSetIds: input.attachmentSetIds, bindingContract: { targetType: "payroll_run", bindingType: "payroll_correction_request", contexts: [{ purposeKey: "correction_support", componentKind: "action", componentRef: "correction-request" }] }, targetId: runId, expectedTargetVersion: input.expectedVersion, commandId: key, bindingId: id }) : null;
        const now = ports.clock.now(), result = { id, runId, status: "pending", version: 1 };
        return ports.commands.commit(actor, key, "people.payroll.correction.request", input, id, 1, result, [
            ...(attachmentBinding?.guards ?? []),
            repository.guard159(key, "approved-run", runId, actor.companyId, input.expectedVersion, run.revision, run.input_digest),
            repository.stageRequestPayrollCorrectionEffect(id, runId, actor.companyId, run.version, run.revision, run.input_digest, run.totals_json, input.reason, actor.userId, now, now),
            repository.stageRequestPayrollCorrectionEffect2(`payroll-correction-event-${key}`, id, runId, actor.companyId, actor.userId, run.version, JSON.stringify({ reason: input.reason, runRevision: run.revision, inputDigest: run.input_digest, totals: JSON.parse(run.totals_json) }), now),
            repository.stageRequestPayrollCorrectionEffect3(id, actor.companyId, id, now, actor.companyId, now, actor.userId),
            ...(attachmentBinding?.steps ?? []),
        ]);
    }
    async function decidePayrollCorrection(actor: WorkContext, correctionId: string, input: z.infer<typeof payrollCorrectionDecisionSchema>, key: string) {
        const permission = input.action === "cancel" ? "payroll.prepare" : "payroll.approve";
        actor = ports.authorize(actor, permission);
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Company-wide payroll requires a holding-level assignment.");
        await ports.prepare(actor.holdingId);
        const previous = await ports.commands.replay(actor, key, `people.payroll.correction.${input.action}`, correctionId, input);
        if (previous)
            return previous;
        const row = await repository.readDecidePayrollCorrectionRow(correctionId, actor.companyId);
        if (!row)
            throw new BusinessRuleError(404, "PAYROLL_CORRECTION_NOT_FOUND", "Payroll correction request not found.");
        if (row.status !== "pending" || Number(row.version) !== input.expectedVersion)
            throw new BusinessRuleError(409, "PAYROLL_CORRECTION_CHANGED", "This correction request changed. Reload before deciding.");
        if (input.action === "cancel" && !samePerson(actor, row.requested_by))
            throw new BusinessRuleError(403, "PAYROLL_CORRECTION_OWNER_REQUIRED", "Only the requester can cancel this correction request.");
        if (input.action !== "cancel" && samePerson(actor, row.requested_by))
            throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "A different person must decide the payroll correction.");
        if (row.run_status !== "approved" || Number(row.current_run_version) !== Number(row.run_version) || Number(row.current_run_revision) !== Number(row.run_revision) || row.current_input_digest !== row.input_digest)
            throw new BusinessRuleError(409, "PAYROLL_CORRECTION_SOURCE_CHANGED", "The approved payroll evidence changed. Start a new correction review.");
        const status = input.action === "cancel" ? "cancelled" : input.action === "approve" ? "approved" : "rejected", now = ports.clock.now(), version = input.expectedVersion + 1, result = { id: correctionId, runId: String(row.run_id), status, version, runStatus: input.action === "approve" ? "calculating" : "approved" };
        const statements: Effect[] = [
            repository.guard164(key, "correction-version", correctionId, actor.companyId, input.expectedVersion),
            repository.guard165(key, "run-evidence", row.run_id, actor.companyId, row.run_version, row.run_revision, row.input_digest),
            repository.stageDecidePayrollCorrectionStatements(status, actor.userId, input.reason, now, now, correctionId),
            repository.stageDecidePayrollCorrectionStatements2(`payroll-correction-event-${key}`, correctionId, row.run_id, actor.companyId, status, actor.userId, row.run_version, JSON.stringify({ reason: input.reason, runRevision: row.run_revision, inputDigest: row.input_digest }), now),
            repository.stageDecidePayrollCorrectionStatements3(`payroll-correction-result-${key}`, actor.companyId, row.requested_by, "payroll_correction", correctionId, version, input.action === "approve" ? "Payroll correction approved; recalculate the run with current inputs" : input.action === "reject" ? "Payroll correction rejected; review the decision reason" : "Payroll correction request cancelled", now),
        ];
        if (input.action === "approve")
            statements.push(repository.stageDecidePayrollCorrectionEffect(`Correction approved: ${input.reason}`, now, row.run_id));
        return ports.commands.commit(actor, key, `people.payroll.correction.${input.action}`, input, correctionId, version, result, statements);
    }
    async function payrollCorrectionAlertTarget(companyId: string, userId: string, correctionId: string, now = ports.clock.now()) {
        return (await repository.readPayrollCorrectionAlertTargetEffect(userId, correctionId, companyId, now)) ?? null;
    }
    async function exportPayrollReview(actor: WorkContext, runId: string, key: string) {
        actor = ports.authorize(actor, "payroll.read");
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Company-wide payroll requires a holding-level assignment.");
        await ports.prepare(actor.holdingId);
        const previous = await ports.commands.replay(actor, key, "people.payroll.review-export", runId, {});
        if (previous)
            return previous;
        const run = await repository.readExportPayrollReviewRun(runId, actor.companyId);
        if (!run || !["preview", "in_review", "approved"].includes(run.status) || !run.input_digest)
            throw new BusinessRuleError(409, "PAYROLL_REVIEW_NOT_READY", "Calculate a payroll preview before exporting review evidence.");
        await ports.assertPayrollSourceFresh(runId);
        const existing = await repository.readExportPayrollReviewExisting(runId, run.revision);
        if (existing)
            return { id: runId, revision: run.revision, checksum: existing.checksum_sha256 };
        const items = await repository.readExportPayrollReviewItems(runId);
        const csv = ["employee,worker_type,gross,deductions,employer_cost,net,currency,input_snapshot", ...items.results.map(item => [item.legal_name, item.worker_type, minorToDecimal(item.gross_minor), minorToDecimal(item.deductions_minor), minorToDecimal(item.employer_cost_minor), minorToDecimal(item.net_minor), item.currency, item.input_snapshot_json].map(payrollCsvCell).join(","))].join("\n"), checksum = await ports.hash(csv), fileKey = `${actor.holdingId}/people/payroll/${runId}/review-${run.revision}.csv`, exportId = `payroll-review-${key}`, now = ports.clock.now();
        await ports.files.put(fileKey, csv, { httpMetadata: { contentType: "text/csv" }, customMetadata: { sha256: checksum, inputDigest: run.input_digest, period: `${run.period_start}:${run.period_end}` } });
        return ports.commands.commit(actor, key, "people.payroll.review-export", {}, runId, Number(run.revision), { id: runId, revision: run.revision, checksum }, [
            repository.guard174(key, "run", runId, actor.companyId, run.revision, run.input_digest),
            ...await ports.payrollSourceGuards(key, runId),
            repository.stageExportPayrollReviewEffect(exportId, runId, run.revision, fileKey, checksum, actor.userId, now),
        ]);
    }
    async function getPayrollReviewFile(actor: WorkContext, runId: string, revision: number) {
        actor = ports.authorize(actor, "payroll.read");
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Company-wide payroll requires a holding-level assignment.");
        await ports.prepare(actor.holdingId);
        const row = await repository.readGetPayrollReviewFileRow(runId, revision, actor.companyId);
        if (!row)
            throw new BusinessRuleError(404, "PAYROLL_REVIEW_EXPORT_NOT_FOUND", "Payroll review export not found.");
        const object = await ports.files.get(row.file_key);
        if (!object)
            throw new BusinessRuleError(503, "PAYROLL_REVIEW_EXPORT_UNAVAILABLE", "Payroll review export is temporarily unavailable.");
        return { object, checksum: row.checksum_sha256 };
    }
    async function createPaymentBatch(actor: WorkContext, input: z.infer<typeof paymentBatchSchema>, commandId: string) {
        actor = ports.authorize(actor, "payroll.payment.prepare");
        await ports.prepare(actor.holdingId);
        const c = await command(actor, commandId, "people.payment-batch.create", input);
        if (c.result)
            return c.result;
        if (input.sourceType === "payroll" && (actor.branchId || actor.departmentId))
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Use a Company assignment to prepare payroll payment.");
        if (input.sourceType === "payroll" && (await ports.environment()) === "production")
            await ports.assertStatutoryRunReady(input.sourceId);
        const duplicate = await repository.readCreatePaymentBatchDuplicate(actor.companyId, input.sourceType, input.sourceId);
        if (duplicate)
            throw new BusinessRuleError(409, "PAYMENT_BATCH_EXISTS", "An outstanding payment batch already exists for this obligation.");
        let currency = "";
        let items: {
            results: Array<{
                id: string;
                engagement_id: string | null;
                person_id: string | null;
                legal_name: string;
                preferred_name: string | null;
                net_minor: number;
                currency: string;
                input_snapshot_json?: string;
            }>;
        };
        if (input.sourceType === "payroll") {
            const source = await repository.readCreatePaymentBatchSource(input.sourceId, actor.companyId);
            if (!source || source.status !== "posted")
                throw new BusinessRuleError(409, "PAYROLL_NOT_POSTED", "Payroll must be posted before payment export.");
            currency = source.currency;
            const posted = (await repository.readCreatePaymentBatchPosted(input.sourceId)).results;
            items = { results: posted.map(item => {
                    const snapshot = JSON.parse(item.input_snapshot_json) as {
                        engagement?: {
                            person_id: string;
                        };
                        payee?: {
                            id: string;
                            legal_name: string;
                            preferred_name: string | null;
                        };
                    };
                    const payee = snapshot.payee;
                    if (!payee?.id || !payee.legal_name || payee.id !== snapshot.engagement?.person_id)
                        throw new BusinessRuleError(409, "PAYROLL_PAYEE_REVIEW_REQUIRED", "The posted payroll item lacks an approved employee payment identity.");
                    return { ...item, person_id: payee.id, legal_name: payee.legal_name, preferred_name: payee.preferred_name };
                }) };
        }
        else if (input.sourceType === "expense") {
            throw new BusinessRuleError(409, "EXPENSE_PAYABLE_REQUIRED", "Record reimbursement against the posted expense payable.");
        }
        else {
            const invoice = await repository.readCreatePaymentBatchInvoice(input.sourceId, actor.companyId);
            if (!invoice || invoice.status !== "posted")
                throw new BusinessRuleError(409, "CONTRACTOR_INVOICE_NOT_POSTED", "A posted contractor payable is required before payment export.");
            currency = invoice.currency;
            items = { results: [{ id: invoice.id, engagement_id: null, person_id: null, legal_name: invoice.legal_name, preferred_name: null, net_minor: Number(invoice.total_minor), currency: invoice.currency }] };
        }
        if (!items.results.length)
            throw new BusinessRuleError(409, "PAYMENT_ITEMS_EMPTY", "There are no approved payable items to export.");
        if (input.sourceType === "payroll" && input.bankAccountId) {
            const bank = await repository.readCreatePaymentBatchBank(input.bankAccountId, actor.companyId, currency);
            if (!bank)
                throw new BusinessRuleError(422, "BANK_ACCOUNT_INVALID", "Select an active bank asset account in the payroll currency.");
        }
        const total = items.results.reduce((sum, item) => sum + item.net_minor, 0);
        if (total <= 0)
            throw new BusinessRuleError(422, "PAYMENT_TOTAL_INVALID", "Payment amount must be greater than zero.");
        const batchId = `payment-${ports.identifiers.next()}`;
        const now = ports.clock.now();
        const statements: Effect[] = [
            ...(input.sourceType === "payroll" ? [repository.guard182(commandId, "posted-payroll-source", input.sourceId, actor.companyId)] : []),
            ...items.results.filter((item): item is typeof item & {
                input_snapshot_json: string;
            } => !!item.input_snapshot_json).map((item, index) => repository.guard183(commandId, `posted-payment-item-${index}`, item.id, input.sourceId, item.net_minor, item.input_snapshot_json)),
            repository.stageCreatePaymentBatchStatements(batchId, actor.companyId, input.sourceType, input.sourceId, currency, "draft", input.formatCode, total, input.sourceType === "payroll" ? input.bankAccountId ?? null : null, actor.userId, now, now)
        ];
        if (input.sourceType === 'contractor')
            statements.push(repository.guard185(commandId, 'supplier-payment-reservation', input.sourceId), repository.guard186(commandId, 'contractor-batch-unique', actor.companyId, input.sourceId, batchId), repository.stageCreatePaymentBatchEffect(now, input.sourceId, actor.companyId));
        for (const item of items.results) {
            const reviewed = item.input_snapshot_json ? (JSON.parse(item.input_snapshot_json) as {
                payee?: {
                    legal_name: string;
                    preferred_name: string | null;
                };
            }).payee : null;
            statements.push(repository.stageCreatePaymentBatchEffect2(`payitem-${ports.identifiers.next()}`, batchId, item.engagement_id, item.person_id, `PAY-${input.sourceId}-${item.id}`, reviewed ? (reviewed.preferred_name ?? reviewed.legal_name) : (item.preferred_name ?? item.legal_name), item.net_minor, item.currency));
        }
        const result = { id: batchId, status: "draft", totalMinor: total, itemCount: items.results.length };
        statements.push(receipt(commandId, "people.payment-batch.create", c.digest, result));
        await ports.commands.commit(actor, commandId, "people.payment-batch.create", input, batchId, 1, result, statements);
        return result;
    }
    async function preparePaymentRetry(actor: WorkContext, batchId: string, expectedVersion: number, commandId: string) {
        actor = ports.authorize(actor, "payroll.payment.prepare");
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Use a Company assignment to retry payroll payments.");
        await ports.prepare(actor.holdingId);
        const input = { expectedVersion };
        const prior = await ports.commands.replay(actor, commandId, "people.payment-batch.retry", batchId, input);
        if (prior)
            return prior;
        const batch = await repository.readPreparePaymentRetryBatch(batchId, actor.companyId);
        if (!batch)
            throw new BusinessRuleError(404, "PAYMENT_BATCH_NOT_FOUND", "Payment batch not found.");
        if (batch.source_type !== "payroll" || batch.status !== "partially_settled" || batch.version !== expectedVersion)
            throw new BusinessRuleError(409, "PAYMENT_RETRY_NOT_READY", "Refresh the partially settled payroll batch before preparing a retry.");
        if ((await ports.environment()) === "production")
            await ports.assertStatutoryRunReady(batch.source_id);
        const failed = await repository.readPreparePaymentRetryFailed(batchId);
        if (!Number(failed?.count))
            throw new BusinessRuleError(409, "PAYMENT_RETRY_NOT_READY", "Record at least one bank rejection before preparing a retry.");
        const now = ports.clock.now();
        const result = { id: batchId, status: "draft", version: expectedVersion + 1, retryItems: Number(failed?.count ?? 0) };
        return ports.commands.commit(actor, commandId, "people.payment-batch.retry", input, batchId, result.version, result, [
            repository.guard191(commandId, "batch-retry-version", batchId, actor.companyId, expectedVersion),
            repository.guard192(commandId, "batch-retry-items", batchId),
            repository.stagePreparePaymentRetryEffect(actor.userId, now, batchId, actor.companyId, expectedVersion),
        ]);
    }
    async function approvePaymentBatch(actor: WorkContext, batchId: string, expectedVersion: number, commandId: string) {
        actor = ports.authorize(actor, "finance.pay");
        await ports.prepare(actor.holdingId);
        const prior = await ports.commands.replay(actor, commandId, "people.payment-batch.approve", batchId, { expectedVersion });
        if (prior)
            return prior;
        const batch = await repository.readApprovePaymentBatchBatch(batchId, actor.companyId);
        if (!batch)
            throw new BusinessRuleError(404, "PAYMENT_BATCH_NOT_FOUND", "Payment batch not found.");
        if (batch.source_type === "payroll" && (actor.branchId || actor.departmentId))
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Use a Company assignment to approve payroll payment.");
        if (samePerson(actor, batch.retry_requested_by ?? batch.created_by))
            throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "A different person must approve the payment batch.");
        if (batch.status !== "draft" || batch.version !== expectedVersion)
            throw new BusinessRuleError(409, "PAYMENT_BATCH_NOT_APPROVABLE", "Refresh the payment batch before approving it.");
        const now = ports.clock.now();
        await ports.commands.commit(actor, commandId, "people.payment-batch.approve", { expectedVersion }, batchId, 1, { id: batchId, status: "approved", version: expectedVersion + 1 }, [repository.guard195(commandId, "batch-version", batchId, actor.companyId, expectedVersion, actor.userId), repository.stageApprovePaymentBatchEffect(actor.userId, now, batchId, actor.companyId, expectedVersion), repository.stageApprovePaymentBatchEffect2(ports.identifiers.next(), commandId, batchId, "people.payment-batch.approve", actor.userId, actor.assignmentId, expectedVersion, commandId, now)]);
        return { id: batchId, status: "approved", version: expectedVersion + 1 };
    }
    async function exportPaymentBatch(actor: WorkContext, batchId: string, commandId: string) { return exportPaymentBatchV2(actor, batchId, commandId); }
    async function exportPaymentBatchV2(actor: WorkContext, batchId: string, commandId: string) {
        actor = ports.authorize(actor, "payroll.payment.execute");
        await ports.prepare(actor.holdingId);
        const prior = await ports.commands.replay(actor, commandId, "people.payment-batch.export", batchId, {});
        if (prior)
            return prior;
        const batch = await repository.readExportPaymentBatchV2Batch(batchId, actor.companyId);
        if (!batch)
            throw new BusinessRuleError(404, "PAYMENT_BATCH_NOT_FOUND", "Payment batch not found.");
        if (batch.source_type === "payroll" && (actor.branchId || actor.departmentId))
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Use a Company assignment to export payroll payment.");
        if (batch.source_type === "payroll" && (await ports.environment()) === "production")
            await ports.assertStatutoryRunReady(batch.source_id);
        if (batch.status !== "approved")
            throw new BusinessRuleError(409, "PAYMENT_BATCH_LOCKED", "An independently approved payment batch is required before export.");
        const eligibleStatus = batch.retry_requested_by ? "failed" : "outstanding";
        const items = (await repository.readExportPaymentBatchV2Items(batchId, eligibleStatus)).results;
        if (!items.length)
            throw new BusinessRuleError(409, "PAYMENT_ITEMS_EMPTY", "No outstanding payment instructions remain.");
        const payrollItemGuards: Effect[] = [];
        if (batch.source_type === "payroll") {
            for (const [index, item] of items.entries()) {
                const posted = await repository.readExportPaymentBatchV2Posted(batch.source_id, item.reference, batch.source_id);
                if (!posted)
                    throw new BusinessRuleError(409, "PAYROLL_PAYEE_REVIEW_REQUIRED", "The payment instruction no longer matches a posted payroll item.");
                const snapshot = JSON.parse(posted.input_snapshot_json) as {
                    payee?: {
                        id: string;
                        legal_name: string;
                        preferred_name: string | null;
                    };
                };
                const payee = snapshot.payee;
                if (!payee?.id || item.person_id !== payee.id || item.payee_name !== (payee.preferred_name ?? payee.legal_name) || item.amount_minor !== posted.net_minor || item.currency !== posted.currency)
                    throw new BusinessRuleError(409, "PAYROLL_PAYEE_REVIEW_REQUIRED", "The payment instruction differs from the approved payroll identity or amount.");
                payrollItemGuards.push(repository.guard201(commandId, `payment-item-${index}`, batch.source_id, posted.id, item.id, batchId, eligibleStatus, item.reference, item.person_id, item.payee_name, item.amount_minor, item.currency, posted.net_minor, posted.currency, posted.input_snapshot_json));
            }
        }
        const total = items.reduce((sum, item) => sum + Number(item.amount_minor), 0);
        if (!Number.isSafeInteger(total) || total <= 0)
            throw new BusinessRuleError(422, "PAYMENT_TOTAL_INVALID", "Export amount is invalid.");
        const csv = ["reference,payee,amount,currency,status", ...items.map(item => [item.reference, item.payee_name, minorToDecimal(item.amount_minor), item.currency, "outstanding"].map(payrollCsvCell).join(","))].join("\n");
        const fileKey = actor.holdingId + "/people/payments/" + batchId + "/attempt-" + batch.version + ".csv";
        const checksum = await ports.hash(csv), now = ports.clock.now();
        await ports.files.put(fileKey, csv, { httpMetadata: { contentType: "text/csv" }, customMetadata: { sha256: checksum, format: batch.format_code, synthetic: "true" } });
        const result = { id: batchId, status: "exported", fileKey, checksum, itemCount: items.length, totalMinor: total, synthetic: true };
        return ports.commands.commit(actor, commandId, "people.payment-batch.export", {}, batchId, batch.version + 1, result, [
            repository.guard202(commandId, "batch-export-version", batchId, actor.companyId, batch.version),
            repository.guard203(commandId, "batch-export-items", batchId, eligibleStatus, items.length),
            ...payrollItemGuards,
            ...(batch.source_type === "payroll" ? [repository.guard204(commandId, "posted-payroll-source", batch.source_id, actor.companyId)] : []),
            repository.stageExportPaymentBatchV2Effect("payment-export-" + commandId, actor.companyId, batchId, batch.version, fileKey, checksum, items.length, total, actor.userId, now),
            repository.stageExportPaymentBatchV2Effect2(fileKey, checksum, now, now, batchId, actor.companyId, batch.version),
        ]);
    }
    async function confirmPaymentBatch(actor: WorkContext, batchId: string, input: z.infer<typeof paymentConfirmationSchema>, commandId: string) {
        actor = ports.authorize(actor, "finance.pay");
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Use a Company finance assignment to confirm payroll payments.");
        await ports.prepare(actor.holdingId);
        const previous = await ports.commands.replay(actor, commandId, "people.payment-batch.confirm", batchId, input);
        if (previous)
            return previous;
        const batch = await repository.readConfirmPaymentBatchBatch(batchId, actor.companyId);
        if (!batch)
            throw new BusinessRuleError(404, "PAYMENT_BATCH_NOT_FOUND", "Payment batch not found.");
        if (batch.source_type === "payroll" && (await ports.environment()) === "production")
            await ports.assertStatutoryRunReady(batch.source_id);
        if (!["exported", "partially_settled"].includes(batch.status))
            throw new BusinessRuleError(409, "PAYMENT_BATCH_NOT_CONFIRMABLE", "Export the payment batch first.");
        const items = (await repository.readConfirmPaymentBatchItems(batchId)).results;
        if (new Set(input.rows.map(r => r.reference)).size !== input.rows.length || input.rows.some(r => !items.some(i => i.reference === r.reference && ["outstanding", "failed"].includes(i.status))))
            throw new BusinessRuleError(409, "PAYMENT_ITEMS_CHANGED", "Review the current outstanding payment items.");
        const remaining = items.filter(i => i.status !== "settled" && !input.rows.some(r => r.reference === i.reference && r.status === "confirmed")).length;
        const status = remaining ? "partially_settled" : "settled", now = ports.clock.now(), steps: Effect[] = [repository.guard209(commandId, "payment-batch-version", batchId, actor.companyId, batch.version, batch.status)];
        if (batch.source_type === "payroll") {
            const confirmed = input.rows.filter(row => row.status === "confirmed");
            if (confirmed.length && !batch.bank_account_id)
                throw new BusinessRuleError(422, "PAYROLL_BANK_ACCOUNT_REQUIRED", "Select the bank ledger account before confirming payroll payments.");
            const posting = await repository.readConfirmPaymentBatchPosting(batch.source_id, actor.companyId);
            if (!posting)
                throw new BusinessRuleError(409, "PAYROLL_POSTING_MISSING", "A posted payroll accrual is required before bank settlement.");
            const payables = (await repository.readConfirmPaymentBatchPayables(posting.journal_id)).results;
            if (payables.length !== 1)
                throw new BusinessRuleError(409, "PAYROLL_PAYABLE_MAPPING_MISSING", "The posted payroll journal needs one net-payable control account.");
            const bankAccount = confirmed.length ? await repository.readConfirmPaymentBatchBankAccount(batch.bank_account_id, actor.companyId, posting.chart_id, batch.currency) : null;
            if (confirmed.length && !bankAccount)
                throw new BusinessRuleError(422, "BANK_ACCOUNT_INVALID", "The bank ledger account must be active in the payroll chart and currency.");
            if (new Set(confirmed.map(row => row.bankTransactionId)).size !== confirmed.length)
                throw new BusinessRuleError(409, "BANK_EVIDENCE_REUSED", "Each confirmed employee payment needs its own bank transaction.");
            for (const [index, row] of confirmed.entries()) {
                if (!row.bankTransactionId || !row.bankVersion || !row.confirmationRef)
                    throw new BusinessRuleError(422, "BANK_EVIDENCE_REQUIRED", "Provide the exact bank transaction, version and reference for each confirmed payment.");
                const item = items.find(candidate => candidate.reference === row.reference)!;
                const transaction = await repository.readConfirmPaymentBatchTransaction(row.bankTransactionId, actor.companyId);
                if (!transaction || transaction.version !== row.bankVersion || !['unmatched', 'proposed'].includes(transaction.match_status) || transaction.bank_account !== batch.bank_account_id || transaction.currency !== batch.currency || Number(transaction.amount_minor) !== -Number(item.amount_minor) || transaction.external_id !== row.confirmationRef || transaction.transaction_date < posting.pay_date)
                    throw new BusinessRuleError(422, "BANK_EVIDENCE_MISMATCH", "Match the bank account, exact debit, currency, reference, date and transaction version.");
                const periods = (await repository.readConfirmPaymentBatchPeriods(actor.companyId, transaction.transaction_date, transaction.transaction_date)).results;
                if (periods.length !== 1)
                    throw new BusinessRuleError(409, "PAYROLL_SETTLEMENT_PERIOD_REQUIRED", "One open accounting period must contain the bank transaction date.");
                const payrollItem = await repository.readConfirmPaymentBatchPayrollItem(batch.source_id, item.engagement_id, item.amount_minor);
                if (!payrollItem)
                    throw new BusinessRuleError(409, "PAYROLL_PAYMENT_ITEM_CHANGED", "The payable amount no longer matches the posted payroll item.");
                const payableLines = payrollNetCostShares(payrollItem).map(share => ({ accountId: payables[0].account_id, debit: minorToDecimal(share.amountMinor), credit: "0", description: "Net payroll payable settled", dimensions: share.dimensions }));
                if (payableLines.reduce((sum, line) => sum + decimalToMinor(line.debit), 0) !== Number(item.amount_minor))
                    throw new BusinessRuleError(409, "PAYROLL_COST_ALLOCATION_INVALID", "The payable cost-unit allocation does not match the confirmed bank debit.");
                const journal = await ports.prepareJournal(actor, { chartId: posting.chart_id, periodId: periods[0].id, entryDate: transaction.transaction_date, currency: batch.currency, memo: `Payroll payment ${item.reference} · ${row.confirmationRef}`, sourceType: "payroll_payment", sourceId: item.id, isOpening: false, lines: [...payableLines, { accountId: batch.bank_account_id!, debit: "0", credit: minorToDecimal(item.amount_minor), description: "Confirmed payroll bank debit", dimensions: {} }] }, `${commandId}-settlement-${index}`, "posted");
                steps.push(repository.guard216(commandId, `bank-transaction-${index}`, transaction.id, actor.companyId, row.bankVersion, batch.bank_account_id, batch.currency, -item.amount_minor, row.confirmationRef));
                steps.push(...journal.steps, repository.stageConfirmPaymentBatchEffect(`payroll-settlement-${commandId}-${index}`, actor.companyId, batchId, item.id, transaction.id, journal.result.id, actor.userId, now), repository.stageConfirmPaymentBatchEffect2(item.id, transaction.id, row.bankVersion));
            }
        }
        steps.push(...input.rows.map(row => repository.stageConfirmPaymentBatchEffect3(row.status === "confirmed" ? "settled" : "failed", row.confirmationRef ?? null, row.status === "confirmed" ? now : null, batchId, row.reference)), repository.stageConfirmPaymentBatchEffect4(status, now, batchId, actor.companyId));
        return ports.commands.commit(actor, commandId, "people.payment-batch.confirm", input, batchId, batch.version + 1, { id: batchId, status, confirmed: input.rows.filter(r => r.status === "confirmed").length, failed: input.rows.filter(r => r.status === "failed").length }, steps);
    }
    async function importSyntheticPaymentResults(actor: WorkContext, batchId: string, csv: string, commandId: string) {
        const parsed = parseSyntheticPayrollBankResultCsv(csv);
        return confirmPaymentBatch(actor, batchId, paymentConfirmationSchema.parse(parsed), commandId);
    }
    async function getPaymentFile(actor: WorkContext, batchId: string) {
        actor = ports.authorize(actor, "payroll.payment.read");
        await ports.prepare(actor.holdingId);
        const batch = await repository.readGetPaymentFileBatch(batchId, actor.companyId);
        if (!batch?.file_key)
            throw new BusinessRuleError(404, "PAYMENT_FILE_NOT_FOUND", "Payment export is not available.");
        const object = await ports.files.get(batch.file_key);
        if (!object)
            throw new BusinessRuleError(503, "PAYMENT_FILE_UNAVAILABLE", "Payment export is temporarily unavailable.");
        return { object, checksum: batch.file_checksum };
    }
    async function payrollRunsPage(actor:WorkContext,cursor?:string){
        actor=ports.authorize(actor,"payroll.read");
        if(actor.branchId||actor.departmentId)throw new BusinessRuleError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Company-wide payroll requires a holding-level assignment.");
        await ports.prepare(actor.holdingId);
        return ports.pagedRuns(actor.companyId,cursor);
    }
    return { payrollRunsPage,payrollCostAssignments, verifyPayrollCostAssignment, listPeople, getPerson, peopleSummary, payrollSummary, createPerson, createEngagement, createCompensation, payrollOptions, assignPayGroup, transitionCompensation, transitionEngagement, createTimeEntry, updateTimeEntry, rejectTimeEntry, approveTimeEntry, createPayGroup, createCountryPack, listCountryPacks, transitionCountryPack, createPayrollRun, prepareFinalPay, requestPayrollAdvance, decidePayrollAdvance, payrollAdvancePage, payrollAdvanceAlertTarget, calculatePayrollRun, workPayrollRows, getPayrollRun, listPayrollRuns, payrollPostingOptions, savePayrollAccountMapping, approvePayrollAccountMapping, previewPayrollAccounting, postPayroll, approvePayrollRun, requestPayrollCorrection, decidePayrollCorrection, payrollCorrectionAlertTarget, exportPayrollReview, getPayrollReviewFile, createPaymentBatch, preparePaymentRetry, approvePaymentBatch, exportPaymentBatch, confirmPaymentBatch, importSyntheticPaymentResults, getPaymentFile, calculateItem };
}
