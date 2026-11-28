import {identitySnapshotSources as peoplepayrollSnapshotSources} from '@erpx/people-payroll-server/public/identity-snapshot-sources';
import {pmsRecipientProjectionSql} from './pms-recipient-projection-family-r26';
import {assignmentUserScopeProbeSql} from './read-tail-family-r29';
const peoplepayrollSources=peoplepayrollSnapshotSources();
import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
const query = async <T>(db: SqlDatabase, sql: string, ...values: unknown[]) => (await db
    .prepare(sql)
    .bind(...values)
    .all<T>()).results;
export function pmsIdentityAccessFacts(db: SqlDatabase) {
    return {
        readAssignments: (facts: {
            companyId: string;
            fact1: number;
            fact2: number;
            fact3: string;
        }) => query<{
            id: string;
            name: string;
        }>(db, `${pmsRecipientProjectionSql(peoplepayrollSources.pms_command_facts_hr_people_h_join)} AND h.merged_into_id IS NULL WHERE a.company_id=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?)) ORDER BY name`, facts.companyId, facts.fact1, facts.fact2, facts.fact3),
        readAssignments2: (facts: {
            assigneeId: unknown;
            companyId: string;
            fact2: number;
            fact3: number;
            branchId: string;
        }) => db.prepare(assignmentUserScopeProbeSql("revoked_at IS NULL AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?))")).bind(facts.assigneeId, facts.companyId, facts.fact2, facts.fact3, facts.branchId).first()
    };
}
