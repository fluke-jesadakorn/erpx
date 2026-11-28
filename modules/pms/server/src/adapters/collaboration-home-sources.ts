import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import { pmsSourceReadableSql } from '@erpx/pms-server/public/pms/source-policy';
import { pmsSourceRecipientProbeSql } from './pms-read-tail-r35-sql';
export function collaborationHomeSources(db: SqlDatabase) {
    return { guardPmsSource(key: string, value: string, user: string, id: string, companyId: string, value2: number | null, value3: number | null) {
            return assertion(db, key, value, `EXISTS(${pmsSourceRecipientProbeSql()}
        WHERE pr.id=? AND pr.company_id=? AND (?::bigint IS NULL OR pr.version=?) AND ${pmsSourceReadableSql("pr", "recipient.user_id")})`, [user, id, companyId, value2, value3]);
        } };
}
