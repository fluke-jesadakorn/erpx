import { commercialOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as financeProjectionSources } from "../../finance/public/commercial-owner-projection-sources";
import "server-only";
import {salesFollowupMetricSource} from "./metric-purchases";
import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { inputSchema, type FollowupRow, parseFollowupCursor } from '@/server/modules/runtime/public/executive/crm-evidence-contract';
export function executiveFollowupEvidenceSources(db: SqlDatabase) {
    return { async searchRows1(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseFollowupCursor>, referenceDate: string): Promise<FollowupRow[]> {
            const purchased=await salesFollowupMetricSource(db,actor);
            const conditions = [
                "o.company_id=?", "(?::text IS NULL OR o.branch_id=?)", "(?::text IS NULL OR o.department_id=?)",
                "o.kind IN ('sales','quote')", "c.status='open'", "c.due_date BETWEEN ? AND ?", "c.due_date<?",
            ];
            const values: unknown[] = [...purchased.args,actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, input.filters.from, input.filters.to, referenceDate];
            if (input.query) {
                conditions.push("(position(lower(?) in lower(c.next_action))>0 OR position(lower(?) in lower(o.number))>0 OR position(lower(?) in lower(c.due_date))>0)");
                values.push(input.query, input.query, input.query);
            }
            if (cursor) {
                conditions.push("(c.due_date>? OR (c.due_date=? AND c.id>?))");
                values.push(cursor.dueDate, cursor.dueDate, cursor.id);
            }
            const rows = (await db.prepare(`SELECT c.id,c.order_id,c.next_action,c.due_date,c.status,c.version,o.number order_number FROM (${purchased.sql}) c
    JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=c.order_id AND o.company_id=c.company_id JOIN (${financeProjectionSources.finance_parties}) p ON p.id=o.party_id AND p.company_id=o.company_id
    WHERE ${conditions.join(" AND ")} ORDER BY c.due_date,c.id LIMIT ?`)
                .bind(...values, input.limit + 1).all<FollowupRow>()).results;
            return rows;
        } };
}
