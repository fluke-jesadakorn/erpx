import { recordExecutiveEvidenceAccess } from '@erpx/feature-runtime/adapters/server/native-executive-evidence';
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { posExecutiveEvidenceRow } from "@erpx/server-pos/public/pos";
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import { ApiError } from "@erpx/server-runtime/public/http";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { currentAuthority } from "@erpx/server-runtime/public/executive/access";
import { metricCatalogFor } from "@erpx/server-runtime/public/executive/metrics";
import { createPosEvidenceApplication, type PosEvidenceRepository } from '@erpx/server-runtime/application/executive/pos-evidence';
import { executiveEvidenceSources } from '@erpx/server-pos/public/executive-evidence-sources';
import { BusinessRuleError } from '@erpx/server-shared/index';
function application(database: SqlDatabase) {
    const repository: PosEvidenceRepository = { ...executiveEvidenceSources(database), async source0(companyId: string, userId: string, assignmentId: string, providerId: "pos-transaction-records", metric: "pos-sales" | "net-pos-sales" | "pos-payment-mix", action: "search" | "open", value: string, queryHash: string, now: number, value2: string) {
            const rows = JSON.parse(value2) as Array<{ record_id: string; record_version: number | null }>;
            await recordExecutiveEvidenceAccess({
                userId, companyId, assignmentId,
                provider: providerId, metric, action,
                fieldIds: value, queryHash,
                records: rows.map((row) => ({ recordId: row.record_id, recordVersion: Number(row.record_version ?? 0) })),
            });
            return { success: true, meta: { changes: rows.length } };
        } };
    return createPosEvidenceApplication({ repository, clock: { now: Date.now }, digest: sha256, posExecutiveEvidenceRow: (...args) => fromNative(() => posExecutiveEvidenceRow(database, ...args)), assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(database, ...args)), currentAuthority: (...args) => fromNative(() => currentAuthority(database, ...args)), metricCatalogFor: (...args) => fromNative(() => metricCatalogFor(database, ...args)) });
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
export async function searchPosEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) { return invoke(() => application(db).searchPosEvidence(actor, raw)); }
export async function readPosEvidence(db: SqlDatabase, actor: WorkContext, rawMetric: string, recordId: string, rawFilters: unknown) { return invoke(() => application(db).readPosEvidence(actor, rawMetric, recordId, rawFilters)); }
export { posEvidenceMetrics } from '@erpx/server-runtime/domain/executive/pos-evidence';
