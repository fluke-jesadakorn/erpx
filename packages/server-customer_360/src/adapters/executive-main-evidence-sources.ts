import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { customerTimelineEvents, type CustomerActivityItem } from "@erpx/server-customer_360/public/customer-timeline";
export function executiveMainEvidenceSources(db: SqlDatabase) {
    return { async factsSearchCustomerActivityEvidence(actor: WorkContext, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "recent-customer-activity";
            cursor?: string | undefined;
        }, cursor: {
            at: number;
            id: string;
        } | null): Promise<{
            rows: CustomerActivityItem[];
        }> {
            const source = await customerTimelineEvents(actor,db);
            const conditions = ["e.at>=?", "e.at<?"];
            const values: unknown[] = [...source.values, Date.parse(input.filters.from), Date.parse(input.filters.to) + 86400000];
            if (input.query) {
                conditions.push("position(lower(?) in lower(concat_ws(' ',e.id,e.source_id,e.source_type,e.title,e.status,e.customer_name)))>0");
                values.push(input.query);
            }
            if (cursor) {
                conditions.push("(e.at,e.id)<(?,?)");
                values.push(cursor.at, cursor.id);
            }
            const rows = (await db.prepare(`${source.sql} WHERE ${conditions.join(" AND ")} ORDER BY e.at DESC,e.id DESC LIMIT ?`)
                .bind(...values, input.limit + 1).all<CustomerActivityItem>()).results;
            return { rows };
        }, async factsReadCustomerActivityEvidence(actor: WorkContext, recordId: string, filters: {
            from: string;
            to: string;
            currency: string;
        }): Promise<{
            row: CustomerActivityItem | null;
        }> {
            const source = await customerTimelineEvents(actor,db);
            const row = await db.prepare(`${source.sql} WHERE e.id=? AND e.at>=? AND e.at<? LIMIT 1`)
                .bind(...source.values, recordId, Date.parse(filters.from), Date.parse(filters.to) + 86400000).first<CustomerActivityItem>();
            return { row };
        } };
}
