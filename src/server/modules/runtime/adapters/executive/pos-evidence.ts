import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { posExecutiveEvidenceRow } from "@/server/modules/pos/public/pos";
import { assertProductionModuleEnabled } from "@/server/modules/pms/public/hospitality-rollout";
import { ApiError } from "@/server/modules/runtime/public/http";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { currentAuthority } from "@/server/modules/runtime/public/executive/access";
import { metricCatalogFor } from "@/server/modules/runtime/public/executive/metrics";
import { createPosEvidenceApplication, type PosEvidenceRepository } from '../../application/executive/pos-evidence';
import { executiveEvidenceSources } from '../../../pos/public/executive-evidence-sources';
import { BusinessRuleError } from '../../../shared';
function application(database: SqlDatabase) {
    const repository: PosEvidenceRepository = { ...executiveEvidenceSources(database), source0(companyId: string, userId: string, assignmentId: string, providerId: "pos-transaction-records", metric: "pos-sales" | "net-pos-sales" | "pos-payment-mix", action: "search" | "open", value: string, queryHash: string, now: number, value2: string) {
            return database.prepare(`INSERT INTO executive_evidence_access_events
    (id,company_id,actor_id,assignment_id,provider_id,metric_id,action,record_id,record_version,field_ids,query_hash,created_at)
    SELECT 'executive-evidence-'||gen_random_uuid()::text,?,?,?,?,?,?,item.record_id,item.record_version,?::jsonb,?,?
    FROM jsonb_to_recordset(?::jsonb) AS item(record_id text,record_version bigint)`)
                .bind(companyId, userId, assignmentId, providerId, metric, action, value, queryHash, now, value2)
                .run();
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
export { posEvidenceMetrics } from '../../domain/executive/pos-evidence';
