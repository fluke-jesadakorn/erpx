import { z } from "zod";
import { hasModuleAccess } from "@/data/module-access";
import { filtersSchema } from "@/server/modules/runtime/public/rules/canvas-domain";
import { hasPermission } from "@/lib/identity_access/permissions";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { marketingEvidenceMetrics, type MarketingEvidenceMetric, providerId, inputSchema, type MarketingEvidenceRow, fieldsFor, safeRevision, safeTimestamp, notFound, scopeKey, parseCursor, timeRange } from '../../domain/executive/marketing-evidence';
import { BusinessRuleError, type Clock } from '../../../shared';
import type { marketingExecutiveEvidenceRow } from "@/server/modules/marketing/public/marketing";
import type { assertProductionModuleEnabled } from "@/server/modules/pms/public/hospitality-rollout";
import type { currentAuthority } from "@/server/modules/runtime/public/executive/access";
import type { metricCatalogFor } from "@/server/modules/runtime/public/executive/metrics";
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
export interface MarketingEvidenceRepository {
    searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>, query: string): Promise<MarketingEvidenceRow[]>;
    source0(companyId: string, userId: string, assignmentId: string, providerId: "marketing-candidate-engagement", metric: "marketing-lead-candidates" | "marketing-engagement-events", action: "search" | "open", value: string, queryHash: string, now: number, value2: string): Promise<{
        success: boolean;
        meta?: {
            changes?: number;
            duration?: number;
            last_row_id?: number | string;
        };
    }>;
}
export interface MarketingEvidencePorts {
    repository: MarketingEvidenceRepository;
    clock: Clock;
    digest(value: string): Promise<string>;
    marketingExecutiveEvidenceRow(...args: Tail<Parameters<typeof marketingExecutiveEvidenceRow>>): ReturnType<typeof marketingExecutiveEvidenceRow>;
    assertProductionModuleEnabled(...args: Tail<Parameters<typeof assertProductionModuleEnabled>>): ReturnType<typeof assertProductionModuleEnabled>;
    currentAuthority(...args: Tail<Parameters<typeof currentAuthority>>): ReturnType<typeof currentAuthority>;
    metricCatalogFor(...args: Tail<Parameters<typeof metricCatalogFor>>): ReturnType<typeof metricCatalogFor>;
}
export function createMarketingEvidenceApplication(ports: MarketingEvidencePorts) {
    const r = ports.repository;
    async function requireMarketingEvidence(actor: WorkContext, metric: MarketingEvidenceMetric) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "records.read") || !hasModuleAccess(actor.modules, "marketing"))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "Marketing evidence requires current module read access.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "marketing");
    }
    async function writeAudit(actor: WorkContext, metric: MarketingEvidenceMetric, action: "search" | "open", rows: {
        id: string;
        version: number;
    }[], queryHash: string) {
        if (!rows.length)
            return;
        await r.source0(actor.companyId, actor.userId, actor.assignmentId, providerId, metric, action, JSON.stringify(fieldsFor(metric)), queryHash, ports.clock.now(), JSON.stringify(rows.map((row) => ({ record_id: row.id, record_version: row.version }))));
    }
    async function searchMarketingEvidence(actor: WorkContext, raw: unknown) {
        const input = inputSchema.parse(raw);
        await requireMarketingEvidence(actor, input.metric);
        const cursor = parseCursor(input.cursor, input, actor), query = input.query;
        const rows = await r.searchRows0(actor, input, cursor, query);
        const hasMore = rows.length > input.limit, page = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId, metric: input.metric, filters: input.filters, query,
            cursor: input.cursor ?? null, scope: scopeKey(actor) }));
        await ports.currentAuthority(actor);
        await writeAudit(actor, input.metric, "search", page.map((row) => ({
            id: row.id, version: safeRevision(input.metric === "marketing-lead-candidates" ? row.version : row.event_version),
        })), queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: providerId,
            metric: input.metric,
            fields: fieldsFor(input.metric),
            items: page.map((row) => {
                const eventDate = new Date(safeTimestamp(row.created_at)).toISOString().slice(0, 10);
                const candidate = input.metric === "marketing-lead-candidates";
                return {
                    id: row.id,
                    label: candidate ? row.contact_name : `${row.event_type} · ${row.contact_name}`,
                    summary: candidate ? `${row.form_name} · ${row.status} · created ${eventDate}` : `${row.form_name} · ${row.channel} · ${eventDate}`,
                    status: candidate ? row.status : row.candidate_status,
                    version: safeRevision(candidate ? row.version : row.event_version),
                    href: `/marketing/${encodeURIComponent(candidate ? row.id : row.candidate_id!)}`,
                };
            }),
            hasMore,
            nextCursor: hasMore && page.length ? Buffer.from(JSON.stringify(input.metric === "marketing-lead-candidates"
                ? { metric: input.metric, query, filters: input.filters, scope: scopeKey(actor), name: page.at(-1)!.contact_name, id: page.at(-1)!.id }
                : { metric: input.metric, query, filters: input.filters, scope: scopeKey(actor), createdAt: safeTimestamp(page.at(-1)!.created_at), id: page.at(-1)!.id })).toString("base64url") : null,
            queryHash,
        };
    }
    async function readMarketingEvidence(actor: WorkContext, rawMetric: string, recordId: string, rawFilters: unknown) {
        const metric = z.enum(marketingEvidenceMetrics).parse(rawMetric), filters = filtersSchema.parse(rawFilters);
        await requireMarketingEvidence(actor, metric);
        const row = await ports.marketingExecutiveEvidenceRow(actor, metric, recordId, metric === "marketing-engagement-events" ? timeRange(filters) : undefined) as MarketingEvidenceRow | null;
        if (!row)
            throw notFound();
        const candidate = metric === "marketing-lead-candidates", revision = safeRevision(candidate ? row.version : row.event_version);
        const record = candidate ? {
            id: row.id,
            contact_name: row.contact_name,
            form_name: row.form_name,
            form_version: safeRevision(row.form_version),
            status: row.status!,
            version: revision,
            created_at: safeTimestamp(row.created_at),
            updated_at: safeTimestamp(row.updated_at),
            branch_id: row.branch_id,
            department_id: row.department_id,
            href: `/marketing/${encodeURIComponent(row.id)}`,
        } : {
            id: row.id,
            candidate_id: row.candidate_id!,
            contact_name: row.contact_name,
            form_name: row.form_name,
            candidate_status: row.candidate_status!,
            event_type: row.event_type!,
            channel: row.channel!,
            created_at: safeTimestamp(row.created_at),
            version: revision,
            candidate_version: safeRevision(row.candidate_version),
            href: `/marketing/${encodeURIComponent(row.candidate_id!)}`,
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId, metric, filters, scope: scopeKey(actor), recordId }));
        await ports.currentAuthority(actor);
        await writeAudit(actor, metric, "open", [{ id: row.id, version: revision }], queryHash);
        await ports.currentAuthority(actor);
        return { provider: providerId, metric, fields: fieldsFor(metric), record, queryHash };
    }
    return { searchMarketingEvidence, readMarketingEvidence };
}
