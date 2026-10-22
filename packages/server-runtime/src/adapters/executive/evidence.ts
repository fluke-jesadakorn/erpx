import {readBenefitEvidence,searchBenefitEvidence} from "@erpx/server-benefits/public/executive-evidence";
import "server-only";
import { type Filters, type MetricId } from "@erpx/feature-shared/lib/executive/canvas";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { ApiError } from "@erpx/server-runtime/public/http";
import { currentAuthority } from "@erpx/server-runtime/public/executive/access";
import { metricCatalogFor } from "@erpx/server-runtime/public/executive/metrics";
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import { getSupplierBill } from "@erpx/server-procurement/public/supplier-bill-matching";
import { getQualityHold } from "@erpx/server-quality/public/quality";
import {serviceEvidenceDetail} from "@erpx/server-field_service/public/record-purchases";
import { productionDetail } from "@erpx/server-manufacturing/public/production";
import { maintenanceDowntimeEvidenceRows } from "@erpx/server-maintenance/public/asset-maintenance";
import { getSupportIncident } from "@erpx/server-support/public/support-incidents";
import { canReadOrganizationChanges } from "@erpx/server-organization/public/organization-work";
import { loadPms } from "../../../../../packages/server-pms/src/public/pms";
import { getOvertime } from "@erpx/server-people_payroll/public/overtime";
import { readAgentEvidence, searchAgentEvidence } from "@erpx/server-runtime/public/executive/agent-evidence";
import { readCrmEvidence, searchCrmEvidence } from "@erpx/server-runtime/public/executive/crm-evidence";
import { readBrandEvidence, searchBrandEvidence } from "@erpx/server-runtime/public/executive/brand-evidence";
import { readLearningEvidence, searchLearningEvidence } from "@erpx/server-runtime/public/executive/learning-evidence";
import { readTalentEvidence, searchTalentEvidence } from "@erpx/server-runtime/public/executive/talent-evidence";
import { readPosEvidence, searchPosEvidence } from "@erpx/server-runtime/public/executive/pos-evidence";
import { readMarketingEvidence, searchMarketingEvidence } from "@erpx/server-runtime/public/executive/marketing-evidence";
import { readPlanningEvidence, searchPlanningEvidence } from "@erpx/server-runtime/public/executive/planning-evidence";
import { executiveMainEvidenceSources as runtimeSources } from '@erpx/server-runtime/adapters/executive/main-evidence-sources';
import { executiveMainEvidenceSources as legalSources } from '@erpx/server-legal/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as financeSources } from '@erpx/server-finance/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as people_payrollSources } from '@erpx/server-people_payroll/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as pmsSources } from '@erpx/server-pms/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as procurementSources } from '@erpx/server-procurement/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as commerceSources } from '@erpx/server-commerce/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as field_serviceSources } from '@erpx/server-field_service/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as qualitySources } from '@erpx/server-quality/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as manufacturingSources } from '@erpx/server-manufacturing/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as customer_360Sources } from '@erpx/server-customer_360/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as collaborationSources } from '@erpx/server-collaboration/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as supportSources } from '@erpx/server-support/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as customer_authoritySources } from '@erpx/server-customer_authority/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as organizationSources } from '@erpx/server-organization/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as identity_accessSources } from '@erpx/server-identity_access/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as billingSources } from '@erpx/server-billing/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as inventorySources } from '@erpx/server-inventory/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as projectsSources } from '@erpx/server-projects/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as operationsSources } from '@erpx/server-operations/public/executive-main-evidence-sources';
import { createExecutiveEvidenceApplication, type ExecutiveEvidenceRepository } from '@erpx/server-runtime/application/executive/evidence';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type { FinanceMetricId } from '@erpx/server-runtime/domain/executive/evidence';
function application(db: SqlDatabase) { const repository: ExecutiveEvidenceRepository = { ...runtimeSources(db), ...legalSources(db), ...financeSources(db), ...people_payrollSources(db), ...pmsSources(db), ...procurementSources(db), ...commerceSources(db), ...field_serviceSources(db), ...qualitySources(db), ...manufacturingSources(db), ...customer_360Sources(db), ...collaborationSources(db), ...supportSources(db), ...customer_authoritySources(db), ...organizationSources(db), ...identity_accessSources(db), ...billingSources(db), ...inventorySources(db), ...projectsSources(db), ...operationsSources(db) }; return createExecutiveEvidenceApplication({ repository, clock: { now: Date.now }, digest: sha256, currentAuthority: (...args) => fromNative(() => currentAuthority(db, ...args)), metricCatalogFor: (...args) => fromNative(() => metricCatalogFor(db, ...args)), assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(db, ...args)), getSupplierBill: (...args) => fromNative(() => getSupplierBill(...args, db)), getQualityHold: (...args) => fromNative(() => getQualityHold(...args, db)), fieldServiceDetail: (actor,id) => fromNative(() => serviceEvidenceDetail(db,actor,id)), productionDetail: (...args) => fromNative(() => productionDetail(...args, db)), getSupportIncident: (...args) => fromNative(() => getSupportIncident(...args, db)), canReadOrganizationChanges: (...args) => fromNative(() => canReadOrganizationChanges(db, ...args)), loadPms: (...args) => fromNative(() => loadPms(db, ...args)), getOvertime: (...args) => fromNative(() => getOvertime(...args, db)), maintenanceDowntimeEvidenceRows: (...args) => fromNative(() => maintenanceDowntimeEvidenceRows(...args, db)), searchAgentEvidence: (...args) => fromNative(() => searchAgentEvidence(db, ...args)), readAgentEvidence: (...args) => fromNative(() => readAgentEvidence(db, ...args)), searchCrmEvidence: (...args) => fromNative(() => searchCrmEvidence(db, ...args)), readCrmEvidence: (...args) => fromNative(() => readCrmEvidence(db, ...args)), searchBrandEvidence: (...args) => fromNative(() => searchBrandEvidence(db, ...args)), readBrandEvidence: (...args) => fromNative(() => readBrandEvidence(db, ...args)), searchLearningEvidence: (...args) => fromNative(() => searchLearningEvidence(db, ...args)), readLearningEvidence: (...args) => fromNative(() => readLearningEvidence(db, ...args)), searchTalentEvidence: (...args) => fromNative(() => searchTalentEvidence(db, ...args)), readTalentEvidence: (...args) => fromNative(() => readTalentEvidence(db, ...args)), searchPosEvidence: (...args) => fromNative(() => searchPosEvidence(db, ...args)), readPosEvidence: (...args) => fromNative(() => readPosEvidence(db, ...args)), searchMarketingEvidence: (...args) => fromNative(() => searchMarketingEvidence(db, ...args)), readMarketingEvidence: (...args) => fromNative(() => readMarketingEvidence(db, ...args)), searchBenefitEvidence: (...args) => fromNative(() => searchBenefitEvidence(db, ...args)), readBenefitEvidence: (...args) => fromNative(() => readBenefitEvidence(db, ...args)), searchPlanningEvidence: (...args) => fromNative(() => searchPlanningEvidence(db, ...args)), readPlanningEvidence: (...args) => fromNative(() => readPlanningEvidence(db, ...args)) }); }
async function fromNative<T>(run: () => Promise<T> | T): Promise<T> { try {
    return await run();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function invoke<T>(run: () => Promise<T> | T): Promise<T> { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
export async function searchFinanceEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) { return invoke(() => application(db).searchFinanceEvidence(actor, raw)); }
export async function readFinanceEvidence(db: SqlDatabase, actor: WorkContext, metric: FinanceMetricId, recordId: string, filters: Filters) { return invoke(() => application(db).readFinanceEvidence(actor, metric, recordId, filters)); }
export async function searchExecutiveEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) { return invoke(() => application(db).searchExecutiveEvidence(actor, raw)); }
export async function readExecutiveEvidence(db: SqlDatabase, actor: WorkContext, metric: MetricId, recordId: string, filters: Filters) { return invoke(() => application(db).readExecutiveEvidence(actor, metric, recordId, filters)); }
export type { EvidenceSearchInput } from '@erpx/server-runtime/domain/executive/evidence';
