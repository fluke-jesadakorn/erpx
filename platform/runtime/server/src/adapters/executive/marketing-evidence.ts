import { recordExecutiveEvidenceAccess } from '@erpx/platform-runtime-ui/adapters/server/native-executive-evidence';
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { marketingExecutiveEvidenceRow } from "@erpx/marketing-server/public/marketing";
import { assertProductionModuleEnabled } from "@erpx/pms-server/public/feature-availability";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { sha256 } from "@erpx/platform-runtime-server/public/crypto";
import { currentAuthority } from "@erpx/platform-runtime-server/public/executive/access";
import { metricCatalogFor } from "@erpx/platform-runtime-server/public/executive/metrics";
import { createMarketingEvidenceApplication, type MarketingEvidenceRepository } from '@erpx/platform-runtime-server/application/executive/marketing-evidence';
import { executiveEvidenceSources } from '@erpx/marketing-server/public/executive-evidence-sources';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
function application(database: SqlDatabase) {
    const repository: MarketingEvidenceRepository = { ...executiveEvidenceSources(database), async source0(companyId: string, userId: string, assignmentId: string, providerId: "marketing-candidate-engagement", metric: "marketing-lead-candidates" | "marketing-engagement-events", action: "search" | "open", value: string, queryHash: string, now: number, value2: string) {
            const rows = JSON.parse(value2) as Array<{ record_id: string; record_version: number | null }>;
            await recordExecutiveEvidenceAccess({
                userId, companyId, assignmentId,
                provider: providerId, metric, action,
                fieldIds: value, queryHash,
                records: rows.map((row) => ({ recordId: row.record_id, recordVersion: Number(row.record_version ?? 0) })),
            });
            return { success: true, meta: { changes: rows.length } };
        } };
    return createMarketingEvidenceApplication({ repository, clock: { now: Date.now }, digest: sha256, marketingExecutiveEvidenceRow: (...args) => fromNative(() => marketingExecutiveEvidenceRow(database, ...args)), assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(database, ...args)), currentAuthority: (...args) => fromNative(() => currentAuthority(database, ...args)), metricCatalogFor: (...args) => fromNative(() => metricCatalogFor(database, ...args)) });
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
export async function searchMarketingEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) { return invoke(() => application(db).searchMarketingEvidence(actor, raw)); }
export async function readMarketingEvidence(db: SqlDatabase, actor: WorkContext, rawMetric: string, recordId: string, rawFilters: unknown) { return invoke(() => application(db).readMarketingEvidence(actor, rawMetric, recordId, rawFilters)); }
export { marketingEvidenceMetrics } from '@erpx/platform-runtime-server/domain/executive/marketing-evidence';
