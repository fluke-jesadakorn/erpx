import {identityVerifiedContextProjections as operationsFinalOwnerProjections} from "../../operations/public/identity-access-verified-context-projections";
const operationsFinalSources=operationsFinalOwnerProjections();
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { assertion } from "@/server/modules/workflow/public/commands";
export function RoutinesIdentityAccessFacts(database: SqlDatabase) {
    return { guard3(key: string, value: "template-authority", id: string, companyId: string, userId: string, assignmentId: string, now: number, now2: number) {
            return assertion(database, key, value, `EXISTS(SELECT 1 ${operationsFinalSources.operations_routines_facts_routine_templates_t_from} WHERE t.id=? AND t.company_id=? AND (t.creator_user_id=? OR EXISTS(SELECT 1 FROM assignments a WHERE a.id=? AND a.company_id=t.company_id AND iam_role_allows(a.role,a.company_id,'company.owner') AND a.scope_kind='holding' AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_from<=? AND a.valid_until>?)))`, [id, companyId, userId, assignmentId, now, now2]);
        } };
}
