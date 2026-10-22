import { z } from "zod";
import { filtersSchema, type Filters } from "@erpx/server-runtime/public/rules/canvas-domain";
import { BusinessRuleError } from '@erpx/server-shared/index';
export const brandEvidenceMetrics = ["brand-profiles", "brand-profile-updates"] as const;
export type BrandEvidenceMetric = typeof brandEvidenceMetrics[number];
export const providerId = "brand-profile-records";
export const fields = [
    "profile.scope_id", "branch.display_name", "profile.document_mode", "profile.updated_at",
    "profile.primary_attachment_id", "profile.inverse_attachment_id",
];
export const inputSchema = z.object({
    metric: z.enum(brandEvidenceMetrics),
    filters: filtersSchema,
    query: z.string().trim().max(100).default(""),
    cursor: z.string().max(2000).optional(),
    limit: z.coerce.number().int().min(1).max(50).default(25),
}).strict();
export type BrandProfileRow = {
    id: string;
    branch_name: string;
    document_mode: "holding" | "branch" | "both";
    updated_at: number | string | bigint;
    primary_attachment_id: string | null;
    inverse_attachment_id: string | null;
};
export type Cursor = {
    branchName: string;
    id: string;
};
export function parseCursor(value?: string): Cursor | null {
    if (!value)
        return null;
    try {
        const parsed = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as {
            branchName?: unknown;
            id?: unknown;
        };
        return {
            branchName: z.string().min(1).max(200).parse(parsed.branchName),
            id: z.string().min(1).max(200).parse(parsed.id),
        };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeCursor(row: Pick<BrandProfileRow, "branch_name" | "id">) {
    return Buffer.from(JSON.stringify({ branchName: row.branch_name, id: row.id })).toString("base64url");
}
export function safeRevision(value: number | string | bigint) {
    const revision = Number(value);
    if (!Number.isSafeInteger(revision) || revision < 1)
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", "The source returned an invalid profile revision.");
    return revision;
}
export function notFound() {
    return new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
}
export function timeRange(filters: Filters) {
    return { from: Date.parse(filters.from), toExclusive: Date.parse(filters.to) + 86400000 };
}
