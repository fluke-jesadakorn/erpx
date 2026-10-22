import { z } from "zod";
import { filtersSchema, type Filters } from "@erpx/server-runtime/public/rules/canvas-domain";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { BusinessRuleError } from '@erpx/server-shared/index';
export const posEvidenceMetrics = ["pos-sales", "net-pos-sales", "pos-payment-mix"] as const;
export type PosEvidenceMetric = typeof posEvidenceMetrics[number];
export const providerId = "pos-transaction-records";
export const grossSalesFields = ["sale.id", "sale.number", "sale.business_date", "sale.currency", "sale.branch_id", "sale.state", "sale.total_minor", "sale.version"];
export const netSalesFields = [
    "sale.id", "sale.number", "sale.business_date", "sale.currency", "sale.branch_id", "sale.state", "sale.total_minor", "sale.version",
    "refund.id", "refund.sale_id", "refund.branch_id", "refund.accounting_date", "refund.kind", "refund.state", "refund.amount_minor", "refund.version",
];
export const paymentMixFields = [
    "sale.id", "sale.number", "sale.business_date", "sale.currency", "sale.branch_id", "sale.state", "sale.version",
    "payment.id", "payment.method", "payment.state", "payment.amount_minor", "payment.version",
    "refund.kind", "refund_payment.amount_minor", "refund_payment.state",
];
export const inputSchema = z.object({
    metric: z.enum(posEvidenceMetrics),
    filters: filtersSchema,
    query: z.string().trim().max(100).default(""),
    cursor: z.string().max(2000).optional(),
    limit: z.coerce.number().int().min(1).max(50).default(25),
}).strict();
export type EvidenceRow = {
    id: string;
    source_id: string;
    source_type: "sale" | "refund" | "payment";
    sale_id: string;
    event_date: string;
    receipt_number: string;
    status: string;
    sale_status: string;
    currency: string;
    method: string | null;
    amount_minor: string | number | bigint;
    contribution_minor: string | number | bigint;
    refunded_minor: string | number | bigint;
    revision: string | number | bigint;
    sale_version: string | number | bigint;
};
export type PosDetailRow = {
    source_id: string;
    sale_id: string;
    source_type: "sale" | "refund" | "payment";
    receipt_number: string;
    business_date: string;
    accounting_date: string | null;
    currency: string;
    state: string;
    sale_state?: string;
    method?: string;
    amount_minor: string | number | bigint;
    contribution_minor: string | number | bigint;
    refunded_minor: string | number | bigint;
    revision: string | number | bigint;
    sale_version: string | number | bigint;
};
export type PosCursor = {
    metric: PosEvidenceMetric;
    filters: Filters;
    query: string;
    scope: string[];
    eventDate: string;
    id: string;
};
export function fieldsFor(metric: PosEvidenceMetric) {
    return metric === "pos-sales" ? grossSalesFields : metric === "net-pos-sales" ? netSalesFields : paymentMixFields;
}
export function safeIntegerString(value: string | number | bigint, name: string, allowNegative = false) {
    const text = String(value);
    if (!(allowNegative ? /^-?(0|[1-9][0-9]*)$/ : /^(0|[1-9][0-9]*)$/).test(text))
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", `The POS source returned an invalid ${name}.`);
    return text;
}
export function safeVersion(value: string | number | bigint) {
    const version = Number(value);
    if (!Number.isSafeInteger(version) || version < 1)
        throw new BusinessRuleError(500, "EVIDENCE_VALUE_INVALID", "The POS source returned an invalid record revision.");
    return version;
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
        const cursor: PosCursor = {
            metric: z.enum(posEvidenceMetrics).parse(parsed.metric),
            filters: filtersSchema.parse(parsed.filters),
            query: z.string().max(100).parse(parsed.query),
            scope: z.array(z.string()).max(4).parse(parsed.scope),
            eventDate: z.string().date().parse(parsed.eventDate),
            id: z.string().min(1).max(240).parse(parsed.id),
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
export function hrefFor(row: Pick<EvidenceRow, "sale_id">) {
    return `/pos?tab=history&sale=${encodeURIComponent(row.sale_id)}`;
}
