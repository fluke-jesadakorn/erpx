import { requireFreshMfa } from "./iam-mfa";
import { iamDb } from "./iam-db";
import "server-only";
import { createHash } from "node:crypto";
import type { PoolClient } from "pg";
import type { PostgresDatabase } from "./runtime/postgres";
import { ApiError } from "./http";
import { iamPermission, type IamActor } from "./iam-context";

export async function iamCommand<T>(
  db: PostgresDatabase,
  actor: IamActor,
  key: string,
  operation: string,
  payload: unknown,
  permission: string,
  action: (client: PoolClient) => Promise<T>,
): Promise<T> {
  db = iamDb(db);
  if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))
    throw new ApiError(
      422,
      "COMMAND_KEY_REQUIRED",
      "A valid Idempotency-Key is required.",
    );
  const digest = createHash("sha256")
    .update(
      JSON.stringify({
        actor: actor.userId,
        assignment: actor.assignmentId,
        company: actor.companyId,
        operation,
        payload,
      }),
    )
    .digest("hex");
  const client = await db.pool.connect();
  try {
    await client.query("BEGIN ISOLATION LEVEL SERIALIZABLE");
    const table = actor.platform ? "platform_assignments" : "assignments";
    const assignment = (
      await client.query(
        `SELECT * FROM ${table} WHERE id=$1 AND user_id=$2 ${actor.platform ? "" : "AND company_id=$3"} FOR SHARE`,
        actor.platform
          ? [actor.assignmentId, actor.userId]
          : [actor.assignmentId, actor.userId, actor.companyId],
      )
    ).rows[0];
    if (
      !assignment ||
      !(
        await client.query("SELECT iam_assignment_active($1,$2) ok", [
          actor.assignmentId,
          Date.now(),
        ])
      ).rows[0].ok
    )
      throw new ApiError(
        403,
        "ASSIGNMENT_UNAVAILABLE",
        "Your authorizing assignment is no longer active.",
      );
    if (
      !(
        await client.query("SELECT iam_role_allows($1,$2,$3) ok", [
          assignment.role,
          actor.companyId,
          iamPermission(actor, permission),
        ])
      ).rows[0].ok
    )
      throw new ApiError(
        403,
        "PERMISSION_REQUIRED",
        "Your selected role does not grant this operation.",
      );
    const policy = (
      await client.query(
        "SELECT r.published_revision,v.grantable_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.id=$1 AND r.company_id IN ('*',$2) AND r.status='active' FOR SHARE OF r",
        [assignment.role, actor.companyId],
      )
    ).rows[0];
    if (
      !policy ||
      (actor.iam &&
        Number(policy.published_revision) !== actor.iam.roleRevision) ||
      assignment.role !== actor.role
    )
      throw new ApiError(
        409,
        "POLICY_CHANGED",
        "The role policy changed. Refresh and review again.",
      );
    actor = {
      ...actor,
      iam: actor.iam
        ? { ...actor.iam, grantablePermissions: policy.grantable_json }
        : undefined,
    };
    const member = (
      await client.query(
        "SELECT * FROM iam_members WHERE company_id=$1 AND user_id=$2 FOR SHARE",
        [actor.companyId, actor.userId],
      )
    ).rows[0];
    if (actor.iam && Number(member?.version ?? 0) !== actor.iam.identityVersion)
      throw new ApiError(
        409,
        "IDENTITY_CHANGED",
        "Your identity changed. Refresh first.",
      );
    if (member && member.status !== "active")
      throw new ApiError(
        403,
        "IDENTITY_SUSPENDED",
        "Your identity is inactive.",
      );
    if (member?.login_policy === "directory") {
      const identity = (
        await client.query(
          "SELECT i.enabled,i.observed_at,c.status FROM iam_identities i JOIN iam_connections c ON c.id=i.connection_id WHERE i.id=$1 AND i.user_id=$2 AND i.company_id=$3",
          [member.identity_id, actor.userId, actor.companyId],
        )
      ).rows[0];
      if (
        !identity?.enabled ||
        identity.status !== "active" ||
        Number(identity.observed_at) <= Date.now() - 300_000
      )
        throw new ApiError(
          403,
          "DIRECTORY_ACCESS_UNAVAILABLE",
          "Directory access requires a successful refresh.",
        );
    }
    if (
      (
        await client.query(
          "SELECT 1 FROM iam_revoked_sessions WHERE session_id=$1 AND company_id=$2 AND user_id=$3",
          [actor.sessionId, actor.companyId, actor.userId],
        )
      ).rowCount
    )
      throw new ApiError(401, "SESSION_REVOKED", "Sign in again to continue.");
    if (
      member?.login_policy === "emergency" &&
      (!actor.iam?.emergencyUntil || actor.iam.emergencyUntil <= Date.now())
    )
      throw new ApiError(
        403,
        "EMERGENCY_ACCESS_REQUIRED",
        "Activate a temporary emergency session.",
      );
    if (!actor.platform) {
      const context = (
        await client.query(
          "SELECT * FROM active_work_contexts WHERE session_id=$1 AND company_id=$2 FOR UPDATE",
          [actor.sessionId, actor.companyId],
        )
      ).rows[0];
      if (
        !context ||
        (actor.authority &&
          Number(context.revision) !== actor.authority.revision) ||
        context.assignment_id !== actor.assignmentId ||
        Number(context.assignment_version) !== Number(assignment.version)
      )
        throw new ApiError(
          409,
          "WORK_CONTEXT_CHANGED",
          "Choose your current work role again.",
        );
    }
    const old = (
      await client.query("SELECT * FROM iam_receipts WHERE id=$1", [key])
    ).rows[0];
    if (old) {
      if (old.request_hash !== digest)
        throw new ApiError(
          409,
          "IDEMPOTENCY_CONFLICT",
          "This key belongs to another request.",
        );
      await client.query("COMMIT");
      return old.result as T;
    }
    const recoveryQuery = `SELECT 1 FROM iam_members m JOIN ${table} a ON a.user_id=m.user_id ${actor.platform ? "" : "AND a.company_id=m.company_id"} WHERE m.company_id=$1 AND m.status='active' AND m.login_policy='emergency' AND a.role=$2 AND iam_assignment_active(a.id,$3) LIMIT 1`;
    const hadRecovery = (
      await client.query(recoveryQuery, [
        actor.companyId,
        actor.platform ? "platform_admin" : "holding_owner",
        Date.now(),
      ])
    ).rowCount;
    const result = await action(client);
    if (
      hadRecovery &&
      !(
        await client.query(recoveryQuery, [
          actor.companyId,
          actor.platform ? "platform_admin" : "holding_owner",
          Date.now(),
        ])
      ).rowCount
    )
      throw new ApiError(
        409,
        "LAST_RECOVERY_ADMINISTRATOR",
        "The final recovery administrator must retain active access.",
      );
    if (
      /^(role\.propose|assignment\.propose|migration\.prepare|mapping\.propose|connection\.activate\.propose|review\.decide|emergency\.|identity\.restore\.propose|invitation\.propose|beta\.invite)/.test(
        operation,
      )
    )
      await requireFreshMfa(actor);
    await client.query("INSERT INTO iam_receipts VALUES($1,$2,$3,$4,$5,$6)", [
      key,
      actor.companyId,
      actor.userId,
      digest,
      JSON.stringify(result),
      Date.now(),
    ]);
    await client.query(
      "INSERT INTO iam_audit VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
      [
        crypto.randomUUID(),
        actor.companyId,
        actor.userId,
        actor.assignmentId,
        operation,
        key,
        JSON.stringify({
          input: payload,
          result,
          role: actor.role,
          scope: actor.scopeKind,
          roleRevision: actor.iam?.roleRevision,
        }),
        Date.now(),
      ],
    );
    await client.query("COMMIT");
    return result;
  } catch (error) {
    await client.query("ROLLBACK");
    if (
      ["40001", "40P01", "23505"].includes(
        (error as { code?: string }).code ?? "",
      )
    )
      throw new ApiError(
        409,
        "IAM_CONFLICT",
        "Access changed. Refresh and review before retrying.",
      );
    throw error;
  } finally {
    client.release();
  }
}
