import { platformOwnerProjectionSources as commercePlatformSources } from "../../commerce/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as financePlatformSources } from "../../finance/public/platform-owner-projection-sources";
import "server-only";
import { type Filters } from "@/lib/shared/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { SupplierBillEvidenceRow } from '@/server/modules/runtime/public/executive/evidence-contract';
async function supplierBillEvidenceRows(db: SqlDatabase, actor: WorkContext, filters: Filters, query: string, cursor?: {
    invoiceDate: string;
    id: string;
}, limit = 25, recordId?: string) {
    const scoped = supplierBillConditions(actor, filters, query, cursor, recordId);
    return (await db.prepare(`SELECT latest.id,latest.reference,latest.invoice_date,latest.due_date,latest.currency,
      (latest.subtotal_minor::numeric+latest.tax_minor::numeric)::text total_minor,latest.status,latest.version,
      p.name supplier_name,o.number order_number,latest.expected_subtotal_minor::text expected_subtotal_minor,
      latest.subtotal_minor::text subtotal_minor,latest.tax_minor::text tax_minor,latest.quantity_matches
    FROM procurement_bill_matches latest
    JOIN (${financePlatformSources.finance_parties}) p ON p.id=latest.supplier_id AND p.company_id=latest.company_id
    JOIN (${commercePlatformSources.commerce_orders}) o ON o.id=latest.order_id AND o.company_id=latest.company_id
    WHERE ${scoped.conditions.join(" AND ")}
    ORDER BY latest.invoice_date DESC,latest.id DESC LIMIT ?`).bind(...scoped.values, recordId ? 1 : limit + 1).all<SupplierBillEvidenceRow>()).results;
}
function supplierBillConditions(actor: WorkContext, filters: Filters, query: string, cursor?: {
    invoiceDate: string;
    id: string;
}, recordId?: string) {
    const conditions = [
        "latest.company_id=?",
        "(?::text IS NULL OR latest.branch_id=?)",
        "(?::text IS NULL OR latest.department_id=?)",
        "latest.invoice_date BETWEEN ? AND ?",
        "latest.currency=?",
        `((latest.status='review' AND (NOT latest.quantity_matches OR latest.subtotal_minor<>latest.expected_subtotal_minor))
      OR (latest.status='returned' AND NOT EXISTS(
        SELECT 1 FROM procurement_bill_match_receipts returned_receipt
        JOIN procurement_bill_match_receipts replacement_receipt
          ON replacement_receipt.fulfillment_id=returned_receipt.fulfillment_id AND replacement_receipt.active=TRUE
        WHERE returned_receipt.match_id=latest.id AND replacement_receipt.match_id<>latest.id
      )))`,
    ];
    const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, filters.from, filters.to, filters.currency];
    if (query) {
        conditions.push("position(lower(?) in lower(latest.reference))>0");
        values.push(query);
    }
    if (cursor) {
        conditions.push("(latest.invoice_date,latest.id)<(?,?)");
        values.push(cursor.invoiceDate, cursor.id);
    }
    if (recordId) {
        conditions.push("latest.id=?");
        values.push(recordId);
    }
    return { conditions, values };
}
export function executiveMainEvidenceSources(db: SqlDatabase) { return { async factsSearchSupplierBillEvidence(actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "invoice-match-exceptions";
        cursor?: string | undefined;
    }, cursor: {
        invoiceDate: string;
        id: string;
    } | null): Promise<{
        rows: SupplierBillEvidenceRow[];
    }> { const rows = await supplierBillEvidenceRows(db, actor, input.filters, input.query, cursor ?? undefined, input.limit); return { rows }; }, async factsReadSupplierBillEvidence(actor: WorkContext, filters: {
        from: string;
        to: string;
        currency: string;
    }, recordId: string): Promise<{
        latest: SupplierBillEvidenceRow;
    }> { const [latest] = await supplierBillEvidenceRows(db, actor, filters, "", undefined, 1, recordId); return { latest }; } }; }
