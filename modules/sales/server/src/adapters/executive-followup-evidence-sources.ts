import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/commerce-server/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/finance-server/public/commercial-owner-projection-sources";
import "server-only";
import {salesFollowupMetricSource} from "@erpx/sales-server/adapters/metric-purchases";
import {nativeSalesPlan,SALES_RD_FOLLOWUP_EVIDENCE_PREFIX} from './native-sales-read';
import { z } from "zod";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { inputSchema, type FollowupRow, parseFollowupCursor } from '@erpx/platform-runtime-server/public/executive/crm-evidence-contract';
export function executiveFollowupEvidenceSources(db: SqlDatabase) {
    return { async searchRows1(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseFollowupCursor>, referenceDate: string): Promise<FollowupRow[]> {
            const purchased=await salesFollowupMetricSource(db,actor),evidencePrefix=(await nativeSalesPlan(SALES_RD_FOLLOWUP_EVIDENCE_PREFIX,[])).sql;
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
            const rows = (await db.prepare(`${evidencePrefix}(${purchased.sql}) c
    JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=c.order_id AND o.company_id=c.company_id JOIN (${financeProjectionSources.finance_parties}) p ON p.id=o.party_id AND p.company_id=o.company_id
    WHERE ${conditions.join(" AND ")} ORDER BY c.due_date,c.id LIMIT ?`)
                .bind(...values, input.limit + 1).all<FollowupRow>()).results;
            return rows;
        } };
}
