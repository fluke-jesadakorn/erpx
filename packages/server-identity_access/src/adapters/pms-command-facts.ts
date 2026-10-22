import {identitySnapshotSources as peoplepayrollSnapshotSources} from '@erpx/server-people_payroll/public/identity-snapshot-sources';
const peoplepayrollSources=peoplepayrollSnapshotSources();
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
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
        }>(db, `SELECT DISTINCT a.user_id id,COALESCE(h.legal_name,a.user_id) name FROM assignments a LEFT ${peoplepayrollSources.pms_command_facts_hr_people_h_join} ON h.user_id=a.user_id AND h.company_id=a.company_id AND h.merged_into_id IS NULL WHERE a.company_id=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?)) ORDER BY name`, facts.companyId, facts.fact1, facts.fact2, facts.fact3),
        readAssignments2: (facts: {
            assigneeId: unknown;
            companyId: string;
            fact2: number;
            fact3: number;
            branchId: string;
        }) => db.prepare("SELECT 1 FROM assignments WHERE user_id=? AND company_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?))").bind(facts.assigneeId, facts.companyId, facts.fact2, facts.fact3, facts.branchId).first()
    };
}
