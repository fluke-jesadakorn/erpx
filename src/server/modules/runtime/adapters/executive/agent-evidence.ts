import "server-only";
import { type MetricId } from "@/lib/shared/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError } from "@/server/modules/runtime/public/http";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { assertProductionModuleEnabled } from "@/server/modules/pms/public/hospitality-rollout";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { currentAuthority } from "@/server/modules/runtime/public/executive/access";
import { metricCatalogFor } from "@/server/modules/runtime/public/executive/metrics";
import { createAgentEvidenceApplication, type AgentEvidenceRepository } from '../../application/executive/agent-evidence';
import { executiveEvidenceSources } from '../../../agent/public/executive-evidence-sources';
import { BusinessRuleError } from '../../../shared';
function application(database: SqlDatabase) {
    const repository: AgentEvidenceRepository = { ...executiveEvidenceSources(database), source0(companyId: string, userId: string, assignmentId: string, providerId: "agent-governance", metric: "approved-agent-versions" | "agent-invocation-outcomes", action: "search" | "open", value: string, queryHash: string, now: number, value2: string) {
            return database.prepare(`INSERT INTO executive_evidence_access_events
    (id,company_id,actor_id,assignment_id,provider_id,metric_id,action,record_id,record_version,field_ids,query_hash,created_at)
    SELECT 'executive-evidence-'||gen_random_uuid()::text,?,?,?,?,?,?,NULLIF(item.record_id,''),NULLIF(item.record_version,0),?::jsonb,?,?
    FROM jsonb_to_recordset(?::jsonb) AS item(record_id text,record_version bigint)`)
                .bind(companyId, userId, assignmentId, providerId, metric, action, value, queryHash, now, value2).run();
        } };
    return createAgentEvidenceApplication({ repository, clock: { now: Date.now }, digest: sha256, assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(database, ...args)), currentAuthority: (...args) => fromNative(() => currentAuthority(database, ...args)), metricCatalogFor: (...args) => fromNative(() => metricCatalogFor(database, ...args)) });
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
export async function searchAgentEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) { return invoke(() => application(db).searchAgentEvidence(actor, raw)); }
export async function readAgentEvidence(db: SqlDatabase, actor: WorkContext, metric: MetricId, recordId: string, rawFilters: unknown) { return invoke(() => application(db).readAgentEvidence(actor, metric, recordId, rawFilters)); }
export { agentEvidenceMetrics } from '../../domain/executive/agent-evidence';
