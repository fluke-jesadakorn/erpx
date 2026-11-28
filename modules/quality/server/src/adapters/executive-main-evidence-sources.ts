import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { getQualityHold } from "@erpx/quality-server/public/quality";
import { holdEvidenceSource, inspectionEvidenceSource, evidenceHead } from "./inspection-evidence-source-sql";
import { QualityInspectionRow } from '@erpx/platform-runtime-server/public/executive/evidence-contract';
export function executiveMainEvidenceSources(db: SqlDatabase) {
    return { async factsSearchQualityHoldEvidence(releases: boolean, input: {
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
            const rows = (await db.prepare(`${holdEvidenceSource(`h.id,h.id hold_id,${timeColumn} created_at,h.version reviewed_version,h.status result,
      p.name product_name,w.name warehouse_name`)}
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
            const parent = await db.prepare(evidenceHead(`i.hold_id`,` FROM inventory_quality_inspections i
    JOIN inventory_quality_holds h ON h.id=i.hold_id
    WHERE i.id=? AND h.company_id=? AND (?::text IS NULL OR h.branch_id=?)
      AND i.created_at>=? AND i.created_at<?`,''))
                .bind(recordId, actor.companyId, actor.branchId, actor.branchId, Date.parse(filters.from), Date.parse(filters.to) + 86400000)
                .first<{
                hold_id: string;
            }>();
            if (!parent)
                throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
            await getQualityHold(actor, parent.hold_id);
            const row = await db.prepare(`${inspectionEvidenceSource("i.id,i.hold_id,i.result,i.evidence,i.inspected_by,i.created_at,i.reviewed_version,\n      p.name product_name,w.name warehouse_name")}
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
