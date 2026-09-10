import type { SqlDatabase } from "./runtime/types";
// Database-only domain code shared by the Next.js server and jobs Worker.
import { z } from "zod";
import { serviceCategories } from "@/lib/employee-services";
import type { WorkContext } from "./authorization";
import type { RecordScope } from "./staff-policy";
import { authorityGuard } from "./staff-policy";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import {
  companyMember,
  requireServiceOwner,
  serviceAuthoritySnapshot,
  serviceMember,
  validateServiceScope,
} from "./service-access";

const category = z.enum(serviceCategories.map(([id]) => id));
export const serviceTeamSchema = z.object({
  name: z.string().trim().min(2).max(160),
  active: z.boolean().default(true),
  expectedVersion: z.number().int().positive().default(1),
  members: z
    .array(
      z.object({
        userId: z.string().min(1).max(160),
        responsibility: z.enum(["lead", "member"]),
      }),
    )
    .max(100),
  coverage: z
    .array(
      z.object({
        category,
        scopeKind: z.enum(["holding", "branch", "department"]),
        scopeNodeId: z.string().min(1).max(160),
      }),
    )
    .min(1)
    .max(50),
});
export type ServiceTeam = {
  id: string;
  company_id: string;
  name: string;
  active: number;
  version: number;
  configuration_json: string;
};
export async function saveServiceTeam(
  db: SqlDatabase,
  actor: WorkContext,
  id: string | null,
  input: z.infer<typeof serviceTeamSchema>,
  key: string,
) {
  const snap = await serviceAuthoritySnapshot(db, actor.companyId);
  await requireServiceOwner(db, actor);
  const teamId = id ?? `team-${key}`,
    operation = id ? "service.team.update" : "service.team.create";
  const prior = await replayCommand<{ id: string; version: number }>(
    db,
    actor,
    key,
    operation,
    teamId,
    input,
  );
  if (prior) return prior;
  const current = id
    ? await db
        .prepare("SELECT * FROM service_teams WHERE id=? AND company_id=?")
        .bind(id, actor.companyId)
        .first<ServiceTeam>()
    : null;
  if (id && !current)
    throw new ApiError(404, "TEAM_NOT_FOUND", "Service team not found.");
  if (current && current.version !== input.expectedVersion)
    throw new ApiError(
      409,
      "VERSION_CONFLICT",
      "Team setup changed. Refresh before saving.",
    );
  if (new Set(input.members.map((m) => m.userId)).size !== input.members.length)
    throw new ApiError(
      422,
      "DUPLICATE_MEMBER",
      "Choose each team member once.",
    );
  if (
    new Set(
      input.coverage.map(
        (c) => `${c.category}:${c.scopeKind}:${c.scopeNodeId}`,
      ),
    ).size !== input.coverage.length
  )
    throw new ApiError(422, "DUPLICATE_COVERAGE", "Choose each coverage once.");
  if (input.active && !input.members.some((m) => m.responsibility === "lead"))
    throw new ApiError(
      422,
      "TEAM_LEAD_REQUIRED",
      "Choose a lead before activating this team.",
    );
  for (const m of input.members)
    if (!(await companyMember(db, actor.companyId, m.userId)))
      throw new ApiError(
        422,
        "TEAM_MEMBER_INVALID",
        "Every handler needs current company membership. Use People to invite them first.",
      );
  for (const c of input.coverage) {
    const node = await db
      .prepare(
        "SELECT kind,branch_id FROM organization_nodes WHERE id=? AND company_id=? AND status='active'",
      )
      .bind(c.scopeNodeId, actor.companyId)
      .first<{ kind: string; branch_id: string | null }>();
    if (
      !node ||
      node.kind !== (c.scopeKind === "holding" ? "company" : c.scopeKind)
    )
      throw new ApiError(
        422,
        "COVERAGE_INVALID",
        "Choose active coverage in this company.",
      );
    const scope = {
      company_id: actor.companyId,
      branch_id: c.scopeKind === "branch" ? c.scopeNodeId : node.branch_id,
      department_id: c.scopeKind === "department" ? c.scopeNodeId : null,
    };
    await validateServiceScope(db, actor, scope);
    if (input.active) {
      let covered = false;
      for (const m of input.members.filter((m) => m.responsibility === "lead"))
        if (await serviceMember(db, scope, m.userId)) covered = true;
      if (!covered)
        throw new ApiError(
          422,
          "TEAM_COVERAGE_ACCESS_REQUIRED",
          "At least one team lead needs an assignment covering each selected location.",
        );
    }
  }
  const version = current ? current.version + 1 : 1,
    now = Date.now();
  const statements = [
    ...snap(key),
    authorityGuard(db, key, actor, ["holding_owner"]),
    ...(current
      ? [
          assertion(
            db,
            key,
            "team-version",
            "EXISTS(SELECT 1 FROM service_teams WHERE id=? AND company_id=? AND version=?)",
            [teamId, actor.companyId, input.expectedVersion],
          ),
          db
            .prepare(
              "UPDATE service_teams SET name=?,active=?,version=?,configuration_json=?,updated_at=? WHERE id=? AND company_id=?",
            )
            .bind(
              input.name,
              input.active ? 1 : 0,
              version,
              JSON.stringify(input),
              now,
              teamId,
              actor.companyId,
            ),
          db
            .prepare(
              "DELETE FROM service_team_coverage WHERE team_id=? AND company_id=?",
            )
            .bind(teamId, actor.companyId),
          db
            .prepare(
              "DELETE FROM service_team_members WHERE team_id=? AND company_id=?",
            )
            .bind(teamId, actor.companyId),
        ]
      : [
          db
            .prepare(
              "INSERT INTO service_teams(id,company_id,name,active,version,configuration_json,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)",
            )
            .bind(
              teamId,
              actor.companyId,
              input.name,
              input.active ? 1 : 0,
              version,
              JSON.stringify(input),
              actor.userId,
              now,
              now,
            ),
        ]),
    db
      .prepare(
        "INSERT INTO service_team_revisions(company_id,team_id,version,configuration_json,actor_id,created_at) VALUES(?,?,?,?,?,?)",
      )
      .bind(
        actor.companyId,
        teamId,
        version,
        JSON.stringify(input),
        actor.userId,
        now,
      ),
    ...input.members.map((m) =>
      db
        .prepare(
          "INSERT INTO service_team_members(company_id,team_id,user_id,responsibility) VALUES(?,?,?,?)",
        )
        .bind(actor.companyId, teamId, m.userId, m.responsibility),
    ),
    ...(input.active ? input.coverage : []).map((c) =>
      db
        .prepare(
          "INSERT INTO service_team_coverage(company_id,team_id,category,scope_kind,scope_node_id) VALUES(?,?,?,?,?)",
        )
        .bind(actor.companyId, teamId, c.category, c.scopeKind, c.scopeNodeId),
    ),
  ];
  return executeCommand(
    db,
    actor,
    key,
    operation,
    input,
    teamId,
    version,
    { id: teamId, version },
    statements,
  );
}
export async function teamHandlers(
  db: SqlDatabase,
  scope: RecordScope,
  teamId: string | null,
) {
  if (!teamId) return [];
  const members = (
    await db
      .prepare(
        "SELECT m.user_id,m.responsibility,COALESCE(p.legal_name,m.user_id) name FROM service_team_members m JOIN service_teams t ON t.id=m.team_id AND t.company_id=m.company_id LEFT JOIN hr_people p ON p.company_id=m.company_id AND p.user_id=m.user_id AND p.merged_into_id IS NULL WHERE t.id=? AND t.company_id=? AND t.active=1 ORDER BY m.user_id",
      )
      .bind(teamId, scope.company_id)
      .all<{
        user_id: string;
        responsibility: "lead" | "member";
        name: string;
      }>()
  ).results;
  const unique = [...new Map(members.map((m) => [m.user_id, m])).values()];
  const eligible = [];
  for (const m of unique)
    if (await serviceMember(db, scope, m.user_id)) eligible.push(m);
  return eligible;
}
export async function resolveServiceTeam(
  db: SqlDatabase,
  scope: RecordScope,
  category: string,
) {
  const rows = (
    await db
      .prepare(
        `SELECT t.*,c.scope_kind,c.scope_node_id FROM service_team_coverage c JOIN service_teams t ON t.id=c.team_id AND t.company_id=c.company_id
    WHERE c.company_id=? AND c.category=? AND t.active=1 AND ((c.scope_kind='holding' AND c.scope_node_id=?) OR (c.scope_kind='branch' AND c.scope_node_id=?) OR (c.scope_kind='department' AND c.scope_node_id=?))
    ORDER BY CASE c.scope_kind WHEN 'department' THEN 3 WHEN 'branch' THEN 2 ELSE 1 END DESC,t.id`,
      )
      .bind(
        scope.company_id,
        category,
        scope.company_id,
        scope.branch_id,
        scope.department_id,
      )
      .all<ServiceTeam & { scope_kind: string; scope_node_id: string }>()
  ).results;
  // An unavailable local team is a routing issue; do not silently send its queue elsewhere.
  for (const team of rows.slice(0, 1)) {
    const handlers = await teamHandlers(db, scope, team.id);
    if (handlers.some((h) => h.responsibility === "lead"))
      return { team, handlers };
  }
  return null;
}
