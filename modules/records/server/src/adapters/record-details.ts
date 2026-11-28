import type {SqlStatement} from '@erpx/platform-runtime-server/public/runtime/types';
import { RecordDetailsPeoplePayrollOwnedFacts } from '@erpx/people-payroll-server/public/records-record-details-owned-facts';
import { RecordDetailsPosOwnedFacts } from '@erpx/pos-server/public/records-record-details-owned-facts';
import { RecordDetailsInventoryOwnedFacts } from '@erpx/inventory-server/public/records-record-details-owned-facts';
import { RecordDetailsWorkflowOwnedFacts } from '@erpx/workflow-server/public/records-record-details-owned-facts';
import { RecordDetailsFinanceOwnedFacts } from '@erpx/finance-server/public/records-record-details-owned-facts';
import { RecordDetailsCommerceOwnedFacts } from '@erpx/commerce-server/public/records-record-details-owned-facts';
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { agentEvidenceRecord } from "@erpx/agent-server/public/agent-governance";
import { isFinanceExpansionKind, financeExpansionRecord } from "@erpx/finance-server/public/finance-expansion-records";
import { groupExpansionRecord, groupEditionSourceJournal, groupEditionAccountSources } from "@erpx/finance-server/public/finance-group-editions";
import { financeCompanyTools } from "@erpx/finance-server/public/finance-company-tools";
import { financeExtraRecord, isFinanceExtraKind } from "@erpx/finance-server/public/finance-record-details";
import { pmsRefundRecord, pmsSnapshotEntry, pmsControlSources } from "@erpx/pms-server/public/pms/record-details";
import { queryMetric } from "@erpx/platform-runtime-server/public/executive/metrics";
import "server-only";
import { isDomainRecordKind, recordDomainData } from "@erpx/records-server/public/record-domain-details";
import {commerceOverview,commercePermissions} from '@erpx/commerce-server/public/commerce';
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import {bankWorkspace,bankStatementDetail,financeChartListNative,listAccounts,aging,reportDateSchema,trialBalance,statements} from '@erpx/finance-server/public/finance';
import { marketingWorkspace } from "@erpx/marketing-server/public/marketing";
import { ownedPeoplePayrollSummary } from "@erpx/people-payroll-ui/public/native-payroll-summary-owned";
import { calculationRunDetail } from "@erpx/finance-server/public/finance-engines";
import { accrualRecordDetail } from "@erpx/finance-server/public/finance-accruals";
import { intercompanyDetail } from "@erpx/finance-server/public/finance-intercompany";
import { consolidationDetail, consolidationEliminationDetail } from "@erpx/finance-server/public/finance-consolidation";
import { pricingWorkspace } from "@erpx/sales-server/public/sales-pricing";
import { listInventoryReorderPolicies } from "@erpx/inventory-server/public/inventory-reorder-policies";
import type { RecordTarget } from "@erpx/records-ui/lib/record-drawer";
import { posReportReceipts } from "@erpx/pos-server/public/pos";
import { statementSourceAccounts } from "@erpx/finance-server/public/finance-reporting";
import { financePlanRecord, budgetSourceRecords, financialReportSourceRecords } from "@erpx/finance-server/public/finance-planning";
import { financeBookRecord } from "@erpx/finance-server/public/finance-books";
import { holdingResources as nativeHoldingResources } from '../../../../../platform/runtime/server/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/identity-access-server/public/iam-read-snapshot';

import { ApiError as NativeApiError } from '@erpx/platform-runtime-server/public/http';
import { createNativeRecordDetailsClient, nativeRecordDetailsConfiguration, nativeRecordDetailsFailure } from '../public/native-record-context';
import { drilldownPage as puredrilldownPage } from '@erpx/records-server/domain/record-details';
import { createRecordDetailsApplication, type RecordDetailsRepository } from '@erpx/records-server/application/record-details';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import { RecordDetailsOrganizationFacts } from '@erpx/organization-server/public/records-record-details-facts';
function application(provided?: SqlDatabase, context?: { holdingId: string; holdingSlug: string; companyId?: string }) {
    let database = provided!;
    const repository: RecordDetailsRepository = { readCommerceMasterDetailRow(...args) { return RecordDetailsCommerceOwnedFacts(database).readCommerceMasterDetailRow(...args); },
        readCommerceMasterDetailRow2(...args) { return RecordDetailsFinanceOwnedFacts(database, context).readCommerceMasterDetailRow2(...args); },
        readCommerceMasterDetailRow3(...args) { return RecordDetailsOrganizationFacts(database).readCommerceMasterDetailRow3(...args); },
        readCommerceMasterDetailHistory(...args) { return RecordDetailsWorkflowOwnedFacts(database).readCommerceMasterDetailHistory(...args); },
        readRecordSupplementDataRow(...args) { return RecordDetailsInventoryOwnedFacts(database).readRecordSupplementDataRow(...args); },
        readRecordSupplementDataSales(...args) { return RecordDetailsPosOwnedFacts(database).readRecordSupplementDataSales(...args); },
        readRecordSupplementDataRow2(...args) { return RecordDetailsInventoryOwnedFacts(database).readRecordSupplementDataRow2(...args); },
        readRecordSupplementDataRows(...args) { return RecordDetailsInventoryOwnedFacts(database).readRecordSupplementDataRows(...args); },
        readRecordSupplementDataRow3(...args) { return RecordDetailsInventoryOwnedFacts(database).readRecordSupplementDataRow3(...args); },
        readRecordSupplementDataRows2(...args) { return RecordDetailsInventoryOwnedFacts(database).readRecordSupplementDataRows2(...args); },
        readRecordSupplementDataAccount(...args) { return RecordDetailsFinanceOwnedFacts(database, context).readRecordSupplementDataAccount(...args); },
        readRecordSupplementDataPeriod(...args) { return RecordDetailsFinanceOwnedFacts(database, context).readRecordSupplementDataPeriod(...args); },
        readRecordSupplementDataRows3(...args) { return RecordDetailsFinanceOwnedFacts(database, context).readRecordSupplementDataRows3(...args); },
        readRecordSupplementDataRows4(...args) { return RecordDetailsPeoplePayrollOwnedFacts(database).readRecordSupplementDataRows4(...args); } };
    return createRecordDetailsApplication<SqlStatement>({repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, reportDates: value => reportDateSchema.parse(value), agentEvidenceRecord: (...args) => fromNative(() => agentEvidenceRecord(...args)), isFinanceExpansionKind: isFinanceExpansionKind, financeExpansionRecord: (...args) => fromNative(() => financeExpansionRecord(...args)), groupExpansionRecord: (...args) => fromNative(() => groupExpansionRecord(...args)), groupEditionSourceJournal: (...args) => fromNative(() => groupEditionSourceJournal(...args)), groupEditionAccountSources: (...args) => fromNative(() => groupEditionAccountSources(...args)), financeCompanyTools: (...args) => fromNative(() => financeCompanyTools(...args)), financeExtraRecord: (...args) => fromNative(() => financeExtraRecord(...args)), isFinanceExtraKind: isFinanceExtraKind, pmsRefundRecord: (...args) => fromNative(() => pmsRefundRecord(...args)), pmsSnapshotEntry: (...args) => fromNative(() => pmsSnapshotEntry(...args)), pmsControlSources: (...args) => fromNative(() => pmsControlSources(...args)), queryMetric: (...args) => fromNative(() => queryMetric(database, ...args)), isDomainRecordKind: isDomainRecordKind, recordDomainData: (...args) => fromNative(() => recordDomainData(...args)), commerceOverview: (...args) => fromNative(() => commerceOverview(...args)), commercePermissions: (...args) => commercePermissions(...args), requirePermission: (actor, permission) => fromNativeSync(() => requirePermission(actor, permission)), bankWorkspace: (...args) => fromNative(() => bankWorkspace(...args)), bankStatementDetail: (...args) => fromNative(() => bankStatementDetail(...args)), financeChartListNative: (...args) => fromNative(() => financeChartListNative(...args)), listAccounts: (...args) => fromNative(() => listAccounts(...args)), aging: (...args) => fromNative(() => aging(...args)), trialBalance: (...args) => fromNative(() => trialBalance(...args)), statements: (...args) => fromNative(() => statements(...args)), marketingWorkspace: (...args) => fromNative(() => marketingWorkspace(...args)), payrollSummary: (actor, payMonth) => fromNative(async () => {
            const rows = await ownedPeoplePayrollSummary(actor, actor.holdingSlug, payMonth);
            if (rows === undefined)
                throw new NativeApiError(503, "NATIVE_PEOPLE_PAYROLL_UNAVAILABLE", "The native People Payroll reader is unavailable.");
            return rows;
        }), calculationRunDetail: (...args) => fromNative(() => calculationRunDetail(...args)), accrualRecordDetail: (...args) => fromNative(() => accrualRecordDetail(...args)), intercompanyDetail: (...args) => fromNative(() => intercompanyDetail(...args)), consolidationDetail: (...args) => fromNative(() => consolidationDetail(...args)), consolidationEliminationDetail: (...args) => fromNative(() => consolidationEliminationDetail(...args)), pricingWorkspace: (...args) => fromNative(() => pricingWorkspace(...args)), listInventoryReorderPolicies: (...args) => fromNative(() => listInventoryReorderPolicies(...args)), posReportReceipts: (...args) => fromNative(() => posReportReceipts(...args)), statementSourceAccounts: (...args) => fromNative(() => statementSourceAccounts(...args)), financePlanRecord: (...args) => fromNative(() => financePlanRecord(...args)), budgetSourceRecords: (...args) => fromNative(() => budgetSourceRecords(...args)), financialReportSourceRecords: (...args) => fromNative(() => financialReportSourceRecords(...args)), financeBookRecord: (...args) => fromNative(() => financeBookRecord(...args))});
}
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof NativeApiError)
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
            throw new NativeApiError(error.status, error.code, error.message);
        throw error;
    }
}
function invokeSync<Result>(run: () => Result) {
    try {
        return run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new NativeApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function commerceMasterDetail(actor: WorkContext, kind: "product" | "party" | "warehouse", id: string) { return invoke(() => application(undefined, { holdingId: actor.holdingId, holdingSlug: actor.holdingSlug, companyId: actor.companyId }).commerceMasterDetail(actor, kind, id)); }
export function drilldownPage(value?: string) { return invokeSync(() => puredrilldownPage(value)); }
export async function recordSupplementData(actor: WorkContext, target: RecordTarget, args: Record<string, string>) {
    if (target.kind === 'workspace_record')
        return supplementWorkspaceRecord(actor, target, args);
    return invoke(() => application(undefined, { holdingId: actor.holdingId, holdingSlug: actor.holdingSlug, companyId: actor.companyId }).recordSupplementData(actor, target, args));
}
/** Native detail for a Records-owned workspace record. Never falls back to TS SQL. */
async function supplementWorkspaceRecord(actor: WorkContext, target: RecordTarget, args: Record<string, string>) {
    if (!actor.authority || !actor.iam)
        throw new NativeApiError(409, "WORK_CONTEXT_CHANGED", "Your work role changed. Reload and review before retrying.");
    const moduleId = args.module;
    if (!moduleId)
        throw new NativeApiError(422, "RECORD_TARGET_REQUIRED", "Choose a valid source record.");
    const raw = process.env.ERPX_NATIVE_RECORDS_CONFIG;
    if (raw === undefined)
        throw new NativeApiError(503, "NATIVE_RECORD_DETAILS_UNAVAILABLE", "The native Records detail reader is unavailable.");
    const scope = actor.scope;
    const page = args.detailPage === undefined ? 1 : /^\d+$/.test(args.detailPage) ? Number(args.detailPage) : Number.NaN;
    try {
        const client = createNativeRecordDetailsClient(nativeRecordDetailsConfiguration(raw));
        const { headers: requestHeaders } = await import('next/headers');
        return await client.supplement(await requestHeaders(), {
            userId: actor.userId,
            holdingId: actor.holdingId,
            holdingSlug: actor.holdingSlug,
            companyId: actor.companyId,
            branchId: actor.branchId,
            departmentId: actor.departmentId,
            branchSlug: scope.kind === "branch" || scope.kind === "department" ? (scope.branchSlug ?? null) : null,
            departmentSlug: scope.kind === "department" ? scope.departmentSlug : null,
            assignmentId: actor.assignmentId,
            role: actor.role,
            assignmentScope: actor.assignmentScope,
            modules: actor.modules,
            authority: actor.authority,
            iam: { permissions: actor.iam.permissions },
        }, { holdingSlug: actor.holdingSlug, moduleId, recordId: target.id, page });
    }
    catch (error) {
        const mapped = nativeRecordDetailsFailure(error);
        if (!mapped)
            throw error;
        throw new NativeApiError(mapped.status, mapped.code, mapped.message);
    }
}
function fromNativeSync<Result>(run: () => Result) {
    try {
        return run();
    }
    catch (error) {
        if (error instanceof NativeApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
