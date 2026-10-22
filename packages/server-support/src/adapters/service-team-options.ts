import { finalRootOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/final-root-owner-projection-sources";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { ServiceTeam } from "@erpx/server-support/adapters/service-teams";
import { serviceOwner } from "@erpx/server-support/adapters/service-access";
import { companyMembers } from "@erpx/server-organization/public/member-directory";
import { createServiceTeamOptions } from "@erpx/server-support/application/service-team-options";
export async function serviceTeamOptions(db: SqlDatabase, actor: WorkContext) {
    return createServiceTeamOptions({ owner: actor => serviceOwner(db, actor), nodes: async (actor) => (await db
            .prepare(`SELECT id,kind,display_name,branch_id FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE company_id=? AND status='active' AND kind IN ('company','branch','department') AND (?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR id=? OR kind='branch') ORDER BY display_name`)
            .bind(actor.companyId, actor.branchId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId)
            .all<{
            id: string;
            kind: string;
            display_name: string;
            branch_id: string | null;
        }>()).results, teams: async (actor) => (await db
            .prepare("SELECT * FROM service_teams WHERE company_id=? ORDER BY name,id")
            .bind(actor.companyId)
            .all<ServiceTeam>()).results, members: companyMembers,
        history: async (actor, team) => (await db
            .prepare("SELECT version,configuration_json,actor_id,created_at FROM service_team_revisions WHERE company_id=? AND team_id=? ORDER BY version DESC LIMIT 100")
            .bind(actor.companyId, team.id)
            .all<{
            version: number;
            configuration_json: string;
            actor_id: string;
            created_at: number;
        }>()).results, handlers: async (actor, team) => (await db.prepare("SELECT user_id,responsibility FROM service_team_members WHERE team_id=? AND company_id=? ORDER BY user_id").bind(team.id, actor.companyId).all<{
            user_id: string;
            responsibility: "lead" | "member";
        }>()).results, coverage: async (actor, team) => (await db
            .prepare("SELECT category,scope_kind,scope_node_id FROM service_team_coverage WHERE team_id=? AND company_id=? ORDER BY category,scope_node_id")
            .bind(team.id, actor.companyId)
            .all<{
            category: string;
            scope_kind: "holding" | "branch" | "department";
            scope_node_id: string;
        }>()).results,
    })(actor);
}
