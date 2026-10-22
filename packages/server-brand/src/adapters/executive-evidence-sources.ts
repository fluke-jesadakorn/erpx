import { commercialOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/commercial-owner-projection-sources";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { inputSchema, type BrandProfileRow, parseCursor, timeRange } from '@erpx/server-runtime/public/executive/brand-evidence-contract';
export function executiveEvidenceSources(database: SqlDatabase) {
    const db = database;
    return { async searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>): Promise<BrandProfileRow[]> {
            const conditions = [
                "p.scope_kind='branch'", "p.company_id=?", "b.kind='branch'", "b.status='active'",
                "(?::text IS NULL OR p.scope_id=?)",
            ];
            const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId];
            if (input.metric === "brand-profile-updates") {
                const range = timeRange(input.filters);
                conditions.push("p.updated_at>=?", "p.updated_at<?");
                values.push(range.from, range.toExclusive);
            }
            if (input.query) {
                conditions.push(`(position(lower(?) in lower(b.display_name))>0
      OR position(lower(?) in lower(p.document_mode))>0
      OR position(lower(?) in to_char(to_timestamp(p.updated_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'))>0)`);
                values.push(input.query, input.query, input.query);
            }
            if (cursor) {
                conditions.push("(b.display_name>? OR (b.display_name=? AND p.scope_id>?))");
                values.push(cursor.branchName, cursor.branchName, cursor.id);
            }
            const rows = (await db.prepare(`SELECT p.scope_id id,b.display_name branch_name,p.document_mode,p.updated_at,
      p.primary_attachment_id,p.inverse_attachment_id
    FROM organization_brand_profiles p
    JOIN (${organizationProjectionSources.organization_nodes}) b ON b.id=p.scope_id AND b.company_id=p.company_id
    WHERE ${conditions.join(" AND ")} ORDER BY b.display_name,p.scope_id LIMIT ?`)
                .bind(...values, input.limit + 1).all<BrandProfileRow>()).results;
            return rows;
        } };
}
