import "server-only";
import { type Filters, type MetricId } from "@/lib/shared/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { ApiError } from "@/server/modules/runtime/public/http";
import { currentAuthority } from "@/server/modules/runtime/public/executive/access";
import { metricCatalogFor } from "@/server/modules/runtime/public/executive/metrics";
import { assertProductionModuleEnabled } from "@/server/modules/pms/public/hospitality-rollout";
import { getSupplierBill } from "@/server/modules/procurement/public/supplier-bill-matching";
import { getQualityHold } from "@/server/modules/quality/public/quality";
import { fieldServiceDetail } from "@/server/modules/field_service/public/field-service";
import { productionDetail } from "@/server/modules/manufacturing/public/production";
import { maintenanceDowntimeEvidenceRows } from "@/server/modules/maintenance/public/asset-maintenance";
import { getSupportIncident } from "@/server/modules/support/public/support-incidents";
import { canReadOrganizationChanges } from "@/server/modules/organization/public/organization-work";
import { loadPms } from "@/server/modules/pms/public/pms";
import { getOvertime } from "@/server/modules/people_payroll/public/overtime";
import { readAgentEvidence, searchAgentEvidence } from "@/server/modules/runtime/public/executive/agent-evidence";
import { readCrmEvidence, searchCrmEvidence } from "@/server/modules/runtime/public/executive/crm-evidence";
import { readBrandEvidence, searchBrandEvidence } from "@/server/modules/runtime/public/executive/brand-evidence";
import { readLearningEvidence, searchLearningEvidence } from "@/server/modules/runtime/public/executive/learning-evidence";
import { readTalentEvidence, searchTalentEvidence } from "@/server/modules/runtime/public/executive/talent-evidence";
import { readPosEvidence, searchPosEvidence } from "@/server/modules/runtime/public/executive/pos-evidence";
import { readMarketingEvidence, searchMarketingEvidence } from "@/server/modules/runtime/public/executive/marketing-evidence";
import { readPlanningEvidence, searchPlanningEvidence } from "@/server/modules/runtime/public/executive/planning-evidence";
import { executiveMainEvidenceSources as runtimeSources } from './main-evidence-sources';
import { executiveMainEvidenceSources as legalSources } from '../../../legal/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as financeSources } from '../../../finance/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as people_payrollSources } from '../../../people_payroll/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as pmsSources } from '../../../pms/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as procurementSources } from '../../../procurement/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as commerceSources } from '../../../commerce/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as field_serviceSources } from '../../../field_service/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as qualitySources } from '../../../quality/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as manufacturingSources } from '../../../manufacturing/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as customer_360Sources } from '../../../customer_360/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as collaborationSources } from '../../../collaboration/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as supportSources } from '../../../support/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as customer_authoritySources } from '../../../customer_authority/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as organizationSources } from '../../../organization/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as identity_accessSources } from '../../../identity_access/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as billingSources } from '../../../billing/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as inventorySources } from '../../../inventory/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as projectsSources } from '../../../projects/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as operationsSources } from '../../../operations/public/executive-main-evidence-sources';
import { createExecutiveEvidenceApplication, type ExecutiveEvidenceRepository } from '../../application/executive/evidence';
import { BusinessRuleError } from '../../../shared';
import type { FinanceMetricId } from '../../domain/executive/evidence';
function application(db: SqlDatabase) { const repository: ExecutiveEvidenceRepository = { ...runtimeSources(db), ...legalSources(db), ...financeSources(db), ...people_payrollSources(db), ...pmsSources(db), ...procurementSources(db), ...commerceSources(db), ...field_serviceSources(db), ...qualitySources(db), ...manufacturingSources(db), ...customer_360Sources(db), ...collaborationSources(db), ...supportSources(db), ...customer_authoritySources(db), ...organizationSources(db), ...identity_accessSources(db), ...billingSources(db), ...inventorySources(db), ...projectsSources(db), ...operationsSources(db) }; return createExecutiveEvidenceApplication({ repository, clock: { now: Date.now }, digest: sha256, currentAuthority: (...args) => fromNative(() => currentAuthority(db, ...args)), metricCatalogFor: (...args) => fromNative(() => metricCatalogFor(db, ...args)), assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(db, ...args)), getSupplierBill: (...args) => fromNative(() => getSupplierBill(...args, db)), getQualityHold: (...args) => fromNative(() => getQualityHold(...args, db)), fieldServiceDetail: (...args) => fromNative(() => fieldServiceDetail(...args, db)), productionDetail: (...args) => fromNative(() => productionDetail(...args, db)), getSupportIncident: (...args) => fromNative(() => getSupportIncident(...args, db)), canReadOrganizationChanges: (...args) => fromNative(() => canReadOrganizationChanges(db, ...args)), loadPms: (...args) => fromNative(() => loadPms(db, ...args)), getOvertime: (...args) => fromNative(() => getOvertime(...args, db)), maintenanceDowntimeEvidenceRows: (...args) => fromNative(() => maintenanceDowntimeEvidenceRows(...args, db)), searchAgentEvidence: (...args) => fromNative(() => searchAgentEvidence(db, ...args)), readAgentEvidence: (...args) => fromNative(() => readAgentEvidence(db, ...args)), searchCrmEvidence: (...args) => fromNative(() => searchCrmEvidence(db, ...args)), readCrmEvidence: (...args) => fromNative(() => readCrmEvidence(db, ...args)), searchBrandEvidence: (...args) => fromNative(() => searchBrandEvidence(db, ...args)), readBrandEvidence: (...args) => fromNative(() => readBrandEvidence(db, ...args)), searchLearningEvidence: (...args) => fromNative(() => searchLearningEvidence(db, ...args)), readLearningEvidence: (...args) => fromNative(() => readLearningEvidence(db, ...args)), searchTalentEvidence: (...args) => fromNative(() => searchTalentEvidence(db, ...args)), readTalentEvidence: (...args) => fromNative(() => readTalentEvidence(db, ...args)), searchPosEvidence: (...args) => fromNative(() => searchPosEvidence(db, ...args)), readPosEvidence: (...args) => fromNative(() => readPosEvidence(db, ...args)), searchMarketingEvidence: (...args) => fromNative(() => searchMarketingEvidence(db, ...args)), readMarketingEvidence: (...args) => fromNative(() => readMarketingEvidence(db, ...args)), searchPlanningEvidence: (...args) => fromNative(() => searchPlanningEvidence(db, ...args)), readPlanningEvidence: (...args) => fromNative(() => readPlanningEvidence(db, ...args)) }); }
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
export type { EvidenceSearchInput } from '../../domain/executive/evidence';
