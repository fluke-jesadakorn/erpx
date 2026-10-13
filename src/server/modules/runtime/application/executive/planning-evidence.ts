import { z } from "zod";
import { hasModuleAccess } from "@/data/module-access";
import { filtersSchema } from "@/server/modules/runtime/public/rules/canvas-domain";
import { hasPermission } from "@/lib/identity_access/permissions";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { PlanningExecutiveEvidenceMetric } from "@/server/modules/manufacturing/public/production";
import { planningEvidenceMetrics, type PlanningEvidenceMetric, providerId, inputSchema, type EvidenceRow, fieldsFor, scopeKey, parseCursor, safeRevision, safeWholeText, safeTimestamp, safeDate, shortages, notFound } from '../../domain/executive/planning-evidence';
import { BusinessRuleError, type Clock } from '../../../shared';
import type { planningExecutiveEvidenceRow } from "@/server/modules/manufacturing/public/production";
import type { assertProductionModuleEnabled } from "@/server/modules/pms/public/hospitality-rollout";
import type { currentAuthority } from "@/server/modules/runtime/public/executive/access";
import type { metricCatalogFor } from "@/server/modules/runtime/public/executive/metrics";
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
export interface PlanningEvidenceRepository {
    searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>, isScenario: boolean): Promise<EvidenceRow[]>;
    source0(companyId: string, userId: string, assignmentId: string, providerId: "planning-scenarios-recommendations", metric: "planning-shortage-scenarios" | "proposed-planning-recommendations" | "capacity-feasible-recommendations", action: "search" | "open", value: string, queryHash: string, now: number, value2: string): Promise<{
        success: boolean;
        meta?: {
            changes?: number;
            duration?: number;
            last_row_id?: number | string;
        };
    }>;
}
export interface PlanningEvidencePorts {
    repository: PlanningEvidenceRepository;
    clock: Clock;
    digest(value: string): Promise<string>;
    planningExecutiveEvidenceRow(...args: Tail<Parameters<typeof planningExecutiveEvidenceRow>>): ReturnType<typeof planningExecutiveEvidenceRow>;
    assertProductionModuleEnabled(...args: Tail<Parameters<typeof assertProductionModuleEnabled>>): ReturnType<typeof assertProductionModuleEnabled>;
    currentAuthority(...args: Tail<Parameters<typeof currentAuthority>>): ReturnType<typeof currentAuthority>;
    metricCatalogFor(...args: Tail<Parameters<typeof metricCatalogFor>>): ReturnType<typeof metricCatalogFor>;
}
export function createPlanningEvidenceApplication(ports: PlanningEvidencePorts) {
    const r = ports.repository;
    async function requirePlanningEvidence(actor: WorkContext, metric: PlanningEvidenceMetric) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "records.read") || !hasModuleAccess(actor.modules, "planning"))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "Planning evidence requires current module read access.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "planning");
    }
    async function writeAudit(actor: WorkContext, metric: PlanningEvidenceMetric, action: "search" | "open", rows: {
        id: string;
        version: number;
    }[], queryHash: string) {
        if (!rows.length)
            return;
        await r.source0(actor.companyId, actor.userId, actor.assignmentId, providerId, metric, action, JSON.stringify(fieldsFor(metric)), queryHash, ports.clock.now(), JSON.stringify(rows.map((row) => ({ record_id: row.id, record_version: row.version }))));
    }
    async function searchPlanningEvidence(actor: WorkContext, raw: unknown) {
        const input = inputSchema.parse(raw);
        await requirePlanningEvidence(actor, input.metric);
        const cursor = parseCursor(input.cursor, input, actor), isScenario = input.metric === "planning-shortage-scenarios";
        const rows = await r.searchRows0(actor, input, cursor, isScenario);
        const hasMore = rows.length > input.limit, page = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId, metric: input.metric, filters: input.filters, query: input.query,
            cursor: input.cursor ?? null, scope: scopeKey(actor) }));
        const auditRows = page.flatMap((row) => isScenario
            ? [{ id: row.scenario_id, version: safeRevision(row.scenario_version) }]
            : [
                { id: row.recommendation_id!, version: safeRevision(row.recommendation_version) },
                { id: row.scenario_id, version: safeRevision(row.scenario_version) },
            ]);
        await ports.currentAuthority(actor);
        await writeAudit(actor, input.metric, "search", auditRows, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: providerId,
            metric: input.metric,
            fields: fieldsFor(input.metric),
            items: page.map((row) => {
                const scenario = input.metric === "planning-shortage-scenarios";
                return {
                    id: row.id,
                    label: scenario ? `Scenario · ${row.specification_name ?? row.scenario_id}` : `${row.specification_name ?? row.scenario_id} · ${safeDate(row.feasible_date)}`,
                    summary: scenario
                        ? `${row.branch_name ?? "Branch"} · ${row.warehouse_name ?? "Warehouse"} · ${safeWholeText(row.shortage_count, "shortage count")} material shortage(s) · due ${safeDate(row.due_date)}`
                        : `Scenario ${row.scenario_id} · ${row.recommendation_status} · ${safeWholeText(row.required_minutes, "required minutes")}/${safeWholeText(row.capacity_minutes, "capacity minutes")} minutes · feasible ${safeDate(row.feasible_date)}`,
                    status: scenario ? row.scenario_status : row.recommendation_status,
                    version: safeRevision(scenario ? row.scenario_version : row.recommendation_version),
                    href: "/planning",
                };
            }),
            hasMore,
            nextCursor: hasMore && page.length ? Buffer.from(JSON.stringify({ metric: input.metric, filters: input.filters,
                query: input.query, scope: scopeKey(actor), id: page.at(-1)!.id })).toString("base64url") : null,
            queryHash,
        };
    }
    async function readPlanningEvidence(actor: WorkContext, rawMetric: string, recordId: string, rawFilters: unknown) {
        const metric = z.enum(planningEvidenceMetrics).parse(rawMetric), filters = filtersSchema.parse(rawFilters);
        await requirePlanningEvidence(actor, metric);
        const row = await ports.planningExecutiveEvidenceRow(actor, metric as PlanningExecutiveEvidenceMetric, recordId, filters.from, filters.to) as EvidenceRow | null;
        if (!row)
            throw notFound();
        const scenario = metric === "planning-shortage-scenarios";
        const sourceShortages = shortages(row.shortage_items_json);
        const record = scenario ? {
            id: row.scenario_id,
            branch_name: row.branch_name ?? "Branch",
            warehouse_name: row.warehouse_name ?? "Warehouse",
            specification_name: row.specification_name ?? "Specification",
            output_product_name: row.output_product_name,
            specification_version: safeRevision(row.specification_version),
            demand_quantity_milli: safeWholeText(row.demand_quantity_milli, "demand quantity"),
            due_date: safeDate(row.due_date),
            capacity_minutes: safeWholeText(row.capacity_minutes, "capacity minutes"),
            shortages: sourceShortages,
            recommendation_id: row.recommendation_id,
            recommendation_status: row.recommendation_status,
            recommendation_version: row.recommendation_version === null ? null : safeRevision(row.recommendation_version),
            status: row.scenario_status,
            version: safeRevision(row.scenario_version),
            created_at: safeTimestamp(row.scenario_created_at),
            updated_at: safeTimestamp(row.scenario_updated_at),
            href: "/planning",
        } : {
            id: row.recommendation_id!,
            scenario_id: row.scenario_id,
            branch_name: row.branch_name ?? "Branch",
            warehouse_name: row.warehouse_name ?? "Warehouse",
            specification_name: row.specification_name ?? "Specification",
            output_product_name: row.output_product_name,
            specification_version: safeRevision(row.specification_version),
            scenario_version: safeRevision(row.scenario_version),
            demand_quantity_milli: safeWholeText(row.demand_quantity_milli, "demand quantity"),
            scenario_due_date: safeDate(row.due_date),
            capacity_minutes: safeWholeText(row.capacity_minutes, "capacity minutes"),
            shortages: sourceShortages,
            recommended_quantity_milli: safeWholeText(row.recommended_quantity_milli, "recommended quantity"),
            required_minutes: safeWholeText(row.required_minutes, "required minutes"),
            feasible_date: safeDate(row.feasible_date),
            status: row.recommendation_status!,
            version: safeRevision(row.recommendation_version),
            created_at: safeTimestamp(row.recommendation_created_at),
            updated_at: safeTimestamp(row.recommendation_updated_at),
            href: "/planning",
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId, metric, filters, scope: scopeKey(actor), recordId }));
        const auditRows = scenario
            ? [{ id: row.scenario_id, version: safeRevision(row.scenario_version) }]
            : [
                { id: row.recommendation_id!, version: safeRevision(row.recommendation_version) },
                { id: row.scenario_id, version: safeRevision(row.scenario_version) },
            ];
        await ports.currentAuthority(actor);
        await writeAudit(actor, metric, "open", auditRows, queryHash);
        await ports.currentAuthority(actor);
        return { provider: providerId, metric, fields: fieldsFor(metric), record, queryHash };
    }
    return { searchPlanningEvidence, readPlanningEvidence };
}
