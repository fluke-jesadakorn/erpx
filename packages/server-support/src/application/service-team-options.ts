import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { companyMembers } from "@erpx/server-organization/public/member-directory";
import { serviceCategories } from "@erpx/feature-people_payroll/lib/employee-services";
import type { ServiceTeam, ServiceTeamInput } from "@erpx/server-support/domain/service-teams";
export interface ServiceTeamOptionPorts {
    owner(actor: WorkContext): Promise<boolean>;
    nodes(actor: WorkContext): Promise<{
        id: string;
        kind: string;
        display_name: string;
        branch_id: string | null;
    }[]>;
    teams(actor: WorkContext): Promise<ServiceTeam[]>;
    members(actor: WorkContext): ReturnType<typeof companyMembers>;
    history(actor: WorkContext, team: ServiceTeam): Promise<{
        version: number;
        configuration_json: string;
        actor_id: string;
        created_at: number;
    }[]>;
    handlers(actor: WorkContext, team: ServiceTeam): Promise<{
        user_id: string;
        responsibility: "lead" | "member";
    }[]>;
    coverage(actor: WorkContext, team: ServiceTeam): Promise<{
        category: string;
        scope_kind: "holding" | "branch" | "department";
        scope_node_id: string;
    }[]>;
}
export function createServiceTeamOptions(ports: ServiceTeamOptionPorts) {
    return async (actor: WorkContext) => {
        const owner = await ports.owner(actor), nodes = await ports.nodes(actor), teams = owner ? await ports.teams(actor) : [];
        const members = owner ? await ports.members(actor) : [];
        const projected = await Promise.all(teams.map(async (team) => ({ ...team, history: await ports.history(actor, team), members: await ports.handlers(actor, team), coverage: !team.active && team.configuration_json ? (JSON.parse(team.configuration_json) as ServiceTeamInput).coverage.map(coverage => ({ category: coverage.category, scope_kind: coverage.scopeKind, scope_node_id: coverage.scopeNodeId })) : await ports.coverage(actor, team) })));
        return { owner, categories: serviceCategories, nodes, members, teams: projected };
    };
}
