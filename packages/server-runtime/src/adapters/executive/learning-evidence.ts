import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { learningExecutiveEvidenceRow } from "@erpx/server-learning/public/learning";
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/hospitality-rollout";
import { ApiError } from "@erpx/server-runtime/public/http";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { currentAuthority } from "@erpx/server-runtime/public/executive/access";
import { metricCatalogFor } from "@erpx/server-runtime/public/executive/metrics";
import { createLearningEvidenceApplication, type LearningEvidenceRepository } from '@erpx/server-runtime/application/executive/learning-evidence';
import { executiveEvidenceSources } from '@erpx/server-learning/public/executive-evidence-sources';
import { BusinessRuleError } from '@erpx/server-shared/index';
function application(database: SqlDatabase) {
    const repository: LearningEvidenceRepository = { ...executiveEvidenceSources(database), source0(companyId: string, userId: string, assignmentId: string, providerId: "learning-assignment-records", metric: "learning-completions" | "overdue-required-learning", action: "search" | "open", value: string, queryHash: string, now: number, value2: string) {
            return database.prepare(`INSERT INTO executive_evidence_access_events
    (id,company_id,actor_id,assignment_id,provider_id,metric_id,action,record_id,record_version,field_ids,query_hash,created_at)
    SELECT 'executive-evidence-'||gen_random_uuid()::text,?,?,?,?,?,?,item.record_id,item.record_version,?::jsonb,?,?
    FROM jsonb_to_recordset(?::jsonb) AS item(record_id text,record_version bigint)`)
                .bind(companyId, userId, assignmentId, providerId, metric, action, value, queryHash, now, value2)
                .run();
        } };
    return createLearningEvidenceApplication({ repository, clock: { now: Date.now }, digest: sha256, learningExecutiveEvidenceRow: (...args) => fromNative(() => learningExecutiveEvidenceRow(database, ...args)), assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(database, ...args)), currentAuthority: (...args) => fromNative(() => currentAuthority(database, ...args)), metricCatalogFor: (...args) => fromNative(() => metricCatalogFor(database, ...args)) });
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
export async function searchLearningEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) { return invoke(() => application(db).searchLearningEvidence(actor, raw)); }
export async function readLearningEvidence(db: SqlDatabase, actor: WorkContext, metric: string, recordId: string, rawFilters: unknown) { return invoke(() => application(db).readLearningEvidence(actor, metric, recordId, rawFilters)); }
export { learningEvidenceMetrics } from '@erpx/server-runtime/domain/executive/learning-evidence';
