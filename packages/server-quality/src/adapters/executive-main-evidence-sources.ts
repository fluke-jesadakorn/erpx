import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/commercial-owner-projection-sources";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import { getQualityHold } from "@erpx/server-quality/public/quality";
import { QualityInspectionRow } from '@erpx/server-runtime/public/executive/evidence-contract';
export function executiveMainEvidenceSources(db: SqlDatabase) {
    return { async factsSearchQualityInspectionEvidence(input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "quality-holds" | "quality-releases" | "inspection-pass-rate";
            cursor?: string | undefined;
        }, actor: WorkContext, cursor: {
            createdAt: number;
            id: string;
        } | null): Promise<{
            rows: QualityInspectionRow[];
        }> {
            const conditions = [
                "h.company_id=?",
                "(?::text IS NULL OR h.branch_id=?)",
                "i.created_at>=?",
                "i.created_at<?",
            ];
            const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, Date.parse(input.filters.from), Date.parse(input.filters.to) + 86400000];
            if (input.query) {
                conditions.push("(position(lower(?) in lower(p.name))>0 OR position(lower(?) in lower(i.result))>0)");
                values.push(input.query, input.query);
            }
            if (cursor) {
                conditions.push("(i.created_at,i.id)<(?,?)");
                values.push(cursor.createdAt, cursor.id);
            }
            const rows = (await db.prepare(`SELECT i.id,i.hold_id,i.result,i.created_at,i.reviewed_version,p.name product_name,w.name warehouse_name
    FROM inventory_quality_inspections i
    JOIN inventory_quality_holds h ON h.id=i.hold_id
    JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=h.product_id AND p.company_id=h.company_id
    JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=h.warehouse_id AND w.company_id=h.company_id
    WHERE ${conditions.join(" AND ")}
    ORDER BY i.created_at DESC,i.id DESC LIMIT ?`).bind(...values, input.limit + 1).all<QualityInspectionRow>()).results;
            return { rows };
        }, async factsSearchQualityHoldEvidence(releases: boolean, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "quality-holds" | "quality-releases" | "inspection-pass-rate";
            cursor?: string | undefined;
        }, actor: WorkContext, cursor: {
            createdAt: number;
            id: string;
        } | null): Promise<{
            rows: QualityInspectionRow[];
        }> {
            const timeColumn = releases ? "h.released_at" : "h.updated_at";
            const conditions = ["h.company_id=?", "(?::text IS NULL OR h.branch_id=?)", releases ? "h.status='released'" : "h.status NOT IN ('released','cancelled','disposed')"];
            const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId];
            if (releases) {
                conditions.push("h.released_at>=?", "h.released_at<?");
                values.push(Date.parse(input.filters.from), Date.parse(input.filters.to) + 86400000);
            }
            if (input.query) {
                conditions.push("(position(lower(?) in lower(p.name))>0 OR position(lower(?) in lower(h.status))>0)");
                values.push(input.query, input.query);
            }
            if (cursor) {
                conditions.push(`(${timeColumn},h.id)<(?,?)`);
                values.push(cursor.createdAt, cursor.id);
            }
            const rows = (await db.prepare(`SELECT h.id,h.id hold_id,${timeColumn} created_at,h.version reviewed_version,h.status result,
      p.name product_name,w.name warehouse_name
    FROM inventory_quality_holds h
    JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=h.product_id AND p.company_id=h.company_id
    JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=h.warehouse_id AND w.company_id=h.company_id
    WHERE ${conditions.join(" AND ")}
    ORDER BY ${timeColumn} DESC,h.id DESC LIMIT ?`).bind(...values, input.limit + 1).all<QualityInspectionRow>()).results;
            return { rows };
        }, async factsReadQualityInspectionEvidence(recordId: string, actor: WorkContext, filters: {
            from: string;
            to: string;
            currency: string;
        }): Promise<{
            row: (QualityInspectionRow & {
                evidence: string;
                inspected_by: string;
            }) | null;
        }> {
            const parent = await db.prepare(`SELECT i.hold_id FROM inventory_quality_inspections i
    JOIN inventory_quality_holds h ON h.id=i.hold_id
    WHERE i.id=? AND h.company_id=? AND (?::text IS NULL OR h.branch_id=?)
      AND i.created_at>=? AND i.created_at<?`)
                .bind(recordId, actor.companyId, actor.branchId, actor.branchId, Date.parse(filters.from), Date.parse(filters.to) + 86400000)
                .first<{
                hold_id: string;
            }>();
            if (!parent)
                throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
            await getQualityHold(actor, parent.hold_id);
            const row = await db.prepare(`SELECT i.id,i.hold_id,i.result,i.evidence,i.inspected_by,i.created_at,i.reviewed_version,
      p.name product_name,w.name warehouse_name
    FROM inventory_quality_inspections i
    JOIN inventory_quality_holds h ON h.id=i.hold_id
    JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=h.product_id AND p.company_id=h.company_id
    JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=h.warehouse_id AND w.company_id=h.company_id
    WHERE i.id=? AND i.hold_id=? AND h.company_id=? AND (?::text IS NULL OR h.branch_id=?)
      AND i.created_at>=? AND i.created_at<?`)
                .bind(recordId, parent.hold_id, actor.companyId, actor.branchId, actor.branchId, Date.parse(filters.from), Date.parse(filters.to) + 86400000)
                .first<QualityInspectionRow & {
                evidence: string;
                inspected_by: string;
            }>();
            return { row };
        } };
}
