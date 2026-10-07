import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { ApiError } from "@/server/modules/runtime/public/http";
import { OrganizationNodeEvidenceRow, PendingStructureEvidenceRow } from '@/server/modules/runtime/public/executive/evidence-contract';
const organizationNodeEvidenceSource = `SELECT n.id,n.kind,n.slug,n.display_name,n.status,n.hierarchy_revision,p.display_name parent_name,
  b.display_name branch_name FROM organization_nodes n
  LEFT JOIN organization_nodes p ON p.id=n.parent_id AND p.company_id=n.company_id
  LEFT JOIN organization_nodes b ON b.id=CASE WHEN n.kind='branch' THEN n.id ELSE n.branch_id END AND b.company_id=n.company_id
  WHERE n.company_id=? AND n.kind IN ('branch','department') AND n.status='active'
    AND (?::text IS NULL OR n.id=? OR n.branch_id=?) AND (?::text IS NULL OR n.id=?)`;
const pendingStructureEvidenceSource = `SELECT c.id,c.action,c.node_kind,c.display_name,c.state,c.version,c.created_at,c.updated_at,
  COALESCE(b.display_name,'Company-wide') branch_name FROM organization_node_changes c
  LEFT JOIN organization_nodes b ON b.id=c.branch_id AND b.company_id=c.company_id
  WHERE c.company_id=? AND c.state='submitted' AND (?::text IS NULL OR c.branch_id=?)`;
export function executiveMainEvidenceSources(db: SqlDatabase) {
    return { async factsSearchOrganizationNodeEvidence(cursor: {
            name: string | undefined;
            at: number | undefined;
            id: string;
        } | null, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "active-organization-nodes";
            cursor?: string | undefined;
        }, actor: WorkContext): Promise<{
            rows: OrganizationNodeEvidenceRow[];
        }> {
            const conditions: string[] = [];
            const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
            if (input.query) {
                conditions.push("position(lower(?) in lower(concat_ws(' ',n.display_name,n.kind,n.slug,b.display_name)))>0");
                values.push(input.query);
            }
            if (cursor) {
                if (cursor.name === undefined || cursor.at !== undefined)
                    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
                conditions.push("(n.display_name>? OR (n.display_name=? AND n.id>?))");
                values.push(cursor.name, cursor.name, cursor.id);
            }
            const rows = (await db.prepare(`${organizationNodeEvidenceSource}${conditions.length ? ` AND ${conditions.join(" AND ")}` : ""}
    ORDER BY n.display_name,n.id LIMIT ?`).bind(...values, input.limit + 1).all<OrganizationNodeEvidenceRow>()).results;
            return { rows };
        }, async factsReadOrganizationNodeEvidence(actor: WorkContext, id: string): Promise<{
            row: OrganizationNodeEvidenceRow | null;
        }> {
            const values = [actor.companyId, actor.branchId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, id];
            const row = await db.prepare(`${organizationNodeEvidenceSource} AND n.id=? LIMIT 1`).bind(...values).first<OrganizationNodeEvidenceRow>();
            return { row };
        }, async factsSearchPendingStructureEvidence(cursor: {
            name: string | undefined;
            at: number | undefined;
            id: string;
        } | null, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "pending-structure-changes";
            cursor?: string | undefined;
        }, actor: WorkContext): Promise<{
            rows: PendingStructureEvidenceRow[];
        }> {
            const conditions: string[] = [];
            const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId];
            if (input.query) {
                conditions.push("position(lower(?) in lower(concat_ws(' ',c.action,c.node_kind,c.display_name,c.state,b.display_name)))>0");
                values.push(input.query);
            }
            if (cursor) {
                if (cursor.at === undefined || cursor.name !== undefined)
                    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
                conditions.push("(c.updated_at<? OR (c.updated_at=? AND c.id>?))");
                values.push(cursor.at, cursor.at, cursor.id);
            }
            const rows = (await db.prepare(`${pendingStructureEvidenceSource}${conditions.length ? ` AND ${conditions.join(" AND ")}` : ""}
    ORDER BY c.updated_at DESC,c.id ASC LIMIT ?`).bind(...values, input.limit + 1).all<PendingStructureEvidenceRow>()).results;
            return { rows };
        }, async factsReadPendingStructureEvidence(actor: WorkContext, id: string): Promise<{
            row: PendingStructureEvidenceRow | null;
        }> {
            const row = await db.prepare(`${pendingStructureEvidenceSource} AND c.id=? LIMIT 1`)
                .bind(actor.companyId, actor.branchId, actor.branchId, id).first<PendingStructureEvidenceRow>();
            return { row };
        } };
}
