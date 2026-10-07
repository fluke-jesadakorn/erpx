import { z } from "zod";
import { filtersSchema } from "@/server/modules/runtime/public/rules/canvas-domain";
import { BusinessRuleError } from '../../../shared';
export const crmEvidenceMetrics = ["opportunity-pipeline", "overdue-followups"] as const;
export type CrmEvidenceMetric = typeof crmEvidenceMetrics[number];
export const opportunityProviderId = "crm-opportunities";
export const opportunityMetricId = "opportunity-pipeline" as const;
export const followupProviderId = "crm-customer-followups";
export const followupMetricId = "overdue-followups" as const;
export const openStages = ["discovery", "proposal", "negotiation"] as const;
export const fields = [
    "lead.title", "lead.status", "lead.version", "opportunity.stage", "opportunity.expected_minor",
    "opportunity.currency", "opportunity.close_date", "opportunity.version",
];
export const followupFields = ["followup.next_action", "followup.status", "followup.due_date", "followup.version", "order.id", "order.number"];
export const inputSchema = z.object({
    metric: z.enum(crmEvidenceMetrics),
    filters: filtersSchema,
    query: z.string().trim().max(100).default(""),
    cursor: z.string().max(2000).optional(),
    limit: z.coerce.number().int().min(1).max(50).default(25),
}).strict();
export type Cursor = {
    closeDate: string;
    id: string;
};
export type OpportunityRow = {
    id: string;
    lead_id: string;
    title: string;
    lead_status: string;
    lead_version: number | string | bigint;
    stage: typeof openStages[number];
    expected_minor: number | string | bigint;
    currency: string;
    close_date: string;
    version: number | string | bigint;
};
export type FollowupRow = {
    id: string;
    order_id: string;
    next_action: string;
    due_date: string;
    status: "open";
    version: number | string | bigint;
    order_number: string;
};
export function parseCursor(value?: string): Cursor | null {
    if (!value)
        return null;
    try {
        const parsed = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as {
            closeDate?: unknown;
            id?: unknown;
        };
        if (typeof parsed.closeDate !== "string")
            throw new Error();
        return { closeDate: z.iso.date().parse(parsed.closeDate), id: z.string().min(1).max(200).parse(parsed.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeCursor(row: Pick<OpportunityRow, "close_date" | "id">) {
    return Buffer.from(JSON.stringify({ closeDate: row.close_date, id: row.id })).toString("base64url");
}
export function parseFollowupCursor(value?: string) {
    if (!value)
        return null;
    try {
        const parsed = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as {
            dueDate?: unknown;
            id?: unknown;
        };
        if (typeof parsed.dueDate !== "string")
            throw new Error();
        return { dueDate: z.iso.date().parse(parsed.dueDate), id: z.string().min(1).max(200).parse(parsed.id) };
    }
    catch {
        throw new BusinessRuleError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
    }
}
export function encodeFollowupCursor(row: Pick<FollowupRow, "due_date" | "id">) {
    return Buffer.from(JSON.stringify({ dueDate: row.due_date, id: row.id })).toString("base64url");
}
export function exactVersion(value: number | string | bigint, field: string) {
    const result = Number(value);
    if (!Number.isSafeInteger(result) || result < 1)
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", `The source returned an invalid ${field}.`);
    return result;
}
export function exactMinor(value: number | string | bigint) {
    const result = String(value);
    if (!/^(0|[1-9]\d*)$/.test(result))
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", "The source returned an invalid opportunity amount.");
    return result;
}
export function notFound() {
    return new BusinessRuleError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
}
export function auditRecords(rows: OpportunityRow[]) {
    return rows.flatMap((row) => [
        { id: row.id, version: exactVersion(row.version, "opportunity version") },
        { id: row.lead_id, version: exactVersion(row.lead_version, "lead version") },
    ]);
}
