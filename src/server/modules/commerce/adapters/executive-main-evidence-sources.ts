import "server-only";
import { type Filters } from "@/lib/shared/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { ApiError } from "@/server/modules/runtime/public/http";
import { SalesMetricId, SalesOrderEvidenceRow, SalesEvidenceCursor, SalesFulfillmentEvidenceRow } from '@/server/modules/runtime/public/executive/evidence-contract';
async function salesOrderEvidenceRows(db: SqlDatabase, actor: WorkContext, metric: SalesMetricId, filters: Filters, query: string, cursor?: SalesEvidenceCursor, limit = 25, recordId?: string) {
    const scoped = salesOrderConditions(actor, metric, filters, query, cursor, recordId);
    return (await db.prepare(`SELECT o.id,o.number,o.order_date,o.updated_at,o.currency,o.subtotal_minor::text subtotal_minor,
      o.tax_minor::text tax_minor,(o.subtotal_minor::numeric+o.tax_minor::numeric)::text total_minor,o.state status,o.version,
      (o.approved_by IS NOT NULL) prior_approval
    FROM commerce_orders o WHERE ${scoped.conditions.join(" AND ")}
    ORDER BY ${metric === "sales-orders" ? "o.order_date DESC,o.id DESC" : "o.updated_at DESC,o.id DESC"} LIMIT ?`).bind(...scoped.values, recordId ? 1 : limit + 1).all<SalesOrderEvidenceRow>()).results;
}
function salesOrderConditions(actor: WorkContext, metric: SalesMetricId, filters: Filters, query: string, cursor?: SalesEvidenceCursor, recordId?: string) {
    const conditions = [
        "o.company_id=?",
        "(?::text IS NULL OR o.branch_id=?)",
        "(?::text IS NULL OR o.department_id=?)",
        "o.kind='sales'",
    ];
    const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
    if (metric === "sales-orders") {
        conditions.push("o.state IN ('confirmed','partially_fulfilled','fulfilled')", "o.order_date BETWEEN ? AND ?", "o.currency=?");
        values.push(filters.from, filters.to, filters.currency);
    }
    else {
        conditions.push("o.state='cancelled'", "o.updated_at>=?", "o.updated_at<?");
        values.push(Date.parse(filters.from), Date.parse(filters.to) + 86400000);
        if (metric === "sales-cancellation-value") {
            conditions.push("o.approved_by IS NOT NULL", "o.currency=?");
            values.push(filters.currency);
        }
    }
    if (query) {
        conditions.push("position(lower(?) in lower(o.number))>0");
        values.push(query);
    }
    if (cursor) {
        if (metric === "sales-orders" && "orderDate" in cursor) {
            conditions.push("(o.order_date,o.id)<(?,?)");
            values.push(cursor.orderDate, cursor.id);
        }
        else if (metric !== "sales-orders" && "cancelledAt" in cursor) {
            conditions.push("(o.updated_at,o.id)<(?,?)");
            values.push(cursor.cancelledAt, cursor.id);
        }
        else {
            throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
        }
    }
    if (recordId) {
        conditions.push("o.id=?");
        values.push(recordId);
    }
    return { conditions, values };
}
async function salesFulfillmentEvidenceRows(db: SqlDatabase, actor: WorkContext, filters: Filters, query: string, cursor?: {
    orderDate: string;
    id: string;
}, limit = 25, recordId?: string) {
    const scoped = salesFulfillmentConditions(actor, filters, query, cursor, recordId);
    return (await db.prepare(`SELECT o.id,o.number,o.order_date,o.state status,o.version
    FROM commerce_orders o WHERE ${scoped.conditions.join(" AND ")}
    ORDER BY o.order_date DESC,o.id DESC LIMIT ?`).bind(...scoped.values, recordId ? 1 : limit + 1).all<SalesFulfillmentEvidenceRow>()).results;
}
function salesFulfillmentConditions(actor: WorkContext, filters: Filters, query: string, cursor?: {
    orderDate: string;
    id: string;
}, recordId?: string) {
    const conditions = [
        "o.company_id=?",
        "(?::text IS NULL OR o.branch_id=?)",
        "(?::text IS NULL OR o.department_id=?)",
        "o.kind='sales'",
        "o.state IN ('confirmed','partially_fulfilled','fulfilled')",
        "o.order_date BETWEEN ? AND ?",
        "EXISTS(SELECT 1 FROM commerce_order_lines l WHERE l.order_id=o.id)",
    ];
    const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, filters.from, filters.to];
    if (query) {
        conditions.push("position(lower(?) in lower(o.number))>0");
        values.push(query);
    }
    if (cursor) {
        conditions.push("(o.order_date,o.id)<(?,?)");
        values.push(cursor.orderDate, cursor.id);
    }
    if (recordId) {
        conditions.push("o.id=?");
        values.push(recordId);
    }
    return { conditions, values };
}
export function executiveMainEvidenceSources(db: SqlDatabase) { return { async factsSearchSalesOrderEvidence(actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "sales-orders" | "sales-cancellations" | "sales-cancellation-value";
        cursor?: string | undefined;
    }, cursor: SalesEvidenceCursor | null): Promise<{
        rows: SalesOrderEvidenceRow[];
    }> { const rows = await salesOrderEvidenceRows(db, actor, input.metric, input.filters, input.query, cursor ?? undefined, input.limit); return { rows }; }, async factsReadSalesOrderEvidence(actor: WorkContext, metric: "sales-orders" | "sales-cancellations" | "sales-cancellation-value", filters: {
        from: string;
        to: string;
        currency: string;
    }, recordId: string): Promise<{
        row: SalesOrderEvidenceRow;
    }> { const [row] = await salesOrderEvidenceRows(db, actor, metric, filters, "", undefined, 1, recordId); return { row }; }, async factsSearchSalesFulfillmentEvidence(actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "sales-fulfillment-rate";
        cursor?: string | undefined;
    }, cursor: {
        orderDate: string;
        id: string;
    } | null): Promise<{
        rows: SalesFulfillmentEvidenceRow[];
    }> { const rows = await salesFulfillmentEvidenceRows(db, actor, input.filters, input.query, cursor ?? undefined, input.limit); return { rows }; }, async factsReadSalesFulfillmentEvidence(actor: WorkContext, filters: {
        from: string;
        to: string;
        currency: string;
    }, recordId: string): Promise<{
        row: SalesFulfillmentEvidenceRow;
    }> { const [row] = await salesFulfillmentEvidenceRows(db, actor, filters, "", undefined, 1, recordId); return { row }; } }; }
