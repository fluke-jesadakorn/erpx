import { recordExecutiveEvidenceAccess } from '@erpx/platform-runtime-ui/adapters/server/native-executive-evidence';
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { assertProductionModuleEnabled } from "@erpx/pms-server/public/feature-availability";
import { crmOpportunityEvidenceRow } from "@erpx/crm-server/public/crm";
import { customerFollowupEvidenceRow } from "@erpx/sales-server/public/customer-followups";
import { sha256 } from "@erpx/platform-runtime-server/public/crypto";
import { currentAuthority } from "@erpx/platform-runtime-server/public/executive/access";
import { metricCatalogFor } from "@erpx/platform-runtime-server/public/executive/metrics";
import { createCrmEvidenceApplication, type CrmEvidenceRepository } from '@erpx/platform-runtime-server/application/executive/crm-evidence';
import { executiveEvidenceSources } from '@erpx/crm-server/public/executive-evidence-sources';
import {executiveFollowupEvidenceSources} from '@erpx/sales-server/public/executive-followup-evidence-sources';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import { planRuntimeControlPlane, RD_UTC_DATE } from '../native-runtime-control-plane';
function application(database: SqlDatabase) {
    const repository: CrmEvidenceRepository = { ...executiveEvidenceSources(database), ...executiveFollowupEvidenceSources(database), async source0(companyId: string, userId: string, assignmentId: string, provider: string, metric: "opportunity-pipeline" | "overdue-followups", action: "search" | "open", value: string, queryHash: string, now: number, value2: string) {
            const rows = JSON.parse(value2) as Array<{ record_id: string; record_version: number | null }>;
            await recordExecutiveEvidenceAccess({
                userId, companyId, assignmentId,
                provider: provider, metric, action,
                fieldIds: value, queryHash,
                records: rows.map((row) => ({ recordId: row.record_id, recordVersion: Number(row.record_version ?? 0) })),
            });
            return { success: true, meta: { changes: rows.length } };
        }, async source1() {
            const plan = await planRuntimeControlPlane(RD_UTC_DATE);
            return database.prepare(plan.sql).bind(...plan.values).first<{
                utc_date: string;
            }>();
        } };
    return createCrmEvidenceApplication({ repository, clock: { now: Date.now }, digest: sha256, assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(database, ...args)), crmOpportunityEvidenceRow: (...args) => fromNative(() => crmOpportunityEvidenceRow(...args, database)), customerFollowupEvidenceRow: (...args) => fromNative(() => customerFollowupEvidenceRow(...args, database)), currentAuthority: (...args) => fromNative(() => currentAuthority(database, ...args)), metricCatalogFor: (...args) => fromNative(() => metricCatalogFor(database, ...args)) });
}
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
export async function searchCrmEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) { return invoke(() => application(db).searchCrmEvidence(actor, raw)); }
export async function readCrmEvidence(db: SqlDatabase, actor: WorkContext, metric: string, recordId: string, rawFilters: unknown) { return invoke(() => application(db).readCrmEvidence(actor, metric, recordId, rawFilters)); }
export { crmEvidenceMetrics } from '@erpx/platform-runtime-server/domain/executive/crm-evidence';
