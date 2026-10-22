import { z } from "zod";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import { filtersSchema } from "@erpx/server-runtime/public/rules/canvas-domain";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { brandEvidenceMetrics, type BrandEvidenceMetric, providerId, fields, inputSchema, type BrandProfileRow, parseCursor, encodeCursor, safeRevision, notFound, timeRange } from '@erpx/server-runtime/domain/executive/brand-evidence';
import { BusinessRuleError, type Clock } from '@erpx/server-shared/index';
import type { brandProfileEvidenceRow } from "@erpx/server-brand/public/branding";
import type { assertProductionModuleEnabled } from "@erpx/server-pms/public/hospitality-rollout";
import type { companyAuthority } from "@erpx/server-operations/public/staff-policy";
import type { currentAuthority } from "@erpx/server-runtime/public/executive/access";
import type { metricCatalogFor } from "@erpx/server-runtime/public/executive/metrics";
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
export interface BrandEvidenceRepository {
    searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>): Promise<BrandProfileRow[]>;
    source0(companyId: string, userId: string, assignmentId: string, providerId: "brand-profile-records", metric: "brand-profiles" | "brand-profile-updates", action: "search" | "open", value: string, queryHash: string, now: number, value2: string): Promise<{
        success: boolean;
        meta?: {
            changes?: number;
            duration?: number;
            last_row_id?: number | string;
        };
    }>;
}
export interface BrandEvidencePorts {
    repository: BrandEvidenceRepository;
    clock: Clock;
    digest(value: string): Promise<string>;
    brandProfileEvidenceRow(...args: Tail<Parameters<typeof brandProfileEvidenceRow>>): ReturnType<typeof brandProfileEvidenceRow>;
    assertProductionModuleEnabled(...args: Tail<Parameters<typeof assertProductionModuleEnabled>>): ReturnType<typeof assertProductionModuleEnabled>;
    companyAuthority(...args: Tail<Parameters<typeof companyAuthority>>): ReturnType<typeof companyAuthority>;
    currentAuthority(...args: Tail<Parameters<typeof currentAuthority>>): ReturnType<typeof currentAuthority>;
    metricCatalogFor(...args: Tail<Parameters<typeof metricCatalogFor>>): ReturnType<typeof metricCatalogFor>;
}
export function createBrandEvidenceApplication(ports: BrandEvidencePorts) {
    const r = ports.repository;
    async function requireBrandEvidence(actor: WorkContext, metric: BrandEvidenceMetric) {
        await ports.currentAuthority(actor);
        const catalog = await ports.metricCatalogFor(actor);
        if (!hasPermission(actor, "records.read") || !hasModuleAccess(actor.modules, "brand")
            || actor.branchId || actor.departmentId || !await ports.companyAuthority(actor)
            || !catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "brand");
    }
    async function writeAudit(actor: WorkContext, metric: BrandEvidenceMetric, action: "search" | "open", rows: {
        id: string;
        version: number;
    }[], queryHash: string) {
        if (!rows.length)
            return;
        await r.source0(actor.companyId, actor.userId, actor.assignmentId, providerId, metric, action, JSON.stringify(fields), queryHash, ports.clock.now(), JSON.stringify(rows.map((row) => ({ record_id: row.id, record_version: row.version }))));
    }
    async function searchBrandEvidence(actor: WorkContext, raw: unknown) {
        const input = inputSchema.parse(raw);
        await requireBrandEvidence(actor, input.metric);
        const cursor = parseCursor(input.cursor);
        const rows = await r.searchRows0(actor, input, cursor);
        const hasMore = rows.length > input.limit, page = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({
            providerId, metric: input.metric, filters: input.filters, query: input.query,
            cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId],
        }));
        await ports.currentAuthority(actor);
        await writeAudit(actor, input.metric, "search", page.map((row) => ({ id: row.id, version: safeRevision(row.updated_at) })), queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: providerId,
            metric: input.metric,
            fields,
            items: page.map((row) => ({
                id: row.id,
                label: row.branch_name,
                summary: `${row.document_mode} documents · updated ${new Date(safeRevision(row.updated_at)).toISOString().slice(0, 10)}`,
                status: row.document_mode,
                version: safeRevision(row.updated_at),
                href: "/settings?section=organization",
            })),
            hasMore,
            nextCursor: hasMore && page.length ? encodeCursor(page.at(-1)!) : null,
            queryHash,
        };
    }
    async function readBrandEvidence(actor: WorkContext, metric: string, recordId: string, rawFilters: unknown) {
        const parsedMetric = z.enum(brandEvidenceMetrics).parse(metric), filters = filtersSchema.parse(rawFilters);
        await requireBrandEvidence(actor, parsedMetric);
        const id = z.string().min(1).max(200).parse(recordId);
        const row = await ports.brandProfileEvidenceRow(actor, id) as BrandProfileRow | null;
        if (!row)
            throw notFound();
        const revision = safeRevision(row.updated_at);
        if (parsedMetric === "brand-profile-updates") {
            const range = timeRange(filters);
            if (revision < range.from || revision >= range.toExclusive)
                throw notFound();
        }
        const record = {
            id: row.id,
            branch_name: row.branch_name,
            document_mode: row.document_mode,
            updated_at: revision,
            primary_configured: row.primary_attachment_id !== null,
            inverse_configured: row.inverse_attachment_id !== null,
            version: revision,
            href: "/settings?section=organization",
        };
        const queryHash = await ports.digest(JSON.stringify({
            providerId, metric: parsedMetric, filters,
            scope: [actor.companyId, actor.assignmentId], recordId: id,
        }));
        await ports.currentAuthority(actor);
        await writeAudit(actor, parsedMetric, "open", [{ id, version: revision }], queryHash);
        await ports.currentAuthority(actor);
        return { provider: providerId, metric: parsedMetric, fields, record, queryHash };
    }
    return { searchBrandEvidence, readBrandEvidence };
}
