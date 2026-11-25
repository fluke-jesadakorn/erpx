import { recordExecutiveEvidenceAccess } from '@erpx/feature-runtime/adapters/server/native-executive-evidence';
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import { crmOpportunityEvidenceRow } from "@erpx/server-crm/public/crm";
import { customerFollowupEvidenceRow } from "@erpx/server-sales/public/customer-followups";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { currentAuthority } from "@erpx/server-runtime/public/executive/access";
import { metricCatalogFor } from "@erpx/server-runtime/public/executive/metrics";
import { createCrmEvidenceApplication, type CrmEvidenceRepository } from '@erpx/server-runtime/application/executive/crm-evidence';
import { executiveEvidenceSources } from '@erpx/server-crm/public/executive-evidence-sources';
import {executiveFollowupEvidenceSources} from '@erpx/server-sales/public/executive-followup-evidence-sources';
import { BusinessRuleError } from '@erpx/server-shared/index';
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
export { crmEvidenceMetrics } from '@erpx/server-runtime/domain/executive/crm-evidence';
