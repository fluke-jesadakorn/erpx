import "server-only";
import { z } from "zod";
import { iamDb } from "./iam-db";
import { requireFreshMfa } from "./iam-mfa";
import { runtimeEnv, holdingResources } from "./runtime";
import { holdingBindingBySlug } from "./runtime";
import { ApiError } from "./http";
import type { PostgresDatabase } from "./runtime/postgres";
export const recoveryInput = z.object({
  holdingSlug: z.string().optional(),
  companyId: z.string().min(1),
  reason: z.string().trim().min(10).max(1000),
});
export async function activateEmergency(
  actor: { userId: string; sessionId: string },
  input: z.infer<typeof recoveryInput>,
) {
  await requireFreshMfa(actor);
  const holding = input.holdingSlug
    ? await holdingBindingBySlug(input.holdingSlug)
    : null;
  if (!holding && input.companyId !== "platform")
    throw new ApiError(
      422,
      "WORKSPACE_REQUIRED",
      "Choose the company and holding for emergency access.",
    );
  const db = iamDb(
      (holding
        ? (await holdingResources(holding.id)).write
        : (await runtimeEnv()).CONTROL_DB) as PostgresDatabase,
    ),
    c = await db.pool.connect(),
    now = Date.now();
  try {
    await c.query("BEGIN ISOLATION LEVEL SERIALIZABLE");
    const member = (
      await c.query(
        "SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 AND status='active' AND login_policy='emergency' FOR SHARE",
        [input.companyId, actor.userId],
      )
    ).rowCount;
    const grant = (
      await c.query(
        `SELECT valid_until FROM ${holding ? "assignments" : "platform_assignments"} WHERE user_id=$1 AND role=$2 AND iam_assignment_active(id,$3) ${holding ? "AND company_id=$4" : ""} ORDER BY valid_until DESC LIMIT 1 FOR SHARE`,
        holding
          ? [actor.userId, "holding_owner", now, input.companyId]
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
    await c.query(
      "INSERT INTO iam_emergency_sessions VALUES($1,$2,$3,$4,$5) ON CONFLICT(session_id,company_id) DO UPDATE SET expires_at=excluded.expires_at,reason=excluded.reason",
      [actor.sessionId, input.companyId, actor.userId, expiresAt, input.reason],
    );
    await c.query("INSERT INTO iam_audit VALUES($1,$2,$3,$4,$5,$6,$7,$8)", [
      crypto.randomUUID(),
      input.companyId,
      actor.userId,
      "recovery",
      "emergency.activated",
      actor.sessionId,
      JSON.stringify({ reason: input.reason, expiresAt }),
      now,
    ]);
    await c.query("COMMIT");
    return {
      expiresAt,
      message: "Emergency access activated for up to one hour.",
    };
  } catch (e) {
    await c.query("ROLLBACK");
    throw e;
  } finally {
    c.release();
  }
}
