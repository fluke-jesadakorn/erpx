import {identitySnapshotSources as peoplepayrollSnapshotSources} from '@erpx/server-people_payroll/public/identity-snapshot-sources';
import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
const peoplepayrollSources=peoplepayrollSnapshotSources();
const organizationSources=organizationSnapshotSources();
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { type StaffPerson } from '@erpx/server-operations/public/staff-policy-contract';
const staffSelect = (datePredicate: string) => `SELECT p.id person_id,p.user_id,p.legal_name,e.id engagement_id,e.version engagement_version,
  e.company_id,e.branch_id,e.department_id,e.worker_type,e.start_date,e.end_date,e.manager_person_id,e.level_id,l.name level_name,l.rank level_rank,l.level_number,e.position_id,pos.is_ceo,pos.roster_required,e.self_service_top_level
  ${peoplepayrollSources.operations_staff_policy_facts_hr_people_p_from} ${peoplepayrollSources.operations_staff_policy_facts_hr_engagements_e_join} ON e.person_id=p.id AND e.company_id=p.company_id
  LEFT ${organizationSources.operations_staff_policy_facts_staff_levels_l_join} ON l.id=e.level_id AND l.company_id=e.company_id
  LEFT ${organizationSources.operations_staff_policy_facts_organization_positions_pos_join} ON pos.id=e.position_id AND pos.company_id=e.company_id AND pos.active=1
  WHERE p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND p.user_id IS NOT NULL
  AND e.status='active' ${datePredicate}
  AND EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=p.user_id AND a.company_id=p.company_id
    AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_from<=? AND a.valid_until>?)`;
export const activeStaffSelect = staffSelect("AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))");
export function StaffPolicyIdentityAccessFacts(database: SqlDatabase) {
    return { readReadActiveStaffResults(companyId: string, now: number, now2: number, userId: string) {
            return database.prepare(`${activeStaffSelect} AND iam_same_person(p.user_id,?,p.company_id)`)
                .bind(companyId, now, now2, userId)
                .all<StaffPerson>();
        },
        guard4(key: string, step: string, userId: string, companyId: string, userId2: string, companyId2: string, now: number, now2: number) {
            return assertion(database, key, step, `(SELECT COUNT(*) ${peoplepayrollSources.operations_staff_policy_facts_hr_people_p_from} ${peoplepayrollSources.operations_staff_policy_facts_hr_engagements_e_join} ON e.person_id=p.id AND e.company_id=p.company_id
    WHERE iam_same_person(p.user_id,?,p.company_id) AND p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active'
    AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))=1
    AND EXISTS(SELECT 1 FROM assignments WHERE iam_same_person(user_id,?,company_id) AND company_id=? AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?)`, [userId, companyId, userId2, companyId2, now, now2]);
        },
        readCompanyAuthorityEffect(assignmentId: string, companyId: string, value: string, now: number, now2: number) {
            return database.prepare(`SELECT 1 FROM assignments WHERE id=? AND company_id=? AND iam_role_allows(role,company_id,?) AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND scope_kind='holding' AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? LIMIT 1`)
                .bind(assignmentId, companyId, value, now, now2)
                .first();
        },
        guard6(key: string, value: "company-authority", assignmentId: string, companyId: string, value2: string, now: number, now2: number) {
            return assertion(database, key, value, `EXISTS(SELECT 1 FROM assignments WHERE id=? AND company_id=? AND iam_role_allows(role,company_id,?) AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND scope_kind='holding' AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?)`, [assignmentId, companyId, value2, now, now2]);
        },
        readEligibleRoleUsersResults(company_id: string, role: "hr_admin" | "holding_owner", now: number, now2: number, role2: "hr_admin" | "holding_owner", branch_id: string | null, department_id: string | null) {
            return database.prepare(`SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND iam_role_allows(role,company_id,'workflow.'||?) AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?
    AND (scope_kind='holding' OR (?='hr_admin' AND ((scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?)))) ORDER BY user_id`)
                .bind(company_id, role, now, now2, role2, branch_id, department_id)
                .all<{
                user_id: string;
            }>();
        },
        readRequestRouteEffect(user_id: string, companyId: string, now: number, now2: number, now3: number, branch_id: string | null, department_id: string | null) { return database.prepare(`SELECT 1 FROM assignments WHERE user_id=? AND company_id=? AND iam_role_allows(role,company_id,'workflow.people_manager') AND revoked_at IS NULL AND valid_from<=? AND valid_until>? AND iam_assignment_active(id,?) AND (scope_kind='holding' OR scope_kind='branch' AND scope_node_id=? OR scope_kind='department' AND scope_node_id=?)`).bind(user_id, companyId, now, now2, now3, branch_id, department_id).first(); } };
}
