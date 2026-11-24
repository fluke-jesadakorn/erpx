import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { RecordScope } from '@erpx/server-operations/public/staff-policy';
import type { ServiceTeam } from '@erpx/server-support/domain/service-teams';
import { serviceMember } from '@erpx/server-support/adapters/service-access';
import { createServiceTeamHandlers, createResolveServiceTeam } from '@erpx/server-support/application/service-teams';
import { nativeSupportReadAll, SUPPORT_RD_SERVICE_TEAM_COVERAGE, SUPPORT_RD_SERVICE_TEAM_MEMBERS_NAMED } from './native-support-read';
export function nativeServiceTeamHandlers(db: SqlDatabase) {
    return createServiceTeamHandlers({ members: async (scope, teamId) => (await nativeSupportReadAll<{
            user_id: string;
            responsibility: "lead" | "member";
            name: string;
        }>(
            SUPPORT_RD_SERVICE_TEAM_MEMBERS_NAMED,
            [teamId, scope.company_id],
            db,
        )).results,
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
