import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { RecordScope } from "@/server/modules/operations/public/staff-policy";
import { serviceTeamSchema, type ServiceTeamInput, type ServiceTeam, type ServiceHandler } from "../domain/service-teams";
import type { SupportViolation } from "./service-access";
export interface ServiceTeamValidationPorts {
    error(violation: SupportViolation): Error;
    owner(actor: WorkContext): Promise<void>;
    team(actor: WorkContext, id: string): Promise<ServiceTeam | null>;
    companyMember(companyId: string, userId: string): Promise<boolean>;
    node(companyId: string, id: string): Promise<{
        kind: string;
        branch_id: string | null;
    } | null>;
    validateScope(actor: WorkContext, scope: RecordScope): Promise<void>;
    scopeMember(scope: RecordScope, userId: string): Promise<boolean>;
}
export function createValidateServiceTeam(ports: ServiceTeamValidationPorts) {
    return async (actor: WorkContext, id: string | null, input: ServiceTeamInput) => {
        await ports.owner(actor);
        const current = id ? await ports.team(actor, id) : null;
        if (id && !current)
            throw ports.error({ status: 404, code: "TEAM_NOT_FOUND", message: "Service team not found." });
        if (current && current.version !== input.expectedVersion)
            throw ports.error({ status: 409, code: "VERSION_CONFLICT", message: "Team setup changed. Refresh before saving." });
        if (new Set(input.members.map(member => member.userId)).size !== input.members.length)
            throw ports.error({ status: 422, code: "DUPLICATE_MEMBER", message: "Choose each team member once." });
        if (new Set(input.coverage.map(coverage => `${coverage.category}:${coverage.scopeKind}:${coverage.scopeNodeId}`)).size !== input.coverage.length)
            throw ports.error({ status: 422, code: "DUPLICATE_COVERAGE", message: "Choose each coverage once." });
        if (input.active && !input.members.some(member => member.responsibility === "lead"))
            throw ports.error({ status: 422, code: "TEAM_LEAD_REQUIRED", message: "Choose a lead before activating this team." });
        for (const member of input.members)
            if (!await ports.companyMember(actor.companyId, member.userId))
                throw ports.error({ status: 422, code: "TEAM_MEMBER_INVALID", message: "Every handler needs current company membership. Use People to invite them first." });
        for (const coverage of input.coverage) {
            const node = await ports.node(actor.companyId, coverage.scopeNodeId);
            if (!node || node.kind !== (coverage.scopeKind === "holding" ? "company" : coverage.scopeKind))
                throw ports.error({ status: 422, code: "COVERAGE_INVALID", message: "Choose active coverage in this company." });
            const scope: RecordScope = { company_id: actor.companyId, branch_id: coverage.scopeKind === "branch" ? coverage.scopeNodeId : node.branch_id, department_id: coverage.scopeKind === "department" ? coverage.scopeNodeId : null };
            await ports.validateScope(actor, scope);
            if (input.active) {
                let covered = false;
                for (const member of input.members.filter(member => member.responsibility === "lead"))
                    if (await ports.scopeMember(scope, member.userId))
                        covered = true;
                if (!covered)
                    throw ports.error({ status: 422, code: "TEAM_COVERAGE_ACCESS_REQUIRED", message: "At least one team lead needs an assignment covering each selected location." });
            }
        }
        return current;
    };
}
export type ServiceTeamCommand = {
    actor: WorkContext;
    key: string;
    operation: string;
    id: string;
    version: number;
    now: number;
    input: ServiceTeamInput;
    result: {
        id: string;
        version: number;
    };
};
export function createSaveServiceTeam(ports: ServiceTeamValidationPorts & {
    snapshot(companyId: string): Promise<unknown>;
    now(): number;
    replay(actor: WorkContext, key: string, operation: string, id: string, input: ServiceTeamInput): Promise<{
        id: string;
        version: number;
    } | null>;
    prepare(command: ServiceTeamCommand, current: ServiceTeam | null, snapshot: unknown): Promise<unknown>;
    commit(command: ServiceTeamCommand, mutation: unknown): Promise<{
        id: string;
        version: number;
    }>;
}) {
    const validate = createValidateServiceTeam(ports);
    return async (actor: WorkContext, id: string | null, input: ServiceTeamInput, key: string) => {
        const snapshot = await ports.snapshot(actor.companyId);
        await ports.owner(actor);
        const teamId = id ?? `team-${key}`, operation = id ? "service.team.update" : "service.team.create", prior = await ports.replay(actor, key, operation, teamId, input);
        if (prior)
            return prior;
        const current = await validate(actor, id, input), version = current ? current.version + 1 : 1;
        const command: ServiceTeamCommand = { actor, key, id: teamId, operation, input, version, now: ports.now(), result: { id: teamId, version } };
        return ports.commit(command, await ports.prepare(command, current, snapshot));
    };
}
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
export function createPreviewServiceTeam(ports: ServiceTeamValidationPorts & {
    overlaps(actor: WorkContext, id: string | null, coverage: ServiceTeamInput["coverage"][number]): Promise<{
        name: string;
    }[]>;
}) {
    const validate = createValidateServiceTeam(ports);
    return async (actor: WorkContext, id: string | null, raw: unknown) => {
        const input = serviceTeamSchema.parse(raw), current = await validate(actor, id, input), overlaps: string[] = [];
        if (input.active)
            for (const coverage of input.coverage)
                for (const team of await ports.overlaps(actor, id, coverage))
                    overlaps.push(`${team.name} already covers ${coverage.category} at this scope. Overlapping active teams may prevent routing.`);
        return { previousVersion: current?.version ?? 0, newVersion: current ? current.version + 1 : 1, previous: current ? JSON.parse(current.configuration_json) : null, proposed: input, warnings: overlaps, effect: "New intake uses current coverage. Existing jobs retain their team until an explicit recorded repair." };
    };
}
