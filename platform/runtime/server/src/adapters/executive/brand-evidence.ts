import { recordExecutiveEvidenceAccess } from '@erpx/platform-runtime-ui/adapters/server/native-executive-evidence';
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { readBrandProfileEvidence } from "@erpx/brand-server/public/branding";
import { assertProductionModuleEnabled } from "@erpx/pms-server/public/feature-availability";
import { companyAuthority } from "@erpx/operations-server/public/staff-policy";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { sha256 } from "@erpx/platform-runtime-server/public/crypto";
import { currentAuthority } from "@erpx/platform-runtime-server/public/executive/access";
import { metricCatalogFor } from "@erpx/platform-runtime-server/public/executive/metrics";
import { createBrandEvidenceApplication, type BrandEvidenceRepository } from '@erpx/platform-runtime-server/application/executive/brand-evidence';
import { executiveEvidenceSources } from '@erpx/brand-server/public/executive-evidence-sources';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
function application(database: SqlDatabase) {
    const repository: BrandEvidenceRepository = { ...executiveEvidenceSources(database), async source0(companyId: string, userId: string, assignmentId: string, providerId: "brand-profile-records", metric: "brand-profiles" | "brand-profile-updates", action: "search" | "open", value: string, queryHash: string, now: number, value2: string) {
            const rows = JSON.parse(value2) as Array<{ record_id: string; record_version: number | null }>;
            await recordExecutiveEvidenceAccess({
                userId, companyId, assignmentId,
                provider: providerId, metric, action,
                fieldIds: value, queryHash,
                records: rows.map((row) => ({ recordId: row.record_id, recordVersion: Number(row.record_version ?? 0) })),
            });
            return { success: true, meta: { changes: rows.length } };
        } };
    return createBrandEvidenceApplication({ repository, clock: { now: Date.now }, digest: sha256, brandProfileEvidenceRow: (...args) => fromNative(() => readBrandProfileEvidence(...args)), assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(database, ...args)), companyAuthority: (...args) => fromNative(() => companyAuthority(database, ...args)), currentAuthority: (...args) => fromNative(() => currentAuthority(database, ...args)), metricCatalogFor: (...args) => fromNative(() => metricCatalogFor(database, ...args)) });
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
export { brandEvidenceMetrics } from '@erpx/platform-runtime-server/domain/executive/brand-evidence';
