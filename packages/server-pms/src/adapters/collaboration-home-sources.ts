import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { pmsSourcesSql, pmsSourceReadableSql } from '@erpx/server-pms/public/pms/source-policy';
export function collaborationHomeSources(db: SqlDatabase) {
    return { guardPmsSource(key: string, value: string, user: string, id: string, companyId: string, value2: number | null, value3: number | null) {
            return assertion(db, key, value, `EXISTS(SELECT 1 FROM (${pmsSourcesSql}) pr CROSS JOIN (SELECT ?::text user_id) recipient
        WHERE pr.id=? AND pr.company_id=? AND (?::bigint IS NULL OR pr.version=?) AND ${pmsSourceReadableSql("pr", "recipient.user_id")})`, [user, id, companyId, value2, value3]);
        } };
}
