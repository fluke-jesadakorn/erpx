import type { RecordScope } from "@erpx/server-operations/public/staff-policy";
import type { ServiceTeam, ServiceHandler } from "@erpx/server-support/domain/service-teams";
export function createServiceTeamHandlers(ports: {
    members(scope: RecordScope, id: string): Promise<ServiceHandler[]>;
    principals(companyId: string, ids: string[]): Promise<Map<string, string>>;
    member(scope: RecordScope, userId: string): Promise<boolean>;
}) {
    return async (scope: RecordScope, teamId: string | null) => {
        if (!teamId)
            return [];
        const members = await ports.members(scope, teamId), current = await ports.principals(scope.company_id, members.map(member => member.user_id));
        const unique = [...new Map(members.map(member => { const userId = current.get(member.user_id) ?? member.user_id; return [userId, { ...member, user_id: userId }] as const; })).values()], eligible: ServiceHandler[] = [];
        for (const member of unique)
            if (await ports.member(scope, member.user_id))
                eligible.push(member);
        return eligible;
    };
}
export function createResolveServiceTeam(ports: {
    teams(scope: RecordScope, category: string): Promise<(ServiceTeam & {
        scope_kind: string;
        scope_node_id: string;
    })[]>;
    handlers(scope: RecordScope, id: string): Promise<ServiceHandler[]>;
}) {
    return async (scope: RecordScope, category: string) => {
        const rows = await ports.teams(scope, category);
        for (const team of rows.slice(0, 1)) {
            const handlers = await ports.handlers(scope, team.id);
            if (handlers.some(handler => handler.responsibility === "lead"))
                return { team, handlers };
        }
        return null;
    };
}
