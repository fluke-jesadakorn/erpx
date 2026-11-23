import { finalRootOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/final-root-owner-projection-sources";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { ServiceTeam } from "@erpx/server-support/domain/service-teams";
import { serviceOwner } from "@erpx/server-support/adapters/service-access";
import { companyMembers } from "@erpx/server-organization/public/member-directory";
import { createServiceTeamOptions } from "@erpx/server-support/application/service-team-options";
import {
  nativeSupportReadAll,
  SUPPORT_RD_SERVICE_TEAM_OPTIONS,
  SUPPORT_RD_SERVICE_TEAM_OPTION_HISTORY,
  SUPPORT_RD_SERVICE_TEAM_OPTION_HANDLERS,
  SUPPORT_RD_SERVICE_TEAM_OPTION_COVERAGE,
} from "@erpx/server-support/adapters/native-support-read";

/** Owner-native service-team option reads. The owner-local statements are
 * planned by the Support `command-plan` (`service_team*` tables) and executed on
 * the caller's open unit; this module carries no SQL for these four reads. The
 * organization node list stays here because Organization owns that projection. */
export async function serviceTeamOptions(db: SqlDatabase, actor: WorkContext) {
    return createServiceTeamOptions({ owner: actor => serviceOwner(db, actor), nodes: async (actor) => (await db
            .prepare(`SELECT id,kind,display_name,branch_id FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE company_id=? AND status='active' AND kind IN ('company','branch','department') AND (?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR id=? OR kind='branch') ORDER BY display_name`)
            .bind(actor.companyId, actor.branchId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId)
            .all<{
            id: string;
            kind: string;
            display_name: string;
            branch_id: string | null;
        }>()).results, teams: async (actor) => (await nativeSupportReadAll<ServiceTeam>(
            SUPPORT_RD_SERVICE_TEAM_OPTIONS, [actor.companyId], db,
        )).results, members: companyMembers,
        history: async (actor, team) => (await nativeSupportReadAll<{
            version: number;
            configuration_json: string;
            actor_id: string;
            created_at: number;
        }>(SUPPORT_RD_SERVICE_TEAM_OPTION_HISTORY, [actor.companyId, team.id], db)).results,
        handlers: async (actor, team) => (await nativeSupportReadAll<{
            user_id: string;
            responsibility: "lead" | "member";
        }>(SUPPORT_RD_SERVICE_TEAM_OPTION_HANDLERS, [team.id, actor.companyId], db)).results,
        coverage: async (actor, team) => (await nativeSupportReadAll<{
            category: string;
            scope_kind: "holding" | "branch" | "department";
            scope_node_id: string;
        }>(SUPPORT_RD_SERVICE_TEAM_OPTION_COVERAGE, [team.id, actor.companyId], db)).results,
    })(actor);
}
