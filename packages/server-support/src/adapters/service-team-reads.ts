import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from '@erpx/server-people_payroll/public/final-root-owner-projection-sources';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { RecordScope } from '@erpx/server-operations/public/staff-policy';
import type { ServiceTeam } from '@erpx/server-support/domain/service-teams';
import { serviceMember } from '@erpx/server-support/adapters/service-access';
import { createServiceTeamHandlers, createResolveServiceTeam } from '@erpx/server-support/application/service-teams';
import { nativeSupportReadAll, SUPPORT_RD_SERVICE_TEAM_COVERAGE } from './native-support-read';
export function nativeServiceTeamHandlers(db: SqlDatabase) {
    return createServiceTeamHandlers({ members: async (scope, teamId) => (await db
            .prepare(`SELECT m.user_id,m.responsibility,COALESCE(p.legal_name,m.user_id) name FROM service_team_members m JOIN service_teams t ON t.id=m.team_id AND t.company_id=m.company_id LEFT JOIN (${people_payrollProjectionSources.hr_people}) p ON p.company_id=m.company_id AND p.user_id=m.user_id AND p.merged_into_id IS NULL WHERE t.id=? AND t.company_id=? AND t.active=1 ORDER BY m.user_id`)
            .bind(teamId, scope.company_id)
            .all<{
            user_id: string;
            responsibility: "lead" | "member";
            name: string;
        }>()).results,
        principals: async (companyId, ids) => (await import("@erpx/server-identity_access/public/iam-person")).currentPersonPrincipals(db, companyId, ids), member: (scope, userId) => serviceMember(db, scope, userId) });
}
export async function teamHandlers(db: SqlDatabase, scope: RecordScope, teamId: string | null) { return nativeServiceTeamHandlers(db)(scope, teamId); }
export async function resolveServiceTeam(db: SqlDatabase, scope: RecordScope, category: string) {
    return createResolveServiceTeam({ teams: async (scope, category) =>
        (await nativeSupportReadAll<ServiceTeam & { scope_kind: string; scope_node_id: string }>(
            SUPPORT_RD_SERVICE_TEAM_COVERAGE,
            [scope.company_id, category, scope.company_id, scope.branch_id, scope.department_id],
            db,
        )).results, handlers: (scope, id) => teamHandlers(db, scope, id) })(scope, category);
}
