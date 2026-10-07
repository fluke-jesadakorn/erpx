import "server-only";
import { pmsSourcesSql, pmsSourceReadableSql } from "@/server/modules/pms/public/pms/source-policy";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
export function HandoverPmsFacts(database: SqlDatabase) {
    return { readPublishHandoverAccessible(recipientId: string, id: string, companyId: string) {
            return database.prepare(`SELECT 1 FROM (${pmsSourcesSql}) pr CROSS JOIN (SELECT ?::text user_id) recipient
          WHERE pr.id=? AND pr.company_id=? AND ${pmsSourceReadableSql("pr", "recipient.user_id")}`)
                .bind(recipientId, id, companyId).first();
        } };
}
