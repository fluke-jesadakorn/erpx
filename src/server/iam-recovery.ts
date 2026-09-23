import "server-only";
import { createHash } from "node:crypto";
import type { PoolClient } from "pg";
import { z } from "zod";
import { iamDb } from "./iam-db";
import { requireFreshMfa } from "./iam-mfa";
import {
  runtimeEnv,
  activeHoldingBindings,
  holdingResources,
  holdingBindingBySlug,
} from "./runtime";
import { ApiError } from "./http";
import type { PostgresDatabase } from "./runtime/postgres";

export const recoveryInput = z.object({
  holdingSlug: z.string().optional(),
  companyId: z.string().min(1),
  reason: z.string().trim().min(10).max(1000),
});

export const emergencyRevocationInput = recoveryInput;

type EmergencyActor = { userId: string; sessionId: string };
type RecoveryScope = {
  companyId: string;
  holdingSlug: string | null;
  scopeLabel: string;
  db: PostgresDatabase;
};

function requireIdempotencyKey(key: string) {
  if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))
    throw new ApiError(
      422,
      "COMMAND_KEY_REQUIRED",
      "A valid Idempotency-Key is required.",
    );
}

async function recoveryScope(input: z.infer<typeof recoveryInput>): Promise<RecoveryScope> {
  const holding = input.holdingSlug
    ? await holdingBindingBySlug(input.holdingSlug)
    : null;
  if (!holding && input.companyId !== "platform")
    throw new ApiError(
      422,
      "WORKSPACE_REQUIRED",
      "Choose the company and holding for emergency access.",
    );
  const env = await runtimeEnv();
  return {
    companyId: input.companyId,
    holdingSlug: holding?.slug ?? null,
    scopeLabel: holding
      ? `${holding.displayName} · ${input.companyId}`
      : "Platform administration",
    db: (holding
      ? (await holdingResources(holding.id)).write
      : env.CONTROL_DB) as PostgresDatabase,
  };
}

function requestHash(
  actor: EmergencyActor,
  scope: RecoveryScope,
  operation: string,
  input: z.infer<typeof recoveryInput>,
) {
  return createHash("sha256")
    .update(
      JSON.stringify({
        actor: actor.userId,
        session: actor.sessionId,
        company: scope.companyId,
        holding: scope.holdingSlug,
        operation,
        input,
      }),
    )
    .digest("hex");
}

async function beginIdempotent(
  client: PoolClient,
  key: string,
  actor: EmergencyActor,
  scope: RecoveryScope,
  hash: string,
) {
  // Take a session lock before opening the serializable snapshot. Retries for
  // the same command therefore see the prior receipt after the first commit.
  await client.query("SELECT pg_advisory_lock(hashtextextended($1,0))", [
    `${scope.companyId}:${actor.userId}:${key}`,
  ]);
  await client.query("BEGIN ISOLATION LEVEL SERIALIZABLE");
  const receipt = (
    await client.query(
      "SELECT company_id,actor_id,request_hash,result FROM iam_receipts WHERE id=$1 FOR UPDATE",
      [key],
    )
  ).rows[0];
  if (!receipt) return null;
  if (
    receipt.company_id !== scope.companyId ||
    receipt.actor_id !== actor.userId ||
    receipt.request_hash !== hash
  )
    throw new ApiError(
      409,
      "IDEMPOTENCY_CONFLICT",
      "This request key was already used for a different emergency access action.",
    );
  return receipt.result as Record<string, unknown>;
}

async function releaseIdempotencyLock(
  client: PoolClient,
  key: string,
  actor: EmergencyActor,
  scope: RecoveryScope,
) {
  await client
    .query("SELECT pg_advisory_unlock(hashtextextended($1,0))", [
      `${scope.companyId}:${actor.userId}:${key}`,
    ])
    .catch(() => undefined);
}

async function saveReceipt(
  client: PoolClient,
  key: string,
  actor: EmergencyActor,
  scope: RecoveryScope,
  hash: string,
  result: Record<string, unknown>,
  now: number,
) {
  await client.query("INSERT INTO iam_receipts VALUES($1,$2,$3,$4,$5,$6)", [
    key,
    scope.companyId,
    actor.userId,
    hash,
    JSON.stringify(result),
    now,
  ]);
}

export async function activateEmergency(
  actor: EmergencyActor,
  input: z.infer<typeof recoveryInput>,
  key: string,
) {
  requireIdempotencyKey(key);
  const scope = await recoveryScope(input),
    db = iamDb(scope.db),
    client = await db.pool.connect(),
    now = Date.now(),
    hash = requestHash(actor, scope, "emergency.activate", input);
  try {
    const replay = await beginIdempotent(client, key, actor, scope, hash);
    if (replay) {
      await client.query("COMMIT");
      return replay;
    }
    await requireFreshMfa(actor);
    const member = (
      await client.query(
        "SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 AND status='active' AND login_policy='emergency' FOR SHARE",
        [scope.companyId, actor.userId],
      )
    ).rowCount;
    const grant = (
      await client.query(
        `SELECT valid_until FROM ${scope.holdingSlug ? "assignments" : "platform_assignments"} WHERE user_id=$1 AND role=$2 AND iam_assignment_active(id,$3) ${scope.holdingSlug ? "AND company_id=$4" : ""} ORDER BY valid_until DESC LIMIT 1 FOR SHARE`,
        scope.holdingSlug
          ? [actor.userId, "holding_owner", now, scope.companyId]
          : [actor.userId, "platform_admin", now],
      )
    ).rows[0];
    if (!member || !grant)
      throw new ApiError(
        403,
        "RECOVERY_UNAVAILABLE",
        "This account is not a designated recovery administrator in this workspace.",
      );
    const expiresAt = Math.min(now + 3600000, Number(grant.valid_until));
    await client.query(
      "INSERT INTO iam_emergency_sessions VALUES($1,$2,$3,$4,$5) ON CONFLICT(session_id,company_id) DO UPDATE SET expires_at=excluded.expires_at,reason=excluded.reason",
      [actor.sessionId, scope.companyId, actor.userId, expiresAt, input.reason],
    );
    const result = {
      expiresAt,
      message: "Emergency access activated for up to one hour.",
    };
    await client.query("INSERT INTO iam_audit VALUES($1,$2,$3,$4,$5,$6,$7,$8)", [
      crypto.randomUUID(),
      scope.companyId,
      actor.userId,
      "recovery",
      "emergency.activated",
      actor.sessionId,
      JSON.stringify({ reason: input.reason, expiresAt }),
      now,
    ]);
    await saveReceipt(client, key, actor, scope, hash, result, now);
    await client.query("COMMIT");
    return result;
  } catch (e) {
    await client.query("ROLLBACK");
    throw e;
  } finally {
    await releaseIdempotencyLock(client, key, actor, scope);
    client.release();
  }
}

export async function revokeEmergency(
  actor: EmergencyActor,
  input: z.infer<typeof emergencyRevocationInput>,
  key: string,
) {
  requireIdempotencyKey(key);
  const scope = await recoveryScope(input),
    db = iamDb(scope.db),
    client = await db.pool.connect(),
    now = Date.now(),
    hash = requestHash(actor, scope, "emergency.revoke", input);
  try {
    const replay = await beginIdempotent(client, key, actor, scope, hash);
    if (replay) {
      await client.query("COMMIT");
      return replay;
    }
    const deleted = await client.query(
        "DELETE FROM iam_emergency_sessions WHERE session_id=$1 AND user_id=$2 AND company_id=$3 AND expires_at>$4 RETURNING session_id",
        [actor.sessionId, actor.userId, scope.companyId, now],
      ),
      revoked = (deleted.rowCount ?? 0) > 0;
    if (!revoked)
      throw new ApiError(
        404,
        "EMERGENCY_ACCESS_NOT_ACTIVE",
        "No active temporary access exists for this session and workspace.",
      );
    const result = {
      revoked: true,
      message: "Temporary emergency access was revoked for this session.",
    };
    await client.query("INSERT INTO iam_audit VALUES($1,$2,$3,$4,$5,$6,$7,$8)", [
      crypto.randomUUID(),
      scope.companyId,
      actor.userId,
      "recovery",
      "emergency.revoked",
      actor.sessionId,
      JSON.stringify({ reason: input.reason, revoked }),
      now,
    ]);
    await saveReceipt(client, key, actor, scope, hash, result, now);
    await client.query("COMMIT");
    return result;
  } catch (e) {
    await client.query("ROLLBACK");
    throw e;
  } finally {
    await releaseIdempotencyLock(client, key, actor, scope);
    client.release();
  }
}

export async function listEmergencyAccess(actor: EmergencyActor) {
  const env = await runtimeEnv(),
    now = Date.now(),
    access: Array<{
      companyId: string;
      holdingSlug: string | null;
      scopeLabel: string;
      reason: string;
      expiresAt: number;
    }> = [],
    unavailableScopes: string[] = [];
  const platform: RecoveryScope = {
    companyId: "platform",
    holdingSlug: null,
    scopeLabel: "Platform administration",
    db: env.CONTROL_DB as PostgresDatabase,
  };
  const bindings = await activeHoldingBindings();
  const scopes: RecoveryScope[] = [platform];
  for (const holding of bindings) {
    try {
      scopes.push({
        companyId: "*",
        holdingSlug: holding.slug,
        scopeLabel: holding.displayName,
        db: (await holdingResources(holding.id)).write as PostgresDatabase,
      });
    } catch {
      unavailableScopes.push(holding.displayName);
    }
  }
  for (const scope of scopes) {
    try {
      const rows = (
        await iamDb(scope.db)
          .prepare(
            "SELECT company_id,expires_at,reason FROM iam_emergency_sessions WHERE session_id=? AND user_id=? AND expires_at>? ORDER BY company_id",
          )
          .bind(actor.sessionId, actor.userId, now)
          .all<{ company_id: string; expires_at: number; reason: string }>()
      ).results;
      for (const row of rows)
        access.push({
          companyId: row.company_id,
          holdingSlug: scope.holdingSlug,
          scopeLabel:
            scope.holdingSlug === null
              ? scope.scopeLabel
              : `${scope.scopeLabel} · ${row.company_id}`,
          reason: row.reason,
          expiresAt: Number(row.expires_at),
        });
    } catch {
      if (scope.holdingSlug) unavailableScopes.push(scope.scopeLabel);
      else throw new ApiError(
        503,
        "RECOVERY_STATUS_UNAVAILABLE",
        "Temporary access status is currently unavailable. Try again shortly.",
      );
    }
  }
  return { access, unavailableScopes };
}
