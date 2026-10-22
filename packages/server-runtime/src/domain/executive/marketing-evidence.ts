import { z } from "zod";
import { filtersSchema, type Filters } from "@erpx/server-runtime/public/rules/canvas-domain";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { BusinessRuleError } from '@erpx/server-shared/index';
export const marketingEvidenceMetrics = ["marketing-lead-candidates", "marketing-engagement-events"] as const;
export type MarketingEvidenceMetric = typeof marketingEvidenceMetrics[number];
export const providerId = "marketing-candidate-engagement";
export const candidateFields = [
    "candidate.id", "candidate.contact_name", "candidate.form_id", "candidate.form_version", "candidate.status",
    "candidate.version", "candidate.created_at", "candidate.updated_at", "candidate.branch_id", "candidate.department_id", "marketing_assets.name",
];
export const eventFields = [
    "event.id", "event.candidate_id", "event.event_type", "event.channel", "event.version", "event.created_at",
    "candidate.contact_name", "candidate.status", "candidate.version", "candidate.branch_id", "candidate.department_id", "marketing_assets.name",
];
export const inputSchema = z.object({
    metric: z.enum(marketingEvidenceMetrics),
    filters: filtersSchema,
    query: z.string().trim().max(100).default(""),
    cursor: z.string().max(2000).optional(),
    limit: z.coerce.number().int().min(1).max(50).default(25),
}).strict();
export type CandidateCursor = {
    metric: "marketing-lead-candidates";
    query: string;
    filters: Filters;
    scope: string[];
    name: string;
    id: string;
};
export type EventCursor = {
    metric: "marketing-engagement-events";
    query: string;
    filters: Filters;
    scope: string[];
    createdAt: number;
    id: string;
};
export type MarketingEvidenceRow = {
    id: string;
    candidate_id?: string;
    contact_name: string;
    form_name: string;
    status?: string;
    candidate_status?: string;
    event_type?: string;
    channel?: string;
    form_version?: number | string | bigint;
    version?: number | string | bigint;
    event_version?: number | string | bigint;
    candidate_version?: number | string | bigint;
    created_at: number | string | bigint;
    updated_at?: number | string | bigint;
    branch_id: string;
    department_id: string | null;
};
export function fieldsFor(metric: MarketingEvidenceMetric) {
    return metric === "marketing-lead-candidates" ? candidateFields : eventFields;
}
export function safeRevision(value: number | string | bigint | undefined) {
    const revision = Number(value);
    if (!Number.isSafeInteger(revision) || revision < 1)
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", "The Marketing source returned an invalid record revision.");
    return revision;
}
export function safeTimestamp(value: number | string | bigint | undefined) {
    const timestamp = Number(value);
    if (!Number.isSafeInteger(timestamp) || timestamp < 1)
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", "The Marketing source returned an invalid event time.");
    return timestamp;
}
export function notFound() {
    return new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
}
export function scopeKey(actor: WorkContext) {
    return [actor.companyId, actor.assignmentId, actor.branchId ?? "", actor.departmentId ?? ""];
}
export function parseCursor(value: string | undefined, input: z.infer<typeof inputSchema>, actor: WorkContext) {
    if (!value)
        return null;
    try {
        const parsed = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as Record<string, unknown>;
        const common = {
            metric: z.enum(marketingEvidenceMetrics).parse(parsed.metric),
            query: z.string().max(100).parse(parsed.query),
            filters: filtersSchema.parse(parsed.filters),
            scope: z.array(z.string()).max(4).parse(parsed.scope),
            id: z.string().min(1).max(200).parse(parsed.id),
        };
        if (common.metric !== input.metric || common.query !== input.query || JSON.stringify(common.filters) !== JSON.stringify(input.filters)
            || JSON.stringify(common.scope) !== JSON.stringify(scopeKey(actor)))
            throw new Error();
        if (input.metric === "marketing-lead-candidates" && common.metric === input.metric)
            return { ...common, metric: input.metric, name: z.string().min(1).max(160).parse(parsed.name) } satisfies CandidateCursor;
        if (input.metric === "marketing-engagement-events" && common.metric === input.metric)
            return { ...common, metric: input.metric, createdAt: z.number().int().positive().parse(parsed.createdAt) } satisfies EventCursor;
        throw new Error();
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function timeRange(filters: Filters) {
    return { from: Date.parse(filters.from), toExclusive: Date.parse(filters.to) + 86400000 };
}
