import "server-only";
import {crmMetricSource} from "./metric-purchases";
import { z } from "zod";
import { type Filters } from "@/lib/shared/executive/canvas";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { inputSchema, type OpportunityRow, parseCursor } from '@/server/modules/runtime/public/executive/crm-evidence-contract';
function scopedConditions(actor: WorkContext, filters: Filters) {
    return {
        sql: [
            "l.company_id=?", "(?::text IS NULL OR l.branch_id=?)", "(?::text IS NULL OR l.department_id=?)",
            "v.currency=?", "v.close_date BETWEEN ? AND ?", "v.stage IN ('discovery','proposal','negotiation')",
        ],
        values: [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, filters.currency, filters.from, filters.to] as unknown[],
    };
}
export function executiveEvidenceSources(database: SqlDatabase) {
    const db = database;
    return { async searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>): Promise<OpportunityRow[]> {
            const purchased=await crmMetricSource(db,actor,"opportunity-pipeline"),scope = scopedConditions(actor, input.filters);
            const conditions = [...scope.sql];
            const values = [...purchased.args,...scope.values];
            if (input.query) {
                conditions.push("(position(lower(?) in lower(l.title))>0 OR position(lower(?) in lower(v.stage))>0 OR position(lower(?) in lower(v.close_date))>0)");
                values.push(input.query, input.query, input.query);
            }
            if (cursor) {
                conditions.push("(v.close_date>? OR (v.close_date=? AND v.id>?))");
                values.push(cursor.closeDate, cursor.closeDate, cursor.id);
            }
            const rows = (await db.prepare(`SELECT v.id,v.lead_id,l.title,l.status lead_status,l.version lead_version,
      v.stage,v.expected_minor,v.currency,v.close_date,v.version
    FROM crm_opportunities v JOIN (${purchased.sql}) l ON l.id=v.lead_id
    WHERE ${conditions.join(" AND ")} ORDER BY v.close_date,v.id LIMIT ?`)
                .bind(...values, input.limit + 1).all<OpportunityRow>()).results;
            return rows;
        } };
}
