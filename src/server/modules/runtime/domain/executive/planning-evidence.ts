import { z } from "zod";
import { filtersSchema, type Filters } from "@/server/modules/runtime/public/rules/canvas-domain";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { BusinessRuleError } from '../../../shared';
export const planningEvidenceMetrics = [
    "planning-shortage-scenarios",
    "proposed-planning-recommendations",
    "capacity-feasible-recommendations",
] as const;
export type PlanningEvidenceMetric = typeof planningEvidenceMetrics[number];
export const providerId = "planning-scenarios-recommendations";
export const scenarioFields = [
    "planning_scenarios.id", "planning_scenarios.company_id", "planning_scenarios.branch_id", "planning_scenarios.warehouse_id",
    "planning_scenarios.specification_id", "planning_scenarios.specification_version", "planning_scenarios.demand_quantity_milli",
    "planning_scenarios.due_date", "planning_scenarios.capacity_minutes", "planning_scenarios.shortages_json", "planning_scenarios.status",
    "planning_scenarios.version", "planning_scenarios.created_at", "planning_scenarios.updated_at", "manufacturing_specifications.name",
    "commerce_products.name", "commerce_warehouses.name", "organization_nodes.display_name", "planning_recommendations.id",
    "planning_recommendations.status", "planning_recommendations.required_minutes", "planning_recommendations.feasible_date",
    "planning_recommendations.version",
];
export const recommendationFields = [
    "planning_recommendations.id", "planning_recommendations.scenario_id", "planning_recommendations.status",
    "planning_recommendations.recommended_quantity_milli", "planning_recommendations.required_minutes", "planning_recommendations.feasible_date",
    "planning_recommendations.version", "planning_recommendations.scenario_version", "planning_recommendations.created_at",
    "planning_recommendations.updated_at", "planning_scenarios.id", "planning_scenarios.branch_id", "planning_scenarios.warehouse_id",
    "planning_scenarios.specification_id", "planning_scenarios.specification_version", "planning_scenarios.demand_quantity_milli",
    "planning_scenarios.due_date", "planning_scenarios.capacity_minutes", "planning_scenarios.shortages_json", "planning_scenarios.status",
    "planning_scenarios.version", "manufacturing_specifications.name", "commerce_products.name", "commerce_warehouses.name",
    "organization_nodes.display_name",
];
export const inputSchema = z.object({
    metric: z.enum(planningEvidenceMetrics),
    filters: filtersSchema,
    query: z.string().trim().max(100).default(""),
    cursor: z.string().max(2000).optional(),
    limit: z.coerce.number().int().min(1).max(50).default(25),
}).strict();
export type Cursor = {
    metric: PlanningEvidenceMetric;
    filters: Filters;
    query: string;
    scope: string[];
    id: string;
};
export type EvidenceRow = {
    id: string;
    scenario_id: string;
    recommendation_id: string | null;
    branch_id: string;
    branch_name: string | null;
    warehouse_id: string;
    warehouse_name: string | null;
    specification_id: string;
    specification_name: string | null;
    output_product_id: string | null;
    output_product_name: string | null;
    specification_version: string | number | bigint;
    demand_quantity_milli: string | number | bigint;
    due_date: string;
    capacity_minutes: string | number | bigint;
    scenario_status: string;
    scenario_version: string | number | bigint;
    scenario_created_at: string | number | bigint;
    scenario_updated_at: string | number | bigint;
    recommendation_status: string | null;
    recommended_quantity_milli: string | number | bigint | null;
    required_minutes: string | number | bigint | null;
    feasible_date: string | null;
    recommendation_version: string | number | bigint | null;
    scenario_version_ref: string | number | bigint | null;
    recommendation_created_at: string | number | bigint | null;
    recommendation_updated_at: string | number | bigint | null;
    shortage_count: string | number | bigint;
    shortage_items_json: string;
};
export function fieldsFor(metric: PlanningEvidenceMetric) {
    return metric === "planning-shortage-scenarios" ? scenarioFields : recommendationFields;
}
export function scopeKey(actor: WorkContext) {
    return [actor.companyId, actor.assignmentId, actor.branchId ?? "", actor.departmentId ?? ""];
}
export function parseCursor(value: string | undefined, input: z.infer<typeof inputSchema>, actor: WorkContext): Cursor | null {
    if (!value)
        return null;
    try {
        const parsed = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as Record<string, unknown>;
        const cursor = {
            metric: z.enum(planningEvidenceMetrics).parse(parsed.metric),
            filters: filtersSchema.parse(parsed.filters),
            query: z.string().max(100).parse(parsed.query),
            scope: z.array(z.string()).max(4).parse(parsed.scope),
            id: z.string().min(1).max(200).parse(parsed.id),
        };
        if (cursor.metric !== input.metric || JSON.stringify(cursor.filters) !== JSON.stringify(input.filters)
            || cursor.query !== input.query || JSON.stringify(cursor.scope) !== JSON.stringify(scopeKey(actor)))
            throw new Error();
        return cursor;
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function safeRevision(value: string | number | bigint | null | undefined) {
    const revision = Number(value);
    if (!Number.isSafeInteger(revision) || revision < 1)
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", "The Planning source returned an invalid record revision.");
    return revision;
}
export function safeWholeText(value: string | number | bigint | null | undefined, name: string) {
    const text = String(value);
    if (!/^(0|[1-9][0-9]*)$/.test(text))
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", `The Planning source returned an invalid ${name}.`);
    return text;
}
export function safeTimestamp(value: string | number | bigint | null | undefined) {
    const timestamp = Number(value);
    if (!Number.isSafeInteger(timestamp) || timestamp < 1)
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", "The Planning source returned an invalid record time.");
    return timestamp;
}
export function safeDate(value: string | null | undefined) {
    if (!value || !/^\d{4}-\d{2}-\d{2}$/.test(value) || !Number.isFinite(Date.parse(`${value}T00:00:00Z`)))
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", "The Planning source returned an invalid date.");
    return value;
}
export function shortages(value: string) {
    const schema = z.array(z.object({
        product_name: z.string().min(1).max(160),
        required_milli: z.string().regex(/^(0|[1-9][0-9]*)$/),
        available_milli: z.string().regex(/^(0|[1-9][0-9]*)$/),
        shortage_milli: z.string().regex(/^[1-9][0-9]*$/),
    }).strict()).max(100);
    try {
        return schema.parse(JSON.parse(value));
    }
    catch {
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", "The Planning source returned invalid shortage evidence.");
    }
}
export function notFound() {
    return new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
}
