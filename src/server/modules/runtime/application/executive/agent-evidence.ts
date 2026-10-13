import { z } from "zod";
import { hasModuleAccess } from "@/data/module-access";
import { hasPermission } from "@/lib/identity_access/permissions";
import { filtersSchema, type MetricId } from "@/server/modules/runtime/public/rules/canvas-domain";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { agentEvidenceMetrics, type AgentEvidenceMetric, providerId, approvedVersionMetric, approvedVersionFields, invocationFields, inputSchema, type ApprovedVersionRow, type InvocationRow, parseCursor, encodeCursor, exactNumber } from '../../domain/executive/agent-evidence';
import { BusinessRuleError, type Clock } from '../../../shared';
import type { assertProductionModuleEnabled } from "@/server/modules/pms/public/hospitality-rollout";
import type { currentAuthority } from "@/server/modules/runtime/public/executive/access";
import type { metricCatalogFor } from "@/server/modules/runtime/public/executive/metrics";
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
export interface AgentEvidenceRepository {
    searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>): Promise<{
        rows: ApprovedVersionRow[] | InvocationRow[];
        fields: string[];
    }>;
    source0(companyId: string, userId: string, assignmentId: string, providerId: "agent-governance", metric: "approved-agent-versions" | "agent-invocation-outcomes", action: "search" | "open", value: string, queryHash: string, now: number, value2: string): Promise<{
        success: boolean;
        meta?: {
            changes?: number;
            duration?: number;
            last_row_id?: number | string;
        };
    }>;
    source1(companyId: string, id: string): Promise<ApprovedVersionRow | null>;
    source2(companyId: string, value: number, value2: number, id: string): Promise<InvocationRow | null>;
}
export interface AgentEvidencePorts {
    repository: AgentEvidenceRepository;
    clock: Clock;
    digest(value: string): Promise<string>;
    assertProductionModuleEnabled(...args: Tail<Parameters<typeof assertProductionModuleEnabled>>): ReturnType<typeof assertProductionModuleEnabled>;
    currentAuthority(...args: Tail<Parameters<typeof currentAuthority>>): ReturnType<typeof currentAuthority>;
    metricCatalogFor(...args: Tail<Parameters<typeof metricCatalogFor>>): ReturnType<typeof metricCatalogFor>;
}
export function createAgentEvidenceApplication(ports: AgentEvidencePorts) {
    const r = ports.repository;
    async function requireAgentEvidence(actor: WorkContext, metric: AgentEvidenceMetric) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "records.read") || !hasModuleAccess(actor.modules, "agent"))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "agent");
    }
    async function writeAudit(actor: WorkContext, metric: AgentEvidenceMetric, action: "search" | "open", records: {
        id: string;
        version: number | null;
    }[], fields: string[], queryHash: string) {
        if (!records.length && action === "search")
            return;
        const rows = records.length ? records : [{ id: "", version: 0 }];
        await r.source0(actor.companyId, actor.userId, actor.assignmentId, providerId, metric, action, JSON.stringify(fields), queryHash, ports.clock.now(), JSON.stringify(rows.map((row) => ({ record_id: row.id, record_version: row.version }))));
    }
    async function searchAgentEvidence(actor: WorkContext, raw: unknown) {
        const input = inputSchema.parse(raw);
        await requireAgentEvidence(actor, input.metric);
        const cursor = parseCursor(input.cursor);
        const { rows, fields } = await r.searchRows0(actor, input, cursor);
        const hasMore = rows.length > input.limit, page = rows.slice(0, input.limit), queryHash = await ports.digest(JSON.stringify({
            providerId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null,
            scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId],
        }));
        await ports.currentAuthority(actor);
        await writeAudit(actor, input.metric, "search", page.map((row) => ({ id: row.id, version: input.metric === approvedVersionMetric ? exactNumber((row as ApprovedVersionRow).version, "agent version") : 1 })), fields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: providerId,
            metric: input.metric,
            fields,
            items: input.metric === approvedVersionMetric
                ? (page as ApprovedVersionRow[]).map((row) => ({
                    id: row.id,
                    label: `${row.agent_name} · Version ${exactNumber(row.version_number, "approved version number")}`,
                    summary: `Approved agent · Revision ${exactNumber(row.version, "agent version")}`,
                    version: exactNumber(row.version, "agent version"),
                    href: "/agents",
                }))
                : (page as InvocationRow[]).map((row) => ({
                    id: row.id,
                    label: `${row.outcome === "succeeded" ? "Succeeded" : "Failed"} invocation`,
                    summary: `${row.turn_id} · ${new Date(exactNumber(row.created_at, "invocation time")).toISOString()}`,
                    version: 1,
                    href: "/agents",
                })),
            hasMore,
            nextCursor: hasMore && page.length ? encodeCursor(page.at(-1)!) : null,
            queryHash,
        };
    }
    async function readAgentEvidence(actor: WorkContext, metric: MetricId, recordId: string, rawFilters: unknown) {
        const parsedMetric = z.enum(agentEvidenceMetrics).parse(metric), filters = filtersSchema.parse(rawFilters);
        await requireAgentEvidence(actor, parsedMetric);
        const id = z.string().min(1).max(200).parse(recordId);
        let fields: string[];
        let record: Record<string, string | number> & {
            id: string;
            href: string;
        };
        let version: number | null;
        if (parsedMetric === approvedVersionMetric) {
            const row = await r.source1(actor.companyId, id);
            if (!row)
                throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
            fields = approvedVersionFields;
            version = exactNumber(row.version, "agent version");
            record = {
                id: row.id, agent_id: row.agent_id, agent_name: row.agent_name, agent_status: row.agent_status,
                version_status: row.version_status, version_number: exactNumber(row.version_number, "approved version number"),
                version, created_at: exactNumber(row.created_at, "version creation time"), updated_at: exactNumber(row.updated_at, "version update time"), href: "/agents",
            };
        }
        else {
            const row = await r.source2(actor.companyId, Date.parse(filters.from), Date.parse(filters.to) + 86400000, id);
            if (!row)
                throw new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
            fields = invocationFields;
            version = 1;
            record = { id: row.id, turn_id: row.turn_id, outcome: row.outcome, created_at: exactNumber(row.created_at, "invocation time"), href: "/agents" };
        }
        const queryHash = await ports.digest(JSON.stringify({ providerId, metric: parsedMetric, filters, scope: [actor.companyId, actor.assignmentId], recordId: id }));
        await ports.currentAuthority(actor);
        await writeAudit(actor, parsedMetric, "open", [{ id, version }], fields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: providerId, metric: parsedMetric, fields, record, queryHash };
    }
    return { searchAgentEvidence, readAgentEvidence };
}
