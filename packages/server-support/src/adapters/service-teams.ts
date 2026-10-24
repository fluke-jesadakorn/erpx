import { finalRootOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/final-root-owner-projection-sources";

import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
// Database-only domain code shared by the Next.js server and jobs Worker.
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

import { authorityGuard } from "@erpx/server-operations/public/staff-policy";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { ApiError } from "@erpx/server-runtime/public/http";
import { companyMember, requireServiceOwner, serviceAuthoritySnapshot, serviceMember, validateServiceScope, } from "@erpx/server-support/adapters/service-access";
import { createSaveServiceTeam, createPreviewServiceTeam, type ServiceTeamValidationPorts } from "@erpx/server-support/application/service-teams";
import type { ServiceTeam, ServiceTeamInput } from "@erpx/server-support/domain/service-teams";
export { serviceTeamSchema } from "@erpx/server-support/domain/service-teams";
export type { ServiceTeam } from "@erpx/server-support/domain/service-teams";
function validationPorts(db: SqlDatabase): ServiceTeamValidationPorts {
    return { error: violation => new ApiError(violation.status, violation.code, violation.message), owner: actor => requireServiceOwner(db, actor),
        team: async (actor, id) => await db
            .prepare("SELECT * FROM service_teams WHERE id=? AND company_id=?")
            .bind(id, actor.companyId)
            .first<ServiceTeam>(), companyMember: (companyId, userId) => companyMember(db, companyId, userId), node: async (companyId, id) => db.prepare(`SELECT kind,branch_id FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND company_id=? AND status='active'`).bind(id, companyId).first<{
            kind: string;
            branch_id: string | null;
        }>(), validateScope: (actor, scope) => validateServiceScope(db, actor, scope), scopeMember: (scope, userId) => serviceMember(db, scope, userId) };
}
export function nativeServiceTeams(db: SqlDatabase) {
    return createSaveServiceTeam({ ...validationPorts(db), snapshot: companyId => serviceAuthoritySnapshot(db, companyId), now: Date.now, replay: (actor, key, operation, id, input) => replayCommand<{
            id: string;
            version: number;
        }>(db, actor, key, operation, id, input),
        async prepare(command, current, captured) {
            const { actor, key, input, version, now, id: teamId } = command, snap = captured as Awaited<ReturnType<typeof serviceAuthoritySnapshot>>;
            return [
                ...snap(key),
                authorityGuard(db, key, actor, ["holding_owner"]),
                ...(current
                    ? [
                        assertion(db, key, "team-version", "EXISTS(SELECT 1 FROM service_teams WHERE id=? AND company_id=? AND version=?)", [teamId, actor.companyId, input.expectedVersion]),
                        db
                            .prepare("UPDATE service_teams SET name=?,active=?,version=?,configuration_json=?,updated_at=? WHERE id=? AND company_id=?")
                            .bind(input.name, input.active ? 1 : 0, version, JSON.stringify(input), now, teamId, actor.companyId),
                        db
                            .prepare("DELETE FROM service_team_coverage WHERE team_id=? AND company_id=?")
                            .bind(teamId, actor.companyId),
                        db
                            .prepare("DELETE FROM service_team_members WHERE team_id=? AND company_id=?")
                            .bind(teamId, actor.companyId),
                    ]
                    : [
                        db
                            .prepare("INSERT INTO service_teams(id,company_id,name,active,version,configuration_json,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)")
                            .bind(teamId, actor.companyId, input.name, input.active ? 1 : 0, version, JSON.stringify(input), actor.userId, now, now),
                    ]),
                db
                    .prepare("INSERT INTO service_team_revisions(company_id,team_id,version,configuration_json,actor_id,created_at) VALUES(?,?,?,?,?,?)")
                    .bind(actor.companyId, teamId, version, JSON.stringify(input), actor.userId, now),
                ...input.members.map((m) => db
                    .prepare("INSERT INTO service_team_members(company_id,team_id,user_id,responsibility) VALUES(?,?,?,?)")
                    .bind(actor.companyId, teamId, m.userId, m.responsibility)),
                ...(input.active ? input.coverage : []).map((c) => db
                    .prepare("INSERT INTO service_team_coverage(company_id,team_id,category,scope_kind,scope_node_id) VALUES(?,?,?,?,?)")
                    .bind(actor.companyId, teamId, c.category, c.scopeKind, c.scopeNodeId)),
            ];
        },
        commit: (command, statements) => executeCommand(db, command.actor, command.key, command.operation, command.input, command.id, command.version, command.result, statements as Parameters<typeof executeCommand>[8]),
    });
}
export async function saveServiceTeam(db: SqlDatabase, actor: WorkContext, id: string | null, input: ServiceTeamInput, key: string) { return nativeServiceTeams(db)(actor, id, input, key); }
export async function previewServiceTeam(db: SqlDatabase, actor: WorkContext, id: string | null, raw: unknown) { return createPreviewServiceTeam({ ...validationPorts(db), async overlaps(actor, id, c) { return (await db.prepare("SELECT t.name FROM service_team_coverage c JOIN service_teams t ON t.id=c.team_id AND t.company_id=c.company_id WHERE c.company_id=? AND c.category=? AND c.scope_kind=? AND c.scope_node_id=? AND t.active=1 AND (?::text IS NULL OR t.id<>?) ORDER BY t.name").bind(actor.companyId, c.category, c.scopeKind, c.scopeNodeId, id, id).all<{
        name: string;
    }>()).results; } })(actor, id, raw); }
