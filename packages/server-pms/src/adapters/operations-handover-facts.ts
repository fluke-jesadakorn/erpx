import "server-only";
import { pmsSourcesSql, pmsSourceReadableSql } from "@erpx/server-pms/public/pms/source-policy";
import { PMS_RECIPIENT_CROSS_JOIN_SQL } from './pms-read-tail-r21-sql';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function HandoverPmsFacts(database: SqlDatabase) {
    return { readPublishHandoverAccessible(recipientId: string, id: string, companyId: string) {
            return database.prepare(`SELECT 1 FROM (${pmsSourcesSql}) pr ${PMS_RECIPIENT_CROSS_JOIN_SQL}
          WHERE pr.id=? AND pr.company_id=? AND ${pmsSourceReadableSql("pr", "recipient.user_id")}`)
                .bind(recipientId, id, companyId).first();
        } };
}
