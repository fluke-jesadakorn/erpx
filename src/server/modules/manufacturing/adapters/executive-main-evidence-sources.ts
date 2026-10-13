import { inventoryExecutiveOwnerProjections as commerceProjectionSources } from "../../commerce/public/inventory-executive-owner-projections";
import { inventoryExecutiveOwnerProjections as financeProjectionSources } from "../../finance/public/inventory-executive-owner-projections";
import "server-only";
import { type Filters } from "@/lib/shared/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { ManufacturingCostEvidenceRow } from '@/server/modules/runtime/public/executive/evidence-contract';
async function manufacturingCostEvidenceRows(db: SqlDatabase, actor: WorkContext, filters: Filters, query: string, cursor?: {
    postedAt: number;
    id: string;
}, limit = 25, recordId?: string) {
    const scoped = manufacturingCostConditions(actor, filters, query, cursor, recordId);
    return (await db.prepare(`SELECT m.id,m.order_id,m.movement_kind,m.amount_minor::text amount_minor,j.posted_at,m.version,m.journal_id,
      j.status journal_status,j.currency,o.status order_status,o.version order_version,p.name product_name,w.name warehouse_name
    FROM manufacturing_cost_movements m
    JOIN manufacturing_orders o ON o.id=m.order_id AND o.company_id=m.company_id
    JOIN manufacturing_specifications s ON s.id=o.specification_id
    JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=s.output_product_id AND p.company_id=o.company_id
    JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=o.warehouse_id AND w.company_id=o.company_id
    JOIN (${financeProjectionSources.finance_journals}) j ON j.id=m.journal_id AND j.company_id=m.company_id
    JOIN (${commerceProjectionSources.commerce_settings}) cs ON cs.company_id=o.company_id
    WHERE ${scoped.conditions.join(" AND ")}
    ORDER BY j.posted_at DESC,m.id DESC LIMIT ?`).bind(...scoped.values, recordId ? 1 : limit + 1).all<ManufacturingCostEvidenceRow>()).results;
}
function manufacturingCostConditions(actor: WorkContext, filters: Filters, query: string, cursor?: {
    postedAt: number;
    id: string;
}, recordId?: string) {
    const conditions = [
        "m.company_id=?",
        "(?::text IS NULL OR o.branch_id=?)",
        "o.status='completed'",
        "m.movement_kind IN ('material_issue','material_return','labor')",
        "j.status='posted'",
        "j.currency=?",
        "j.posted_at>=?",
        "j.posted_at<?",
        "cs.currency=j.currency",
    ];
    const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, filters.currency, Date.parse(filters.from), Date.parse(filters.to) + 86400000];
    if (query) {
        conditions.push("(position(lower(?) in lower(m.id))>0 OR position(lower(?) in lower(m.order_id))>0 OR position(lower(?) in lower(m.movement_kind))>0 OR position(lower(?) in lower(p.name))>0)");
        values.push(query, query, query, query);
    }
    if (cursor) {
        conditions.push("(j.posted_at,m.id)<(?,?)");
        values.push(cursor.postedAt, cursor.id);
    }
    if (recordId) {
        conditions.push("m.id=?");
        values.push(recordId);
    }
    return { conditions, values };
}
export function executiveMainEvidenceSources(db: SqlDatabase) { return { async factsSearchManufacturingCostEvidence(actor: WorkContext, input: {
        filters: {
            from: string;
            to: string;
            currency: string;
        };
        query: string;
        limit: number;
        metric: "production-cost";
        cursor?: string | undefined;
    }, cursor: {
        postedAt: number;
        id: string;
    } | null): Promise<{
        rows: ManufacturingCostEvidenceRow[];
    }> { const rows = await manufacturingCostEvidenceRows(db, actor, input.filters, input.query, cursor ?? undefined, input.limit); return { rows }; }, async factsReadManufacturingCostEvidence(actor: WorkContext, filters: {
        from: string;
        to: string;
        currency: string;
    }, recordId: string): Promise<{
        row: ManufacturingCostEvidenceRow;
    }> { const [row] = await manufacturingCostEvidenceRows(db, actor, filters, "", undefined, 1, recordId); return { row }; } }; }
