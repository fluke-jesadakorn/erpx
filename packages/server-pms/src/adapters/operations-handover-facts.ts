import "server-only";
import { pmsSourceReadableSql } from "@erpx/server-pms/public/pms/source-policy";
import { pmsSourceRecipientProbeSql } from './pms-read-tail-r35-sql';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function HandoverPmsFacts(database: SqlDatabase) {
    return { readPublishHandoverAccessible(recipientId: string, id: string, companyId: string) {
            return database.prepare(`${pmsSourceRecipientProbeSql()}
          WHERE pr.id=? AND pr.company_id=? AND ${pmsSourceReadableSql("pr", "recipient.user_id")}`)
                .bind(recipientId, id, companyId).first();
        } };
}
