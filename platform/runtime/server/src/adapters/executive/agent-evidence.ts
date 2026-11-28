import { recordExecutiveEvidenceAccess } from '@erpx/platform-runtime-ui/adapters/server/native-executive-evidence';
import "server-only";
import { type MetricId } from "@erpx/platform-shared-ui/lib/executive/canvas";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { assertProductionModuleEnabled } from "@erpx/pms-server/public/feature-availability";
import { sha256 } from "@erpx/platform-runtime-server/public/crypto";
import { currentAuthority } from "@erpx/platform-runtime-server/public/executive/access";
import { metricCatalogFor } from "@erpx/platform-runtime-server/public/executive/metrics";
import { createAgentEvidenceApplication, type AgentEvidenceRepository } from '@erpx/platform-runtime-server/application/executive/agent-evidence';
import { executiveEvidenceSources } from '@erpx/agent-server/public/executive-evidence-sources';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
function application(database: SqlDatabase) {
    const repository: AgentEvidenceRepository = { ...executiveEvidenceSources(database), async source0(companyId: string, userId: string, assignmentId: string, providerId: "agent-governance", metric: "approved-agent-versions" | "agent-invocation-outcomes", action: "search" | "open", value: string, queryHash: string, now: number, value2: string) {
            const rows = JSON.parse(value2) as Array<{ record_id: string; record_version: number | null }>;
            await recordExecutiveEvidenceAccess({
                userId, companyId, assignmentId,
                provider: providerId, metric, action,
                fieldIds: value, queryHash,
                records: rows.map((row) => ({ recordId: row.record_id, recordVersion: Number(row.record_version ?? 0) })),
            });
            return { success: true, meta: { changes: rows.length } };
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
export { agentEvidenceMetrics } from '@erpx/platform-runtime-server/domain/executive/agent-evidence';
