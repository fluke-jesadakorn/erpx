import { z } from "zod";
import { hasModuleAccess } from "@/data/module-access";
import { filtersSchema } from "@/server/modules/runtime/public/rules/canvas-domain";
import { hasPermission } from "@/lib/identity_access/permissions";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { talentEvidenceMetrics, type TalentEvidenceMetric, providerId, requisitionFields, applicationFields, inputSchema, type TalentEvidenceRow, parseCursor, safeVersion, safeTimestamp, utcRange, notFound } from '../../domain/executive/talent-evidence';
import { BusinessRuleError, type Clock } from '../../../shared';
import type { talentExecutiveEvidenceRow } from "@/server/modules/talent/public/talent";
import type { assertProductionModuleEnabled } from "@/server/modules/pms/public/hospitality-rollout";
import type { currentAuthority } from "@/server/modules/runtime/public/executive/access";
import type { metricCatalogFor } from "@/server/modules/runtime/public/executive/metrics";
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
export interface TalentEvidenceRepository {
    searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>): Promise<TalentEvidenceRow[]>;
    source0(companyId: string, userId: string, assignmentId: string, providerId: "talent-recruiting-records", metric: "open-requisitions" | "application-stage-counts", action: "search" | "open", value: string, queryHash: string, now: number, value2: string): Promise<{
        success: boolean;
        meta?: {
            changes?: number;
            duration?: number;
            last_row_id?: number | string;
        };
    }>;
}
export interface TalentEvidencePorts {
    repository: TalentEvidenceRepository;
    clock: Clock;
    digest(value: string): Promise<string>;
    talentExecutiveEvidenceRow(...args: Tail<Parameters<typeof talentExecutiveEvidenceRow>>): ReturnType<typeof talentExecutiveEvidenceRow>;
    assertProductionModuleEnabled(...args: Tail<Parameters<typeof assertProductionModuleEnabled>>): ReturnType<typeof assertProductionModuleEnabled>;
    currentAuthority(...args: Tail<Parameters<typeof currentAuthority>>): ReturnType<typeof currentAuthority>;
    metricCatalogFor(...args: Tail<Parameters<typeof metricCatalogFor>>): ReturnType<typeof metricCatalogFor>;
}
export function createTalentEvidenceApplication(ports: TalentEvidencePorts) {
    const r = ports.repository;
    async function requireTalentEvidence(actor: WorkContext, metric: TalentEvidenceMetric) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "records.read") || !hasPermission(actor, "records.review")
            || !hasModuleAccess(actor.modules, "talent"))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "Talent evidence requires current reviewer access.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "talent");
    }
    async function writeAudit(actor: WorkContext, metric: TalentEvidenceMetric, action: "search" | "open", rows: {
        id: string;
        version: number;
    }[], queryHash: string) {
        if (!rows.length)
            return;
        const fields = metric === "open-requisitions" ? requisitionFields : applicationFields;
        await r.source0(actor.companyId, actor.userId, actor.assignmentId, providerId, metric, action, JSON.stringify(fields), queryHash, ports.clock.now(), JSON.stringify(rows.map((row) => ({ record_id: row.id, record_version: row.version }))));
    }
    async function searchTalentEvidence(actor: WorkContext, raw: unknown) {
        const input = inputSchema.parse(raw);
        await requireTalentEvidence(actor, input.metric);
        const cursor = parseCursor(input.cursor, input.metric);
        const rows = await r.searchRows0(actor, input, cursor);
        const hasMore = rows.length > input.limit, page = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({
            providerId, metric: input.metric, filters: input.filters, query: input.query,
            cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId],
        }));
        await ports.currentAuthority(actor);
        await writeAudit(actor, input.metric, "search", page.map((row) => ({ id: row.id, version: safeVersion(row.version) })), queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: providerId,
            metric: input.metric,
            fields: input.metric === "open-requisitions" ? requisitionFields : applicationFields,
            items: page.map((row) => ({
                id: row.id,
                label: row.job_title,
                summary: input.metric === "open-requisitions"
                    ? `Open · start ${row.target_start_date}`
                    : `${row.status.replaceAll("_", " ")} · updated ${new Date(safeTimestamp(row.updated_at)).toISOString().slice(0, 10)}`,
                status: row.status,
                version: safeVersion(row.version),
                href: `/talent/${encodeURIComponent(row.id)}`,
            })),
            hasMore,
            nextCursor: hasMore && page.length ? Buffer.from(JSON.stringify(input.metric === "open-requisitions"
                ? { jobTitle: page.at(-1)!.job_title, id: page.at(-1)!.id }
                : { updatedAt: safeTimestamp(page.at(-1)!.updated_at), id: page.at(-1)!.id })).toString("base64url") : null,
            queryHash,
        };
    }
    async function readTalentEvidence(actor: WorkContext, metric: string, recordId: string, rawFilters: unknown) {
        const parsedMetric = z.enum(talentEvidenceMetrics).parse(metric), filters = filtersSchema.parse(rawFilters);
        await requireTalentEvidence(actor, parsedMetric);
        const range = parsedMetric === "application-stage-counts" ? utcRange(filters) : undefined;
        const row = await ports.talentExecutiveEvidenceRow(actor, parsedMetric, recordId, range) as TalentEvidenceRow | null;
        if (!row)
            throw notFound();
        const version = safeVersion(row.version), fields = parsedMetric === "open-requisitions" ? requisitionFields : applicationFields;
        const record = parsedMetric === "open-requisitions" ? {
            id: row.id,
            job_title: row.job_title,
            target_start_date: row.target_start_date!,
            status: row.status,
            version,
            updated_at: safeTimestamp(row.updated_at),
            branch_id: row.branch_id,
            department_id: row.department_id,
            href: `/talent/${encodeURIComponent(row.id)}`,
        } : {
            id: row.id,
            requisition_id: row.requisition_id!,
            job_title: row.job_title,
            status: row.status,
            version,
            updated_at: safeTimestamp(row.updated_at),
            branch_id: row.branch_id,
            department_id: row.department_id,
            href: `/talent/${encodeURIComponent(row.id)}`,
        };
        const queryHash = await ports.digest(JSON.stringify({
            providerId, metric: parsedMetric, filters,
            scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId,
        }));
        await ports.currentAuthority(actor);
        await writeAudit(actor, parsedMetric, "open", [{ id: row.id, version }], queryHash);
        await ports.currentAuthority(actor);
        return { provider: providerId, metric: parsedMetric, fields, record, queryHash };
    }
    return { searchTalentEvidence, readTalentEvidence };
}
