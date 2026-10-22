import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { brandProfileEvidenceRow } from "@erpx/server-brand/public/branding";
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import { companyAuthority } from "@erpx/server-operations/public/staff-policy";
import { ApiError } from "@erpx/server-runtime/public/http";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { currentAuthority } from "@erpx/server-runtime/public/executive/access";
import { metricCatalogFor } from "@erpx/server-runtime/public/executive/metrics";
import { createBrandEvidenceApplication, type BrandEvidenceRepository } from '@erpx/server-runtime/application/executive/brand-evidence';
import { executiveEvidenceSources } from '@erpx/server-brand/public/executive-evidence-sources';
import { BusinessRuleError } from '@erpx/server-shared/index';
function application(database: SqlDatabase) {
    const repository: BrandEvidenceRepository = { ...executiveEvidenceSources(database), source0(companyId: string, userId: string, assignmentId: string, providerId: "brand-profile-records", metric: "brand-profiles" | "brand-profile-updates", action: "search" | "open", value: string, queryHash: string, now: number, value2: string) {
            return database.prepare(`INSERT INTO executive_evidence_access_events
    (id,company_id,actor_id,assignment_id,provider_id,metric_id,action,record_id,record_version,field_ids,query_hash,created_at)
    SELECT 'executive-evidence-'||gen_random_uuid()::text,?,?,?,?,?,?,item.record_id,item.record_version,?::jsonb,?,?
    FROM jsonb_to_recordset(?::jsonb) AS item(record_id text,record_version bigint)`)
                .bind(companyId, userId, assignmentId, providerId, metric, action, value, queryHash, now, value2)
                .run();
        } };
    return createBrandEvidenceApplication({ repository, clock: { now: Date.now }, digest: sha256, brandProfileEvidenceRow: (...args) => fromNative(() => brandProfileEvidenceRow(database, ...args)), assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(database, ...args)), companyAuthority: (...args) => fromNative(() => companyAuthority(database, ...args)), currentAuthority: (...args) => fromNative(() => currentAuthority(database, ...args)), metricCatalogFor: (...args) => fromNative(() => metricCatalogFor(database, ...args)) });
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
export async function searchBrandEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) { return invoke(() => application(db).searchBrandEvidence(actor, raw)); }
export async function readBrandEvidence(db: SqlDatabase, actor: WorkContext, metric: string, recordId: string, rawFilters: unknown) { return invoke(() => application(db).readBrandEvidence(actor, metric, recordId, rawFilters)); }
export { brandEvidenceMetrics } from '@erpx/server-runtime/domain/executive/brand-evidence';
