import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { ServiceTeam } from "@erpx/support-server/domain/service-teams";
import { serviceOwner } from "@erpx/support-server/adapters/service-access";
import { companyMembers } from "@erpx/organization-server/public/member-directory";
import { createServiceTeamOptions } from "@erpx/support-server/application/service-team-options";
import {
  nativeSupportReadAll,
  SUPPORT_RD_SERVICE_TEAM_OPTIONS,
  SUPPORT_RD_SERVICE_TEAM_OPTION_HISTORY,
  SUPPORT_RD_SERVICE_TEAM_OPTION_HANDLERS,
  SUPPORT_RD_SERVICE_TEAM_OPTION_COVERAGE,
  SUPPORT_RD_SERVICE_TEAM_OPTION_NODES,
} from "@erpx/support-server/adapters/native-support-read";

/** Owner-native service-team option reads. The owner-local statements and the
 * closed Organization node projection are planned by the Support `command-plan`
 * and executed on the caller's open unit; this module carries no SQL. */
export async function serviceTeamOptions(db: SqlDatabase, actor: WorkContext) {
    return createServiceTeamOptions({ owner: actor => serviceOwner(db, actor), nodes: async (actor) => (await nativeSupportReadAll<{
            id: string;
            kind: string;
            display_name: string;
            branch_id: string | null;
        }>(
            SUPPORT_RD_SERVICE_TEAM_OPTION_NODES,
            [actor.companyId, actor.branchId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId],
            db,
        )).results, teams: async (actor) => (await nativeSupportReadAll<ServiceTeam>(
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
