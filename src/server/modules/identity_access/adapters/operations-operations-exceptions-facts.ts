import {operationsPolicyVersionGuard} from "../../operations/public/identity-policy-version-guard";
import {identitySnapshotSources as peoplepayrollSnapshotSources} from '../../people_payroll/public/identity-snapshot-sources';
const peoplepayrollSources=peoplepayrollSnapshotSources();
import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { assertion } from "@/server/modules/workflow/public/commands";
import { type Row } from '../../operations/public/operations-exceptions-contract';
export function OperationsExceptionsIdentityAccessFacts(database: SqlDatabase) {
    return { guard1(key: string, value: string, company_id: string, userId: string, permission: string, branch_id: string | null, department_id: string | null) {
            return assertion(database, key, value, `EXISTS(SELECT 1 FROM assignments g ${peoplepayrollSources.operations_operations_exceptions_facts_hr_people_p_join} ON p.user_id=g.user_id AND p.company_id=g.company_id ${peoplepayrollSources.operations_operations_exceptions_facts_hr_engagements_h_join} ON h.person_id=p.id AND h.company_id=p.company_id
   WHERE g.company_id=? AND g.user_id=? AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
   AND iam_role_allows(g.role,g.company_id,?) AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=? OR g.scope_kind='department' AND g.scope_node_id=?)
   AND p.status='active' AND p.merged_into_id IS NULL AND h.status='active' AND h.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (h.end_date IS NULL OR h.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))`, [company_id, userId, permission, branch_id, department_id]);
        },
        guard3(key: string, value: "policy-review", recordId: string, companyId: string, expectedVersion: number, value2: "published" | "draft", value3: string[]) { return operationsPolicyVersionGuard(database,key,value,recordId,companyId,expectedVersion,value2,value3); },
        guard4(key: string, value: "review-authority", assignmentId: string, companyId: string, userId: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM assignments WHERE id=? AND company_id=? AND user_id=? AND scope_kind='holding' AND iam_role_allows(role,company_id,'records.review'))", [assignmentId, companyId, userId]); },
        readOperationsWorkspaceCandidatespoliciespeople3(companyId: string, now: number, now2: number) { return database.prepare(`SELECT DISTINCT g.user_id id,COALESCE(p.legal_name,g.user_id) name FROM assignments g ${peoplepayrollSources.operations_operations_exceptions_facts_hr_people_p_join} ON p.user_id=g.user_id AND p.company_id=g.company_id WHERE g.company_id=? AND iam_assignment_active(g.id,?) AND iam_principal_active(g.user_id,g.company_id,?) AND p.status='active' AND p.merged_into_id IS NULL ORDER BY name`).bind(companyId, now, now2).all<Row>(); } };
}
