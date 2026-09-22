import { iamDb } from "./iam-db";
import "server-only";
import type { PermissionActor, RoleRevision } from "@/lib/iam";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { ApiError } from "./http";

export async function iamRole(
  db: SqlDatabase,
  companyId: string,
  id: string,
): Promise<RoleRevision | null> {
  db = iamDb(db);
  const r = await db
    .prepare(
      `SELECT r.*,v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id IN ('*',?) AND r.id=? AND r.status='active'`,
    )
    .bind(companyId, id)
    .first<{
      id: string;
      name: string;
      company_id: string;
      published_revision: number;
      built_in: boolean;
      permissions_json: string[];
      grantable_json: string[];
      scopes_json: string[];
      status: string;
    }>();
  return r
    ? {
        id: r.id,
        name: r.name,
        companyId: r.company_id,
        revision: r.published_revision,
        builtIn: r.built_in,
        permissions: r.permissions_json,
        grantablePermissions: r.grantable_json,
        scopes: r.scopes_json,
        status: r.status,
      }
    : null;
}
/** Published definitions for a set of assignments, read afresh for this request. */
export async function iamRoles(db: SqlDatabase, companyId: string, ids: readonly string[]): Promise<Map<string,RoleRevision>> {
  const unique = [...new Set(ids)];
  if (!unique.length) return new Map();
  const rows = (await iamDb(db).prepare(`SELECT r.id,r.name,r.company_id,r.published_revision,r.built_in,r.status,
    v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r
    JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
    WHERE r.company_id IN ('*',?) AND r.id IN (${unique.map(()=>"?").join(",")}) AND r.status='active'`)
    .bind(companyId,...unique).all<{id:string;name:string;company_id:string;published_revision:number;built_in:boolean;status:string;permissions_json:string[];grantable_json:string[];scopes_json:string[]}>()).results;
  const roles = new Map<string,RoleRevision>();
  for (const r of rows) if (!roles.has(r.id)) roles.set(r.id,{
    id:r.id,name:r.name,companyId:r.company_id,revision:r.published_revision,builtIn:r.built_in,
    permissions:r.permissions_json,grantablePermissions:r.grantable_json,scopes:r.scopes_json,status:r.status,
  });
  return roles;
}

export async function loadIamAuthority(
  db: SqlDatabase,
  companyId: string,
  userId: string,
  roleId: string,
  sessionId?: string,
): Promise<NonNullable<PermissionActor["iam"]>> {
  db = iamDb(db);
  const [role, member] = await Promise.all([
    iamRole(db, companyId, roleId),
    db
      .prepare(
        `SELECT m.*,i.enabled,i.observed_at,c.status connection_status FROM iam_members m LEFT JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id LEFT JOIN iam_connections c ON c.id=m.connection_id AND c.company_id=m.company_id WHERE m.company_id=? AND m.user_id=?`,
      )
      .bind(companyId, userId)
      .first<{
        status: string;
        login_policy: string;
        version: number;
        person_id: string | null;
        principal_id: string;
        enabled: boolean;
        observed_at: number;
        connection_status: string;
      }>(),
  ]);
  if (!role)
    throw new ApiError(403, "ROLE_UNAVAILABLE", "This role is unavailable.");
  if (member && member.status !== "active")
    throw new ApiError(
      403,
      "IDENTITY_SUSPENDED",
      "This identity no longer has access to this company.",
    );
  if (
    member?.login_policy === "directory" &&
    (!member.enabled ||
      member.connection_status !== "active" ||
      member.observed_at <= Date.now() - 300_000)
  )
    throw new ApiError(
      403,
      "DIRECTORY_ACCESS_UNAVAILABLE",
      "Your directory access needs a successful refresh. Contact your administrator.",
    );
  const personId =
    member?.person_id ??
    (companyId === "platform"
      ? null
      : ((
          await db
            .prepare(
              "SELECT id FROM hr_people WHERE company_id=? AND user_id=? AND merged_into_id IS NULL",
            )
            .bind(companyId, userId)
            .first<{ id: string }>()
        )?.id ?? null));
  const aliases = personId
    ? (
        await db
          .prepare(
            "SELECT user_id FROM iam_members WHERE company_id=? AND person_id=?",
          )
          .bind(companyId, personId)
          .all<{ user_id: string }>()
      ).results.map((r) => r.user_id)
    : [];
  let emergencyUntil: number | null = null;
  if (member?.login_policy === "emergency") {
    const elevation = await db
      .prepare(
        "SELECT expires_at FROM iam_emergency_sessions WHERE session_id=? AND user_id=? AND company_id=? AND expires_at>?",
      )
      .bind(sessionId ?? "", userId, companyId, Date.now())
      .first<{ expires_at: number }>();
    if (!elevation)
      throw new ApiError(
        403,
        "EMERGENCY_ACCESS_REQUIRED",
        "Verify MFA and activate a temporary emergency session in account security.",
      );
    emergencyUntil = Number(elevation.expires_at);
  }
  return {
    personUserIds: [...new Set([userId, ...aliases])],
    emergencyUntil,
    roleRevision: role.revision,
    identityVersion: member?.version ?? 0,
    permissions: role.permissions,
    grantablePermissions: role.grantablePermissions,
    principalId: member?.principal_id ?? `local:${userId}`,
    personId,
    freshUntil:
      member?.login_policy === "directory"
        ? Number(member.observed_at) + 300_000
        : null,
  };
}
/** Rechecked in the same transaction as the business mutation. */
export function iamMutationGuards(
  db: SqlDatabase,
  actor: PermissionActor & {
    userId: string;
    assignmentId: string;
    companyId: string;
    authority?: { sessionId: string };
  },
  key: string,
): SqlStatement[] {
  db = iamDb(db);
  const iam = actor.iam;
  return [
    db
      .prepare(
        `INSERT INTO command_assertions(command_id,step,ok) VALUES(?,'iam-current-authority',CASE WHEN
 NOT EXISTS(SELECT 1 FROM iam_revoked_sessions WHERE session_id=? AND company_id=? AND user_id=?)
 AND (?::integer IS NULL OR EXISTS(SELECT 1 FROM assignments WHERE id=? AND version=?))
 AND iam_assignment_active(?,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND NOT EXISTS(SELECT 1 FROM iam_members m LEFT JOIN iam_identities i ON i.id=m.identity_id LEFT JOIN iam_connections c ON c.id=m.connection_id WHERE m.company_id=? AND m.user_id=? AND (m.status!='active' OR (?::integer IS NOT NULL AND m.version!=?) OR (m.login_policy='directory' AND (i.enabled IS DISTINCT FROM true OR c.status IS DISTINCT FROM 'active' OR i.observed_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint-300000))))
 AND (?::bigint IS NULL OR ?::bigint>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND (?::integer IS NULL OR COALESCE((SELECT version FROM iam_members WHERE company_id=? AND user_id=?),0)=?)
 AND EXISTS(SELECT 1 FROM iam_roles WHERE id=? AND company_id IN ('*',?) AND status='active' AND (?::integer IS NULL OR published_revision=?))
 THEN 1 ELSE 0 END)`,
      )
      .bind(
        key,
        actor.authority?.sessionId ?? null,
        actor.companyId,
        actor.userId,
        iam?.queuedAssignmentVersion ?? null,
        actor.assignmentId,
        iam?.queuedAssignmentVersion ?? null,
        actor.assignmentId,
        actor.companyId,
        actor.userId,
        iam?.identityVersion ?? null,
        iam?.identityVersion ?? null,
        iam?.emergencyUntil ?? null,
        iam?.emergencyUntil ?? null,
        iam?.identityVersion ?? null,
        actor.companyId,
        actor.userId,
        iam?.identityVersion ?? null,
        actor.role,
        actor.companyId,
        iam?.roleRevision ?? null,
        iam?.roleRevision ?? null,
      ),
  ];
}
