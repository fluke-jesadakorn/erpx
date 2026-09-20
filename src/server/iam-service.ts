import { iamDb } from "./iam-db";
import "server-only";
import { z } from "zod";
import type { PoolClient } from "pg";
import {
  permissionCatalog,
  hasPermission,
  evaluateAccess,
} from "@/lib/permissions";
import type { RoleRevision } from "@/lib/iam";
import { iamCommand } from "./iam-transactions";
import {
  type IamActor,
  iamPermission,
  assertCompanyScope,
} from "./iam-context";
import { requirePermission } from "./iam-permissions";
import { iamRole } from "./iam-store";
import { requireFreshMfa } from "./iam-mfa";
import { ApiError } from "./http";
import { runtimeEnv } from "./runtime";
import type { PostgresDatabase } from "./runtime/postgres";

const identifier = z.string().min(1).max(180);
export const roleInput = z.object({
  id: identifier.optional(),
  name: z.string().trim().min(2).max(100),
  expectedVersion: z.number().int().nonnegative(),
  permissions: z.array(identifier).max(1000),
  grantablePermissions: z.array(identifier).max(1000).default([]),
  scopes: z
    .array(z.enum(["holding", "branch", "department", "platform"]))
    .min(1),
  reason: z.string().trim().min(3).max(1000),
});
export const grantInput = z.object({
  userId: identifier,
  roleId: identifier,
  expectedRoleRevision: z.number().int().positive().optional(),
  scopeKind: z.enum(["holding", "branch", "department", "platform"]),
  scopeNodeId: identifier.nullable().default(null),
  validUntil: z.number().int().positive().default(0),
  reason: z.string().trim().min(3).max(1000),
});
export const reviewInput = z.object({
  expectedVersion: z.number().int().positive(),
  decision: z.enum(["approve", "reject"]),
  reason: z.string().trim().min(3).max(1000),
});
const fail = (code: string, message: string) => {
  throw new ApiError(409, code, message);
};

export async function iamAuditHistory(
  db: PostgresDatabase,
  actor: IamActor,
  search: string,
  cursor?: string,
) {
  assertCompanyScope(actor);
  requirePermission(actor, iamPermission(actor, "iam.read"));
  db = iamDb(db);
  let after: { createdAt: number; id: string } | null = null;
  if (cursor) {
    try {
      after = z
        .object({
          createdAt: z.number().int().nonnegative(),
          id: z.string().uuid(),
        })
        .parse(JSON.parse(cursor));
    } catch {
      throw new ApiError(422, "INVALID_CURSOR", "Refresh the audit history.");
    }
  }
  const needle = `%${search.slice(0, 200).replace(/[\\%_]/g, "\\$&")}%`;
  const rows = (
    await db
      .prepare(
        "SELECT * FROM iam_audit WHERE company_id=? AND (action ILIKE ? OR actor_id ILIKE ? OR target_id ILIKE ?) AND (?::bigint IS NULL OR created_at<? OR (created_at=? AND id<?)) ORDER BY created_at DESC,id DESC LIMIT 101",
      )
      .bind(
        actor.companyId,
        needle,
        needle,
        needle,
        after?.createdAt ?? null,
        after?.createdAt ?? null,
        after?.createdAt ?? null,
        after?.id ?? null,
      )
      .all<Record<string, unknown>>()
  ).results;
  const items = rows.slice(0, 100),
    last = items.at(-1);
  return {
    items,
    nextCursor:
      rows.length > 100 && last
        ? JSON.stringify({
            createdAt: Number(last.created_at),
            id: String(last.id),
          })
        : null,
  };
}

export async function listIamRoles(
  db: PostgresDatabase,
  companyId: string,
): Promise<RoleRevision[]> {
  db = iamDb(db);
  const rows = (
    await db
      .prepare(
        "SELECT id FROM iam_roles WHERE company_id IN ('*',?) AND status='active' AND published_revision>0 ORDER BY built_in DESC,name",
      )
      .bind(companyId)
      .all<{ id: string }>()
  ).results;
  return (
    await Promise.all(rows.map((r) => iamRole(db, companyId, r.id)))
  ).filter((r): r is RoleRevision => !!r);
}
export async function iamOverview(
  db: PostgresDatabase,
  actor: IamActor,
  self = false,
) {
  db = iamDb(db);
  requirePermission(
    actor,
    iamPermission(actor, self ? "iam.self" : "iam.read"),
  );
  const roles = await listIamRoles(db, actor.companyId),
    table = actor.platform ? "platform_assignments" : "assignments";
  const rows = (
    await db
      .prepare(
        `SELECT a.*,${actor.platform ? "'platform' AS company_id,'platform' AS scope_kind,NULL AS scope_node_id,NULL AS branch_id," : ""} m.status identity_status,m.login_policy,m.person_id,m.connection_id,m.version identity_version,mp.group_id,mapping_group.name group_name,mp.connection_id mapping_connection_id,i.enabled directory_enabled,i.observed_at,c.status connection_status FROM ${table} a LEFT JOIN iam_members m ON m.user_id=a.user_id AND m.company_id=? LEFT JOIN iam_identities i ON i.id=m.identity_id LEFT JOIN iam_connections c ON c.id=m.connection_id LEFT JOIN iam_mappings mp ON a.source='directory' AND mp.id=a.source_id AND mp.company_id=${actor.platform ? "'platform'" : "a.company_id"} LEFT JOIN iam_groups mapping_group ON mapping_group.connection_id=mp.connection_id AND mapping_group.external_id=mp.group_id WHERE ${actor.platform ? "true" : "a.company_id=?"} AND (?::text IS NULL OR a.user_id=?) ${!actor.platform && actor.scopeKind !== "holding" ? "AND (a.scope_node_id=? OR a.scope_node_id IN (SELECT id FROM organization_nodes WHERE branch_id=? AND company_id=?))" : ""} ORDER BY a.user_id,a.role`,
      )
      .bind(
        actor.companyId,
        ...(actor.platform ? [] : [actor.companyId]),
        self ? actor.userId : null,
        self ? actor.userId : null,
        ...(!actor.platform && actor.scopeKind !== "holding"
          ? [actor.scopeNodeId, actor.scopeNodeId, actor.companyId]
          : []),
      )
      .all<Record<string, unknown>>()
  ).results;
  const people =
    !actor.platform && !self
      ? (
          await db
            .prepare(
              "SELECT id,user_id,legal_name FROM hr_people WHERE company_id=? AND status='active' AND (? OR EXISTS(SELECT 1 FROM hr_engagements e WHERE e.person_id=hr_people.id AND e.status='active' AND (e.branch_id=? OR e.department_id=?)))",
            )
            .bind(
              actor.companyId,
              actor.scopeKind === "holding",
              actor.scopeNodeId,
              actor.scopeNodeId,
            )
            .all<{ id: string; user_id: string | null; legal_name: string }>()
        ).results
      : [];
  const nodes = !actor.platform
    ? (
        await db
          .prepare(
            "SELECT id,display_name,kind,branch_id FROM organization_nodes WHERE company_id=? AND status='active' AND (? OR id=? OR branch_id=?) ORDER BY kind,display_name",
          )
          .bind(
            actor.companyId,
            actor.scopeKind === "holding",
            actor.scopeNodeId,
            actor.scopeNodeId,
          )
          .all()
      ).results
    : [];
  const migrations =
    !self && canReadCompany(actor)
      ? (
          await db
            .prepare(
              "SELECT * FROM iam_migrations WHERE company_id=? ORDER BY created_at DESC LIMIT 100",
            )
            .bind(actor.companyId)
            .all()
        ).results
      : [];
  const ids = [
    ...new Set([
      ...rows.map((r) => String(r.user_id)),
      ...people.flatMap((p) => (p.user_id ? [p.user_id] : [])),
    ]),
  ];
  if (self && !ids.includes(actor.userId)) ids.push(actor.userId);
  const env = await runtimeEnv();
  const users = ids.length
    ? (
        await env.CONTROL_DB.prepare(
          `SELECT id,name,email FROM "user" WHERE id IN (${ids.map(() => "?").join(",")})`,
        )
          .bind(...ids)
          .all<{ id: string; name: string; email: string }>()
      ).results
    : [];
  const ownStatus = await env.CONTROL_DB.prepare(
    "SELECT enabled FROM iam_mfa WHERE user_id=?",
  )
    .bind(actor.userId)
    .first<{ enabled: boolean }>();
  const ownSessions = (
    await env.CONTROL_DB.prepare(
      'SELECT id,"createdAt","expiresAt","userAgent" FROM session WHERE "userId"=? ORDER BY "createdAt" DESC',
    )
      .bind(actor.userId)
      .all()
  ).results;
  const canManageDirectory =
    hasPermission(actor, iamPermission(actor, "iam.directory.manage")) &&
    ["holding", "platform"].includes(actor.scopeKind);
  const connections = !self
    ? (
        await db
          .prepare(
            `SELECT ${canManageDirectory ? "*" : "id,name,provider,status,health,last_sync_at,last_error,version"} FROM iam_connections WHERE company_id=? ORDER BY name`,
          )
          .bind(actor.companyId)
          .all<Record<string, unknown>>()
      ).results
    : [];
  const canManageMappings = hasPermission(
    actor,
    iamPermission(actor, "iam.manage"),
  );
  const groups: Record<string, unknown>[] =
    !self && (canManageDirectory || canManageMappings)
      ? (
          await db
            .prepare(
              "SELECT g.* FROM iam_groups g JOIN iam_connections c ON c.id=g.connection_id WHERE c.company_id=? ORDER BY g.name",
            )
            .bind(actor.companyId)
            .all()
        ).results
      : [];
  const previews =
    !self && canManageDirectory
      ? (
          await db
            .prepare(
              "SELECT p.* FROM iam_connection_previews p JOIN iam_connections c ON c.id=p.connection_id AND c.version=p.version WHERE c.company_id=?",
            )
            .bind(actor.companyId)
            .all<Record<string, unknown>>()
        ).results
      : [];
  for (const preview of previews)
    for (const group of (
      preview.snapshot_json as {
        groups: { externalId: string; name: string }[];
      }
    ).groups)
      if (
        !groups.some(
          (g) =>
            g.connection_id === preview.connection_id &&
            g.external_id === group.externalId,
        )
      )
        groups.push({
          connection_id: preview.connection_id,
          external_id: group.externalId,
          name: group.name,
          observed_at: preview.created_at,
          preview: true,
        });
  const mappings = !self
    ? (
        await db
          .prepare(
            `SELECT * FROM iam_mappings WHERE company_id=? ${["holding", "platform"].includes(actor.scopeKind) ? "" : "AND (scope_node_id=? OR scope_node_id IN (SELECT id FROM organization_nodes WHERE branch_id=? AND company_id=?))"} ORDER BY id`,
          )
          .bind(
            actor.companyId,
            ...(["holding", "platform"].includes(actor.scopeKind)
              ? []
              : [actor.scopeNodeId, actor.scopeNodeId, actor.companyId]),
          )
          .all()
      ).results
    : [];
  const identities =
    !self && canManageDirectory
      ? (
          await db
            .prepare(
              "SELECT * FROM iam_identities WHERE company_id=? ORDER BY name",
            )
            .bind(actor.companyId)
            .all()
        ).results
      : [];
  const reviews = !self
    ? (
        await db
          .prepare(
            `SELECT * FROM iam_reviews WHERE company_id=? AND (? OR created_by=? OR (? AND kind IN ('assignment.grant','mapping.activate','policy.grant') AND (payload->>'scopeNodeId'=? OR ${actor.platform ? "false AND ?::text IS NULL AND ?::text IS NULL" : "payload->>'scopeNodeId' IN (SELECT id FROM organization_nodes WHERE company_id=? AND branch_id=?)"}))) ORDER BY created_at DESC LIMIT 100`,
          )
          .bind(
            actor.companyId,
            hasPermission(actor, iamPermission(actor, "iam.review")) &&
              ["holding", "platform"].includes(actor.scopeKind),
            actor.userId,
            hasPermission(actor, iamPermission(actor, "iam.review")),
            actor.scopeNodeId,
            actor.companyId,
            actor.scopeNodeId,
          )
          .all()
      ).results
    : [];
  const audit =
    !self && ["holding", "platform"].includes(actor.scopeKind)
      ? (
          await db
            .prepare(
              "SELECT * FROM iam_audit WHERE company_id=? ORDER BY created_at DESC,id DESC LIMIT 100",
            )
            .bind(actor.companyId)
            .all()
        ).results
      : [];
  const catalog = permissionCatalog.filter(
    (p) => (p.namespace === "platform") === actor.platform,
  );
  const validRows: (Record<string, unknown> & { active: boolean })[] =
    await Promise.all(
      rows.map(async (r) => {
        const active = await db
          .prepare("SELECT iam_assignment_active(?,?) active")
          .bind(r.id, Date.now())
          .first<{ active: boolean }>();
        return {
          ...r,
          active:
            !!active?.active &&
            (!r.identity_status || r.identity_status === "active") &&
            (r.login_policy !== "directory" ||
              (r.directory_enabled === true &&
                r.connection_status === "active" &&
                Number(r.observed_at) > Date.now() - 300000)),
        };
      }),
    );
  const decisions = validRows.map((r) => {
    const role = roles.find((v) => v.id === r.role);
    return {
      assignmentId: String(r.id),
      userId: String(r.user_id),
      decisions: catalog.map((p) => ({
        ...evaluateAccess(
          {
            userId: String(r.user_id),
            role: String(r.role),
            permissions: role?.permissions ?? [],
            policyRevision: role?.revision,
            assignmentId: String(r.id),
            assignmentScope: String(r.scope_kind),
          },
          p.id,
          {
            inspect: true,
            valid: r.active,
            expiresAt: Number(r.valid_until),
            source:
              r.source === "directory"
                ? `Directory group: ${r.group_name ?? r.group_id ?? "Unavailable"} · mapping ${r.source_id}`
                : r.source === "policy"
                  ? `Reviewed access policy · evidence ${r.source_id}`
                  : `Manual assignment${r.parent_id ? " · parent " + r.parent_id : ""}`,
          },
        ),
        scope: `${r.scope_kind}${r.scope_node_id ? " · " + (nodes.find((n) => n.id === r.scope_node_id)?.display_name ?? r.scope_node_id) : " · " + actor.companyId}`,
      })),
    };
  });
  return {
    evaluatedAt: Date.now(),
    previews,
    people,
    nodes,
    migrations,
    companyId: actor.companyId,
    scopeKind: actor.scopeKind,
    validUntil: Number(
      rows.find((r) => r.id === actor.assignmentId)?.valid_until ?? Date.now(),
    ),
    roles,
    users,
    assignments: validRows,
    connections,
    groups,
    mappings,
    identities,
    reviews,
    audit,
    catalog,
    decisions,
    ownSessions,
    currentSessionId: actor.sessionId,
    mfaEnabled: !!ownStatus?.enabled,
    currentUserId: actor.userId,
    currentAssignmentId: actor.assignmentId,
    permissions: actor.iam?.permissions ?? [],
    platform: actor.platform,
    grantablePermissions: actor.iam?.grantablePermissions ?? [],
  };
}
function canReadCompany(actor: IamActor) {
  return (
    ["holding", "platform"].includes(actor.scopeKind) &&
    hasPermission(actor, iamPermission(actor, "iam.manage"))
  );
}
async function review(
  client: PoolClient,
  actor: IamActor,
  kind: string,
  payload: unknown,
  reason: string,
) {
  const id = crypto.randomUUID();
  await client.query(
    "INSERT INTO iam_reviews(id,company_id,kind,payload,created_by,created_person,created_at,reason) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    [
      id,
      actor.companyId,
      kind,
      JSON.stringify({
        ...(payload as Record<string, unknown>),
        authorizingAssignmentId: actor.assignmentId,
      }),
      actor.userId,
      actor.iam?.personId ?? actor.userId,
      Date.now(),
      reason,
    ],
  );
  return {
    id,
    status: "pending",
    message: "Submitted for independent review.",
  };
}
export async function saveIamRole(
  db: PostgresDatabase,
  actor: IamActor,
  input: z.infer<typeof roleInput>,
  key: string,
) {
  db = iamDb(db);
  assertCompanyScope(actor);
  await requireFreshMfa(actor);
  const catalog = permissionCatalog
    .filter(
      (p) =>
        (p.namespace === "platform") === actor.platform &&
        p.availability === "live",
    )
    .map((p) => p.id);
  if (
    [...input.permissions, ...input.grantablePermissions].some(
      (p) =>
        !catalog.includes(p) || !actor.iam?.grantablePermissions.includes(p),
    )
  )
    throw new ApiError(
      403,
      "DELEGATION_EXCEEDED",
      "The role exceeds your delegated permission set.",
    );
  if (
    input.scopes.some((s) =>
      actor.platform ? s !== "platform" : s === "platform",
    )
  )
    throw new ApiError(
      422,
      "ROLE_SCOPE_INVALID",
      "Role scopes must remain inside their namespace.",
    );
  return iamCommand(
    db,
    actor,
    key,
    "role.propose",
    input,
    "iam.roles.manage",
    async (c) => {
      if (
        input.id &&
        !input.id.startsWith(actor.platform ? "platform_custom_" : "custom_")
      )
        throw new ApiError(
          403,
          "PROTECTED_ROLE",
          "Clone a built-in role instead of replacing it.",
        );
      const id =
        input.id ??
        `${actor.platform ? "platform_custom_" : "custom_"}${crypto.randomUUID()}`;
      const existing = (
        await c.query(
          "SELECT * FROM iam_roles WHERE id=$1 AND company_id IN ($2,'*') FOR UPDATE",
          [id, actor.companyId],
        )
      ).rows[0];
      if (existing?.built_in)
        fail(
          "PROTECTED_ROLE",
          "Built-in roles are protected. Clone the role instead.",
        );
      if (Number(existing?.published_revision ?? 0) !== input.expectedVersion)
        fail("ROLE_CHANGED", "The role changed. Review the latest revision.");
      if (!existing)
        await c.query(
          "INSERT INTO iam_roles(company_id,id,name) VALUES($1,$2,$3)",
          [actor.companyId, id, input.name],
        );
      const revision = Number(
        (
          await c.query(
            "SELECT COALESCE(MAX(revision),0)+1 next FROM iam_role_revisions WHERE company_id=$1 AND role_id=$2",
            [actor.companyId, id],
          )
        ).rows[0].next,
      );
      await c.query(
        "INSERT INTO iam_role_revisions VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        [
          actor.companyId,
          id,
          revision,
          JSON.stringify([...new Set(input.permissions)]),
          JSON.stringify([...new Set(input.grantablePermissions)]),
          JSON.stringify(input.scopes),
          actor.userId,
          Date.now(),
        ],
      );
      const affected = (
        await c.query(
          `SELECT DISTINCT user_id FROM ${actor.platform ? "platform_assignments" : "assignments"} WHERE role=$1 AND revoked_at IS NULL ${actor.platform ? "" : "AND company_id=$2"}`,
          actor.platform ? [id] : [id, actor.companyId],
        )
      ).rows.map((r) => r.user_id);
      return {
        ...(await review(
          c,
          actor,
          "role.publish",
          {
            id,
            name: input.name,
            revision,
            expectedVersion: input.expectedVersion,
            permissions: input.permissions,
            grantablePermissions: input.grantablePermissions,
            affected,
          },
          input.reason,
        )),
        roleId: id,
        revision,
        affected,
      };
    },
  );
}
export async function validateIamGrant(
  c: PoolClient,
  actor: IamActor,
  input: z.infer<typeof grantInput>,
  parentId = actor.assignmentId,
) {
  if (
    input.userId === actor.userId ||
    (
      await c.query("SELECT iam_same_person($1,$2,$3) same", [
        input.userId,
        actor.userId,
        actor.companyId,
      ])
    ).rows[0].same
  )
    throw new ApiError(
      403,
      "SELF_ACCESS_CHANGE",
      "Another administrator must change your access.",
    );
  const table = actor.platform ? "platform_assignments" : "assignments";
  const parent = (
    await c.query(`SELECT * FROM ${table} WHERE id=$1 FOR SHARE`, [parentId])
  ).rows[0];
  if (
    !parent ||
    !(
      await c.query("SELECT iam_assignment_active($1,$2) ok", [
        parentId,
        Date.now(),
      ])
    ).rows[0].ok
  )
    fail("PARENT_EXPIRED", "The authorizing assignment is no longer active.");
  const reviewer = (
    await c.query(`SELECT valid_until FROM ${table} WHERE id=$1`, [
      actor.assignmentId,
    ])
  ).rows[0];
  if (input.validUntil === 0)
    input.validUntil = Math.min(
      Date.now() + 90 * 86400000,
      Number(parent.valid_until),
      Number(reviewer?.valid_until ?? parent.valid_until),
    );
  if (
    input.validUntil <= Date.now() ||
    input.validUntil > Number(parent.valid_until) ||
    (reviewer && input.validUntil > Number(reviewer.valid_until))
  )
    throw new ApiError(
      422,
      "EXPIRY_INVALID",
      "Access must expire within the authorizing assignment.",
    );
  const parentRole = (
    await c.query(
      "SELECT v.grantable_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.id=$1 AND r.company_id IN ('*',$2) AND r.status='active'",
      [parent.role, actor.companyId],
    )
  ).rows[0];
  if (!parentRole)
    fail("PARENT_ROLE_UNAVAILABLE", "The authorizing role is unavailable.");
  const member = (
    await c.query(
      "SELECT status,login_policy FROM iam_members WHERE company_id=$1 AND user_id=$2",
      [actor.companyId, input.userId],
    )
  ).rows[0];
  if (input.userId.startsWith("dir_") && member?.login_policy !== "directory")
    throw new ApiError(
      403,
      "DIRECTORY_IDENTITY_REVIEW_REQUIRED",
      "Connect and review this directory identity in this company before assigning access.",
    );
  if (member && member.status !== "active")
    throw new ApiError(
      403,
      "IDENTITY_INACTIVE",
      "Suspended or superseded identities cannot receive new access.",
    );
  const role = (
    await c.query(
      "SELECT r.*,v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id IN ('*',$1) AND r.id=$2 AND r.status='active'",
      [actor.companyId, input.roleId],
    )
  ).rows[0];
  if (!role || !role.scopes_json.includes(input.scopeKind))
    fail("ROLE_UNAVAILABLE", "Choose an active role and supported scope.");
  if (
    input.expectedRoleRevision !== undefined &&
    input.expectedRoleRevision !== Number(role.published_revision)
  )
    fail(
      "ROLE_CHANGED",
      "The role changed. Review its current permissions before granting access.",
    );
  input.expectedRoleRevision = Number(role.published_revision);
  if (
    [...role.permissions_json, ...role.grantable_json].some(
      (p: string) =>
        !actor.iam?.grantablePermissions.includes(p) ||
        !parentRole.grantable_json.includes(p),
    )
  )
    throw new ApiError(
      403,
      "DELEGATION_EXCEEDED",
      "This role exceeds your delegated grant permissions.",
    );
  if (!actor.platform) {
    if (parent.company_id !== actor.companyId)
      throw new ApiError(
        403,
        "SCOPE_FORBIDDEN",
        "The parent belongs to another company.",
      );
    if (input.scopeKind === "platform")
      throw new ApiError(
        403,
        "SCOPE_FORBIDDEN",
        "Platform access is separate.",
      );
    if (input.scopeKind !== "holding") {
      const node = (
        await c.query(
          "SELECT * FROM organization_nodes WHERE id=$1 AND company_id=$2 AND kind=$3 AND status='active'",
          [input.scopeNodeId, actor.companyId, input.scopeKind],
        )
      ).rows[0];
      if (!node)
        fail("SCOPE_UNAVAILABLE", "Choose an active organization scope.");
      if (
        (parent.scope_kind === "department" &&
          parent.scope_node_id !== node.id) ||
        (parent.scope_kind === "branch" &&
          parent.scope_node_id !==
            (node.kind === "branch" ? node.id : node.branch_id)) ||
        (actor.scopeKind === "department" && actor.scopeNodeId !== node.id) ||
        (actor.scopeKind === "branch" &&
          actor.scopeNodeId !==
            (node.kind === "branch" ? node.id : node.branch_id))
      )
        throw new ApiError(
          403,
          "SCOPE_FORBIDDEN",
          "The grant exceeds its parent scope.",
        );
    } else if (parent.scope_kind !== "holding" || actor.scopeKind !== "holding")
      throw new ApiError(
        403,
        "SCOPE_FORBIDDEN",
        "The grant exceeds its parent scope.",
      );
  }
  return role;
}
export async function insertIamGrant(
  c: PoolClient,
  actor: IamActor,
  input: z.infer<typeof grantInput>,
  parentId: string,
  source = "manual",
  sourceId: string | null = null,
) {
  const id = `iam-${crypto.randomUUID()}`,
    modules = [
      ...new Set(
        input.roleId === "holding_owner"
          ? permissionCatalog
              .filter((p) => p.id.startsWith("module."))
              .map((p) => p.module)
          : (
              await c.query(
                "SELECT v.permissions_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.id=$1 AND r.company_id IN ('*',$2)",
                [input.roleId, actor.companyId],
              )
            ).rows.flatMap((r) =>
              (r.permissions_json as string[])
                .filter((p) => p.startsWith("module."))
                .map((p) => p.split(".")[1]),
            ),
      ),
    ];
  if (actor.platform)
    await c.query(
      "INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until,parent_id,source,source_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
      [
        id,
        input.userId,
        input.roleId,
        JSON.stringify(modules),
        Date.now(),
        input.validUntil,
        parentId,
        source,
        sourceId,
      ],
    );
  else
    await c.query(
      "INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,scope_kind,scope_node_id,parent_id,source,source_id) VALUES($1,$2,$3,$4,$5,(SELECT CASE WHEN kind='branch' THEN id ELSE branch_id END FROM organization_nodes WHERE id=$6),$7,$8,$9,$6,$10,$11,$12)",
      [
        id,
        input.userId,
        input.roleId,
        JSON.stringify(modules),
        actor.companyId,
        input.scopeNodeId,
        Date.now(),
        input.validUntil,
        input.scopeKind,
        parentId,
        source,
        sourceId,
      ],
    );
  return id;
}
export async function grantIamAccess(
  db: PostgresDatabase,
  actor: IamActor,
  input: z.infer<typeof grantInput>,
  key: string,
) {
  db = iamDb(db);
  const target = await (
    await runtimeEnv()
  ).CONTROL_DB.prepare('SELECT id FROM "user" WHERE id=?')
    .bind(input.userId)
    .first();
  if (!target)
    throw new ApiError(404, "USER_NOT_FOUND", "Choose an existing account.");
  await requireFreshMfa(actor);
  return iamCommand(
    db,
    actor,
    key,
    "assignment.propose",
    input,
    "iam.manage",
    async (c) => {
      await validateIamGrant(c, actor, input);
      return review(
        c,
        actor,
        "assignment.grant",
        {
          ...input,
          parentId: actor.assignmentId,
          grantablePermissions: actor.iam?.grantablePermissions ?? [],
        },
        input.reason,
      );
    },
  );
}
export async function revokeIamAssignment(
  db: PostgresDatabase,
  actor: IamActor,
  id: string,
  version: number,
  reason: string,
  key: string,
) {
  db = iamDb(db);
  return iamCommand(
    db,
    actor,
    key,
    "assignment.revoke",
    { id, version, reason },
    "iam.manage",
    async (c) => {
      return applyIamRevocation(c, actor, id, version, reason);
    },
  );
}
export async function applyIamRevocation(
  c: PoolClient,
  actor: IamActor,
  id: string,
  version: number,
  reason: string,
  approved = false,
) {
  const table = actor.platform ? "platform_assignments" : "assignments";
  const row = (
    await c.query(
      `SELECT * FROM ${table} WHERE id=$1 ${actor.platform ? "" : "AND company_id=$2"} FOR UPDATE`,
      actor.platform ? [id] : [id, actor.companyId],
    )
  ).rows[0];
  if (!row)
    throw new ApiError(404, "ASSIGNMENT_NOT_FOUND", "Assignment not found.");
  if (
    row.user_id === actor.userId ||
    (
      await c.query("SELECT iam_same_person($1,$2,$3) same", [
        row.user_id,
        actor.userId,
        actor.companyId,
      ])
    ).rows[0].same
  )
    throw new ApiError(
      403,
      "SELF_ACCESS_CHANGE",
      "Another administrator must change your access.",
    );
  if (Number(row.version) !== version)
    fail("ASSIGNMENT_CHANGED", "This assignment changed.");
  const recovery = (
    await c.query(
      "SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 AND login_policy='emergency' AND status='active'",
      [actor.companyId, row.user_id],
    )
  ).rowCount;
  if (recovery) {
    const others = (
      await c.query(
        `SELECT 1 FROM iam_members m JOIN ${table} a ON a.user_id=m.user_id WHERE m.company_id=$1 AND m.user_id!=$2 AND m.login_policy='emergency' AND m.status='active' AND a.role=$3 AND iam_assignment_active(a.id,$4) ${actor.platform ? "" : "AND a.company_id=m.company_id"}`,
        [
          actor.companyId,
          row.user_id,
          actor.platform ? "platform_admin" : "holding_owner",
          Date.now(),
        ],
      )
    ).rowCount;
    if (!others)
      fail(
        "LAST_RECOVERY_ADMINISTRATOR",
        "Designate another verified local recovery administrator first.",
      );
  }
  if (["holding_owner", "platform_admin"].includes(row.role)) {
    const other = (
      await c.query(
        `SELECT id FROM ${table} WHERE role=$1 AND user_id!=$2 AND iam_assignment_active(id,$3) ${actor.platform ? "" : "AND company_id=$4"} FOR UPDATE`,
        actor.platform
          ? [row.role, row.user_id, Date.now()]
          : [row.role, row.user_id, Date.now(), actor.companyId],
      )
    ).rows;
    if (!other.length)
      fail(
        "LAST_ADMINISTRATOR",
        "The last recovery administrator must remain available.",
      );
  }
  if (
    !actor.platform &&
    actor.scopeKind !== "holding" &&
    row.scope_node_id !== actor.scopeNodeId
  )
    throw new ApiError(
      403,
      "SCOPE_FORBIDDEN",
      "This assignment is outside your scope.",
    );
  if (recovery && !approved) {
    await requireFreshMfa(actor);
    requirePermission(actor, iamPermission(actor, "iam.recovery"));
    return review(
      c,
      actor,
      "emergency.revoke",
      { id, version, reason },
      reason,
    );
  }
  await c.query(
    `UPDATE ${table} SET revoked_at=$1,version=version+1 WHERE id=$2`,
    [Date.now(), id],
  );
  if (!actor.platform)
    await c.query(
      "UPDATE active_work_contexts SET revision=revision+1,assignment_id=NULL,assignment_version=NULL WHERE company_id=$1 AND assignment_id=$2",
      [actor.companyId, id],
    );
  return { id, status: "revoked" };
}
export async function decideIamReview(
  db: PostgresDatabase,
  actor: IamActor,
  id: string,
  input: z.infer<typeof reviewInput>,
  key: string,
  applyExtension?: (
    c: PoolClient,
    kind: string,
    payload: Record<string, unknown>,
  ) => Promise<void>,
  rejectExtension?: (
    c: PoolClient,
    kind: string,
    payload: Record<string, unknown>,
  ) => Promise<void>,
) {
  db = iamDb(db);
  assertCompanyScope(actor);
  await requireFreshMfa(actor);
  return iamCommand(
    db,
    actor,
    key,
    "review.decide",
    { id, ...input },
    "iam.review",
    async (c) => {
      const r = (
        await c.query(
          "SELECT * FROM iam_reviews WHERE id=$1 AND company_id=$2 FOR UPDATE",
          [id, actor.companyId],
        )
      ).rows[0];
      if (!r) throw new ApiError(404, "REVIEW_NOT_FOUND", "Review not found.");
      if (r.status !== "pending" || Number(r.version) !== input.expectedVersion)
        fail("REVIEW_CHANGED", "This review changed.");
      if (
        r.created_by === actor.userId ||
        r.created_person === (actor.iam?.personId ?? actor.userId) ||
        (
          await c.query("SELECT iam_same_person($1,$2,$3) same", [
            r.created_by,
            actor.userId,
            actor.companyId,
          ])
        ).rows[0].same
      )
        throw new ApiError(
          403,
          "INDEPENDENT_REVIEW_REQUIRED",
          "A different person must review this change.",
        );
      const p = r.payload;
      if (!["holding", "platform"].includes(actor.scopeKind)) {
        const covered =
          ["assignment.grant", "mapping.activate", "policy.grant"].includes(r.kind) &&
          p.scopeKind !== "holding" &&
          p.scopeNodeId &&
          (p.scopeNodeId === actor.scopeNodeId ||
            (actor.scopeKind === "branch" &&
              (
                await c.query(
                  "SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 AND branch_id=$3",
                  [p.scopeNodeId, actor.companyId, actor.scopeNodeId],
                )
              ).rowCount));
        if (!covered)
          throw new ApiError(
            403,
            "REVIEW_OUTSIDE_SCOPE",
            "This review is outside your delegated scope.",
          );
      }
      if (input.decision === "approve") {
        const original = (
          await c.query(
            `SELECT a.role,v.grantable_json FROM ${actor.platform ? "platform_assignments" : "assignments"} a JOIN iam_roles r ON r.id=a.role AND r.company_id IN ('*',$2) AND r.status='active' JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE a.id=$1 AND a.user_id=$3 AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,$2,$4)`,
            [
              p.authorizingAssignmentId,
              actor.companyId,
              r.created_by,
              Date.now(),
            ],
          )
        ).rows[0];
        if (!original)
          fail(
            "PROPOSER_AUTHORITY_REVOKED",
            "The proposer no longer has active authority. Create a new proposal.",
          );
        const required =
          r.kind === "people.provision.grant"
            ? "iam.directory.apply"
            : r.kind === "role.publish"
            ? "iam.roles.manage"
            : ["connection.activate", "directory.operation"].includes(r.kind)
              ? "iam.directory.manage"
              : r.kind.startsWith("emergency.")
                ? "iam.recovery"
                : "iam.manage";
        if (
          !(
            await c.query("SELECT iam_role_allows($1,$2,$3) allowed", [
              original.role,
              actor.companyId,
              iamPermission(actor, required),
            ])
          ).rows[0].allowed
        )
          fail(
            "PROPOSER_AUTHORITY_REVOKED",
            "The proposer no longer has permission for this change.",
          );
        if (
          r.kind === "role.publish" &&
          [...p.permissions, ...p.grantablePermissions].some(
            (v: string) => !original.grantable_json.includes(v),
          )
        )
          fail(
            "DELEGATION_EXCEEDED",
            "The revision exceeds the original delegated permission set.",
          );

        if (r.kind === "role.publish") {
          if (
            (
              await c.query(
                `SELECT 1 FROM ${actor.platform ? "platform_assignments" : "assignments"} WHERE role=$1 AND iam_assignment_active(id,$2) AND iam_same_person(user_id,$3,$4) ${actor.platform ? "" : "AND company_id=$4"}`,
                [p.id, Date.now(), actor.userId, actor.companyId],
              )
            ).rowCount
          )
            throw new ApiError(
              403,
              "INDEPENDENT_REVIEW_REQUIRED",
              "A person affected by this role revision cannot approve it.",
            );
          const role = (
            await c.query(
              "SELECT * FROM iam_roles WHERE company_id=$1 AND id=$2 FOR UPDATE",
              [actor.companyId, p.id],
            )
          ).rows[0];
          if (
            !role ||
            role.built_in ||
            Number(role.published_revision) !== p.expectedVersion
          )
            fail(
              "ROLE_CHANGED",
              "The published role changed. Propose a new revision.",
            );
          if (
            [...p.permissions, ...p.grantablePermissions].some(
              (v: string) => !actor.iam?.grantablePermissions.includes(v),
            )
          )
            throw new ApiError(
              403,
              "DELEGATION_EXCEEDED",
              "This revision exceeds your review authority.",
            );
          await c.query(
            "UPDATE iam_roles SET published_revision=$1,name=$2 WHERE company_id=$3 AND id=$4",
            [p.revision, p.name, actor.companyId, p.id],
          );
          const table = actor.platform ? "platform_assignments" : "assignments";
          await c.query(
            `UPDATE ${table} SET version=version+1 WHERE role=$1 ${actor.platform ? "" : "AND company_id=$2"}`,
            actor.platform ? [p.id] : [p.id, actor.companyId],
          );
          if (!actor.platform)
            await c.query(
              "UPDATE active_work_contexts SET assignment_id=NULL,assignment_version=NULL,revision=revision+1 WHERE company_id=$1 AND assignment_id IN (SELECT id FROM assignments WHERE role=$2 AND company_id=$1)",
              [actor.companyId, p.id],
            );
        } else if (r.kind === "emergency.revoke") {
          requirePermission(actor, iamPermission(actor, "iam.recovery"));
          await applyIamRevocation(c, actor, p.id, p.version, p.reason, true);
        } else if (r.kind === "assignment.grant") {
          const grant = grantInput.parse(p);
          await validateIamGrant(c, actor, grant, p.parentId);
          await insertIamGrant(c, actor, grant, p.parentId);
        } else if (applyExtension) await applyExtension(c, r.kind, p);
        else
          fail(
            "REVIEW_UNSUPPORTED",
            "This review requires its registered workflow handler.",
          );
      }
      if (input.decision === "reject" && rejectExtension)
        await rejectExtension(c, r.kind, p);
      await c.query(
        "UPDATE iam_reviews SET status=$1,reviewed_by=$2,reviewed_at=$3,reason=reason||$4,version=version+1 WHERE id=$5",
        [
          input.decision === "approve" ? "approved" : "rejected",
          actor.userId,
          Date.now(),
          `\nReview: ${input.reason}`,
          id,
        ],
      );
      return {
        id,
        status: input.decision === "approve" ? "approved" : "rejected",
      };
    },
  );
}
export { review as proposeIamReview };
