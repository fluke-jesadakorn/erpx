import { z } from "zod";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import { filtersSchema } from "@erpx/server-runtime/public/rules/canvas-domain";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { posEvidenceMetrics, type PosEvidenceMetric, providerId, grossSalesFields, inputSchema, type EvidenceRow, type PosDetailRow, type PosCursor, fieldsFor, safeIntegerString, safeVersion, notFound, scopeKey, parseCursor, hrefFor } from '@erpx/server-runtime/domain/executive/pos-evidence';
import { BusinessRuleError, type Clock } from '@erpx/server-shared/index';
import type { posExecutiveEvidenceRow } from "@erpx/server-pos/public/pos";
import type { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import type { currentAuthority } from "@erpx/server-runtime/public/executive/access";
import type { metricCatalogFor } from "@erpx/server-runtime/public/executive/metrics";
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
export interface PosEvidenceRepository {
    searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>): Promise<EvidenceRow[]>;
    source0(companyId: string, userId: string, assignmentId: string, providerId: "pos-transaction-records", metric: "pos-sales" | "net-pos-sales" | "pos-payment-mix", action: "search" | "open", value: string, queryHash: string, now: number, value2: string): Promise<{
        success: boolean;
        meta?: {
            changes?: number;
            duration?: number;
            last_row_id?: number | string;
        };
    }>;
}
export interface PosEvidencePorts {
    repository: PosEvidenceRepository;
    clock: Clock;
    digest(value: string): Promise<string>;
    posExecutiveEvidenceRow(...args: Tail<Parameters<typeof posExecutiveEvidenceRow>>): ReturnType<typeof posExecutiveEvidenceRow>;
    assertProductionModuleEnabled(...args: Tail<Parameters<typeof assertProductionModuleEnabled>>): ReturnType<typeof assertProductionModuleEnabled>;
    currentAuthority(...args: Tail<Parameters<typeof currentAuthority>>): ReturnType<typeof currentAuthority>;
    metricCatalogFor(...args: Tail<Parameters<typeof metricCatalogFor>>): ReturnType<typeof metricCatalogFor>;
}
export function createPosEvidenceApplication(ports: PosEvidencePorts) {
    const r = ports.repository;
    async function requirePosEvidence(actor: WorkContext, metric: PosEvidenceMetric) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "pos.read") || !hasPermission(actor, "pos.reports") || !hasModuleAccess(actor.modules, "pos") || actor.departmentId)
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "POS evidence requires report access at Company or Branch scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "pos");
    }
    async function writeAudit(actor: WorkContext, metric: PosEvidenceMetric, action: "search" | "open", rows: {
        id: string;
        version: number;
    }[], queryHash: string) {
        if (!rows.length)
            return;
        await r.source0(actor.companyId, actor.userId, actor.assignmentId, providerId, metric, action, JSON.stringify(fieldsFor(metric)), queryHash, ports.clock.now(), JSON.stringify(rows.map((row) => ({ record_id: row.id, record_version: row.version }))));
    }
    async function searchPosEvidence(actor: WorkContext, raw: unknown) {
        const input = inputSchema.parse(raw);
        await requirePosEvidence(actor, input.metric);
        const cursor = parseCursor(input.cursor, input, actor);
        const rows = await r.searchRows0(actor, input, cursor);
        const hasMore = rows.length > input.limit, page = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId, metric: input.metric, filters: input.filters, query: input.query,
            cursor: input.cursor ?? null, scope: scopeKey(actor) }));
        await ports.currentAuthority(actor);
        await writeAudit(actor, input.metric, "search", page.map((row) => ({ id: row.id, version: safeVersion(row.revision) })), queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: providerId,
            metric: input.metric,
            fields: fieldsFor(input.metric),
            items: page.map((row) => {
                const eventDate = z.string().date().parse(row.event_date);
                const amount = safeIntegerString(row.contribution_minor, "contribution", true);
                const label = row.source_type === "sale" ? `Sale ${row.receipt_number}`
                    : row.source_type === "refund" ? `Refund · ${row.receipt_number}`
                        : `Payment · ${row.method} · ${row.receipt_number}`;
                const summary = row.source_type === "refund" ? `Accounted ${eventDate} · ${row.status}`
                    : row.source_type === "payment" ? `Business date ${eventDate} · ${row.status}`
                        : `Business date ${eventDate} · ${row.status}`;
                return { id: row.id, label, summary, amount, currency: row.currency, status: row.status,
                    version: safeVersion(row.revision), href: hrefFor(row) };
            }),
            hasMore,
            nextCursor: hasMore && page.length ? Buffer.from(JSON.stringify({ metric: input.metric, filters: input.filters, query: input.query,
                scope: scopeKey(actor), eventDate: page.at(-1)!.event_date, id: page.at(-1)!.id } satisfies PosCursor)).toString("base64url") : null,
            queryHash,
        };
    }
    async function readPosEvidence(actor: WorkContext, rawMetric: string, recordId: string, rawFilters: unknown) {
        const metric = z.enum(posEvidenceMetrics).parse(rawMetric), filters = filtersSchema.parse(rawFilters);
        await requirePosEvidence(actor, metric);
        const row = await ports.posExecutiveEvidenceRow(actor, metric, recordId, filters) as PosDetailRow | null;
        if (!row || (metric === "pos-sales" && row.source_type !== "sale")
            || (metric === "net-pos-sales" && !["sale", "refund"].includes(row.source_type))
            || (metric === "pos-payment-mix" && row.source_type !== "payment"))
            throw notFound();
        const revision = safeVersion(row.revision), saleVersion = safeVersion(row.sale_version);
        const amount = safeIntegerString(row.amount_minor, "amount");
        const contribution = safeIntegerString(row.contribution_minor, "contribution", true);
        const refunded = safeIntegerString(row.refunded_minor, "refunded amount");
        const fields = metric === "pos-sales" ? grossSalesFields
            : metric === "net-pos-sales" ? (row.source_type === "refund"
                ? ["refund.id", "refund.sale_id", "refund.branch_id", "refund.accounting_date", "refund.kind", "refund.state", "refund.amount_minor", "refund.version", "sale.number", "sale.currency", "sale.version"]
                : ["sale.id", "sale.number", "sale.business_date", "sale.currency", "sale.branch_id", "sale.state", "sale.total_minor", "sale.version"])
                : ["payment.id", "payment.sale_id", "payment.method", "payment.state", "payment.amount_minor", "payment.version", "sale.number", "sale.business_date", "sale.currency", "sale.branch_id", "sale.state", "sale.version", "refund.kind", "refund_payment.amount_minor", "refund_payment.state"];
        const record = {
            id: recordId,
            source_type: row.source_type,
            receipt_number: row.receipt_number,
            business_date: row.business_date,
            accounting_date: row.accounting_date ?? null,
            currency: row.currency,
            status: row.state,
            sale_status: row.sale_state ?? row.state,
            method: row.method ?? null,
            amount_minor: amount,
            contribution_minor: contribution,
            refunded_minor: refunded,
            version: revision,
            sale_version: saleVersion,
            href: hrefFor(row),
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId, metric, filters, scope: scopeKey(actor), recordId }));
        await ports.currentAuthority(actor);
        await writeAudit(actor, metric, "open", [{ id: recordId, version: revision }], queryHash);
        await ports.currentAuthority(actor);
        return { provider: providerId, metric, fields, record, queryHash };
    }
    return { searchPosEvidence, readPosEvidence };
}
