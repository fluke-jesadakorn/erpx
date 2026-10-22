import { z } from "zod";
import { filtersSchema, type Filters } from "@erpx/server-runtime/public/rules/canvas-domain";
import { BusinessRuleError } from '@erpx/server-shared/index';
export const talentEvidenceMetrics = ["open-requisitions", "application-stage-counts"] as const;
export type TalentEvidenceMetric = typeof talentEvidenceMetrics[number];
export const providerId = "talent-recruiting-records";
export const requisitionFields = [
    "requisition.id", "requisition.job_title", "requisition.target_start_date", "requisition.status",
    "requisition.version", "requisition.updated_at", "requisition.branch_id", "requisition.department_id",
];
export const applicationFields = [
    "application.id", "application.status", "application.version", "application.updated_at",
    "requisition.id", "requisition.job_title", "requisition.branch_id", "requisition.department_id",
];
export const inputSchema = z.object({
    metric: z.enum(talentEvidenceMetrics),
    filters: filtersSchema,
    query: z.string().trim().max(100).default(""),
    cursor: z.string().max(2000).optional(),
    limit: z.coerce.number().int().min(1).max(50).default(25),
}).strict();
export type RequisitionCursor = {
    jobTitle: string;
    id: string;
};
export type ApplicationCursor = {
    updatedAt: number;
    id: string;
};
export type TalentEvidenceRow = {
    id: string;
    requisition_id?: string;
    job_title: string;
    target_start_date?: string;
    status: string;
    version: number | string | bigint;
    updated_at: number | string | bigint;
    branch_id: string;
    department_id: string | null;
};
export function parseCursor(value: string | undefined, metric: TalentEvidenceMetric) {
    if (!value)
        return null;
    try {
        const parsed = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as Record<string, unknown>;
        const id = z.string().min(1).max(200).parse(parsed.id);
        return metric === "open-requisitions"
            ? { jobTitle: z.string().min(1).max(160).parse(parsed.jobTitle), id } satisfies RequisitionCursor
            : { updatedAt: z.number().int().positive().parse(parsed.updatedAt), id } satisfies ApplicationCursor;
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function safeVersion(value: number | string | bigint) {
    const version = Number(value);
    if (!Number.isSafeInteger(version) || version < 1)
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", "The source returned an invalid Talent revision.");
    return version;
}
export function safeTimestamp(value: number | string | bigint) {
    const timestamp = Number(value);
    if (!Number.isSafeInteger(timestamp) || timestamp < 1)
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", "The source returned an invalid Talent timestamp.");
    return timestamp;
}
export function utcRange(filters: Filters) {
    return { from: Date.parse(filters.from), toExclusive: Date.parse(filters.to) + 86400000 };
}
export function notFound() {
    return new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
}
