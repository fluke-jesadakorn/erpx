import type {SqlStatement} from '@erpx/server-runtime/public/runtime/types';
import { RecordDetailsPeoplePayrollOwnedFacts } from '@erpx/server-people_payroll/public/records-record-details-owned-facts';
import { RecordDetailsPosOwnedFacts } from '@erpx/server-pos/public/records-record-details-owned-facts';
import { RecordDetailsInventoryOwnedFacts } from '@erpx/server-inventory/public/records-record-details-owned-facts';
import { RecordDetailsWorkflowOwnedFacts } from '@erpx/server-workflow/public/records-record-details-owned-facts';
import { RecordDetailsFinanceOwnedFacts } from '@erpx/server-finance/public/records-record-details-owned-facts';
import { RecordDetailsCommerceOwnedFacts } from '@erpx/server-commerce/public/records-record-details-owned-facts';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { agentEvidenceRecord } from "@erpx/server-agent/public/agent-governance";
import { isFinanceExpansionKind, financeExpansionRecord } from "@erpx/server-finance/public/finance-expansion-records";
import { groupExpansionRecord, groupEditionSourceJournal, groupEditionAccountSources } from "@erpx/server-finance/public/finance-group-editions";
import { financeCompanyTools } from "@erpx/server-finance/public/finance-company-tools";
import { financeExtraRecord, isFinanceExtraKind } from "@erpx/server-finance/public/finance-record-details";
import { pmsRefundRecord, pmsSnapshotEntry, pmsControlSources } from "@erpx/server-pms/public/pms/record-details";
import { queryMetric } from "@erpx/server-runtime/public/executive/metrics";
import "server-only";
import { isDomainRecordKind, recordDomainData } from "@erpx/server-records/public/record-domain-details";
import { commerceOverview, commercePermissions } from "@erpx/server-commerce/public/commerce";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { bankWorkspace, bankStatementDetail, listCharts, listAccounts, aging, reportDateSchema, trialBalance, statements } from "@erpx/server-finance/public/finance";
import { marketingWorkspace } from "@erpx/server-marketing/public/marketing";
import { payrollSummary } from "@erpx/server-people_payroll/public/people-payroll";
import { calculationRunDetail } from "@erpx/server-finance/public/finance-engines";
import { accrualRecordDetail } from "@erpx/server-finance/public/finance-accruals";
import { intercompanyDetail } from "@erpx/server-finance/public/finance-intercompany";
import { consolidationDetail, consolidationEliminationDetail } from "@erpx/server-finance/public/finance-consolidation";
import { pricingWorkspace } from "@erpx/server-sales/public/sales-pricing";
import { listInventoryReorderPolicies } from "@erpx/server-inventory/public/inventory-reorder-policies";
import type { RecordTarget } from "@erpx/feature-records/lib/record-drawer";
import { posReportReceipts } from "@erpx/server-pos/public/pos";
import { statementSourceAccounts } from "@erpx/server-finance/public/finance-reporting";
import { financePlanRecord, budgetSourceRecords, financialReportSourceRecords } from "@erpx/server-finance/public/finance-planning";
import { financeBookRecord } from "@erpx/server-finance/public/finance-books";
import { holdingResources as nativeHoldingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { policyRead as nativePolicyRead, policyNow as nativePolicyNow } from '@erpx/server-identity_access/public/iam-read-snapshot';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '@erpx/server-workflow/public/commands';
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { drilldownPage as puredrilldownPage } from '@erpx/server-records/domain/record-details';
import { createRecordDetailsApplication, type RecordDetailsRepository } from '@erpx/server-records/application/record-details';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { RecordDetailsOrganizationFacts } from '@erpx/server-organization/public/records-record-details-facts';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: RecordDetailsRepository = { readCommerceMasterDetailRow(...args) { return RecordDetailsCommerceOwnedFacts(database).readCommerceMasterDetailRow(...args); },
        readCommerceMasterDetailRow2(...args) { return RecordDetailsFinanceOwnedFacts(database).readCommerceMasterDetailRow2(...args); },
        readCommerceMasterDetailRow3(...args) { return RecordDetailsOrganizationFacts(database).readCommerceMasterDetailRow3(...args); },
        readCommerceMasterDetailHistory(...args) { return RecordDetailsWorkflowOwnedFacts(database).readCommerceMasterDetailHistory(...args); },
        readRecordSupplementDataRow(...args) { return RecordDetailsInventoryOwnedFacts(database).readRecordSupplementDataRow(...args); },
        readRecordSupplementDataSales(...args) { return RecordDetailsPosOwnedFacts(database).readRecordSupplementDataSales(...args); },
        readRecordSupplementDataRow2(...args) { return RecordDetailsInventoryOwnedFacts(database).readRecordSupplementDataRow2(...args); },
        readRecordSupplementDataRows(...args) { return RecordDetailsInventoryOwnedFacts(database).readRecordSupplementDataRows(...args); },
        readRecordSupplementDataRow3(...args) { return RecordDetailsInventoryOwnedFacts(database).readRecordSupplementDataRow3(...args); },
        readRecordSupplementDataRows2(...args) { return RecordDetailsInventoryOwnedFacts(database).readRecordSupplementDataRows2(...args); },
        readRecordSupplementDataAccount(...args) { return RecordDetailsFinanceOwnedFacts(database).readRecordSupplementDataAccount(...args); },
        readRecordSupplementDataPeriod(...args) { return RecordDetailsFinanceOwnedFacts(database).readRecordSupplementDataPeriod(...args); },
        readRecordSupplementDataRows3(...args) { return RecordDetailsFinanceOwnedFacts(database).readRecordSupplementDataRows3(...args); },
        readRecordSupplementDataRows4(...args) { return RecordDetailsPeoplePayrollOwnedFacts(database).readRecordSupplementDataRows4(...args); } };
    return createRecordDetailsApplication<SqlStatement>({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, production: process.env.NODE_ENV === 'production', batch: effects => database.batch(effects), policyRead: (key, read) => nativePolicyRead(database, key, read), policyNow: nativePolicyNow, reportDates: value => reportDateSchema.parse(value), agentEvidenceRecord: (...args) => fromNative(() => agentEvidenceRecord(...args)), isFinanceExpansionKind: isFinanceExpansionKind, financeExpansionRecord: (...args) => fromNative(() => financeExpansionRecord(...args)), groupExpansionRecord: (...args) => fromNative(() => groupExpansionRecord(...args)), groupEditionSourceJournal: (...args) => fromNative(() => groupEditionSourceJournal(...args)), groupEditionAccountSources: (...args) => fromNative(() => groupEditionAccountSources(...args)), financeCompanyTools: (...args) => fromNative(() => financeCompanyTools(...args)), financeExtraRecord: (...args) => fromNative(() => financeExtraRecord(...args)), isFinanceExtraKind: isFinanceExtraKind, pmsRefundRecord: (...args) => fromNative(() => pmsRefundRecord(...args)), pmsSnapshotEntry: (...args) => fromNative(() => pmsSnapshotEntry(...args)), pmsControlSources: (...args) => fromNative(() => pmsControlSources(...args)), queryMetric: (...args) => fromNative(() => queryMetric(database, ...args)), isDomainRecordKind: isDomainRecordKind, recordDomainData: (...args) => fromNative(() => recordDomainData(...args)), commerceOverview: (...args) => fromNative(() => commerceOverview(...args)), commercePermissions: (...args) => commercePermissions(...args), requirePermission: (actor, permission) => fromNativeSync(() => requirePermission(actor, permission)), bankWorkspace: (...args) => fromNative(() => bankWorkspace(...args)), bankStatementDetail: (...args) => fromNative(() => bankStatementDetail(...args)), listCharts: (...args) => fromNative(() => listCharts(...args)), listAccounts: (...args) => fromNative(() => listAccounts(...args)), aging: (...args) => fromNative(() => aging(...args)), trialBalance: (...args) => fromNative(() => trialBalance(...args)), statements: (...args) => fromNative(() => statements(...args)), marketingWorkspace: (...args) => fromNative(() => marketingWorkspace(...args)), payrollSummary: (...args) => fromNative(() => payrollSummary(...args)), calculationRunDetail: (...args) => fromNative(() => calculationRunDetail(...args)), accrualRecordDetail: (...args) => fromNative(() => accrualRecordDetail(...args)), intercompanyDetail: (...args) => fromNative(() => intercompanyDetail(...args)), consolidationDetail: (...args) => fromNative(() => consolidationDetail(...args)), consolidationEliminationDetail: (...args) => fromNative(() => consolidationEliminationDetail(...args)), pricingWorkspace: (...args) => fromNative(() => pricingWorkspace(...args)), listInventoryReorderPolicies: (...args) => fromNative(() => listInventoryReorderPolicies(...args)), posReportReceipts: (...args) => fromNative(() => posReportReceipts(...args)), statementSourceAccounts: (...args) => fromNative(() => statementSourceAccounts(...args)), financePlanRecord: (...args) => fromNative(() => financePlanRecord(...args)), budgetSourceRecords: (...args) => fromNative(() => budgetSourceRecords(...args)), financialReportSourceRecords: (...args) => fromNative(() => financialReportSourceRecords(...args)), financeBookRecord: (...args) => fromNative(() => financeBookRecord(...args)) });
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
export async function commerceMasterDetail(actor: WorkContext, kind: "product" | "party" | "warehouse", id: string) { return invoke(() => application().commerceMasterDetail(actor, kind, id)); }
export function drilldownPage(value?: string) { return invokeSync(() => puredrilldownPage(value)); }
export async function recordSupplementData(actor: WorkContext, target: RecordTarget, args: Record<string, string>) { return invoke(() => application().recordSupplementData(actor, target, args)); }
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
