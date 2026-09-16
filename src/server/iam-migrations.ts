import { iamDb } from "./iam-db";
import "server-only";
import { createHash } from "node:crypto";
import { z } from "zod";
import type { PoolClient } from "pg";
import { ApiError } from "./http";
import { runtimeEnv } from "./runtime";
import type { PostgresDatabase } from "./runtime/postgres";
import { assertCompanyScope, type IamActor } from "./iam-context";
import { requireFreshMfa } from "./iam-mfa";
import { iamCommand } from "./iam-transactions";
import {
  grantInput,
  insertIamGrant,
  proposeIamReview,
  validateIamGrant,
} from "./iam-service";

export const migrationInput = z.object({
  connectionId: z.string(),
  users: z
    .array(
      z.object({
        localUserId: z.string(),
        identityId: z.string(),
        personId: z.string(),
        grants: z.array(grantInput.omit({ userId: true })).min(1),
      }),
    )
    .min(1)
    .max(200),
  reason: z.string().trim().min(3).max(1000),
});
export async function prepareMigration(
  db: PostgresDatabase,
  actor: IamActor,
  input: z.infer<typeof migrationInput>,
  key: string,
) {
  db = iamDb(db);
  assertCompanyScope(actor);
  await requireFreshMfa(actor);
  if (actor.platform)
    throw new ApiError(
      422,
      "PLATFORM_MIGRATION_UNAVAILABLE",
      "Customer employee migrations use a company workspace.",
    );
  if (
    new Set(input.users.map((u) => u.localUserId)).size !==
      input.users.length ||
    new Set(input.users.map((u) => u.identityId)).size !== input.users.length
  )
    throw new ApiError(
      422,
      "DUPLICATE_MIGRATION_MEMBER",
      "Each account and directory identity can occur once per batch.",
    );
  return iamCommand(
    db,
    actor,
    key,
    "migration.prepare",
    input,
    "iam.manage",
    async (c) => {
      const connection = (
        await c.query(
          "SELECT * FROM iam_connections WHERE id=$1 AND company_id=$2 AND status='active' AND last_sync_at>$3",
          [input.connectionId, actor.companyId, Date.now() - 300_000],
        )
      ).rows[0];
      if (!connection)
        throw new ApiError(
          409,
          "DIRECTORY_NOT_READY",
          "Activate and synchronize the directory first.",
        );
      for (const user of input.users) {
        const person = (
          await c.query(
            "SELECT id FROM hr_people WHERE id=$1 AND company_id=$2 AND user_id=$3 AND status='active'",
            [user.personId, actor.companyId, user.localUserId],
          )
        ).rows[0];
        const identity = (
          await c.query(
            "SELECT id FROM iam_identities WHERE id=$1 AND company_id=$2 AND connection_id=$3 AND enabled AND user_id IS NULL",
            [user.identityId, actor.companyId, input.connectionId],
          )
        ).rows[0];
        if (!person || !identity)
          throw new ApiError(
            409,
            "IDENTITY_LINK_INVALID",
            "Choose an active employee and an unlinked directory identity.",
          );
        if (
          user.localUserId === actor.userId ||
          (
            await c.query("SELECT iam_same_person($1,$2,$3) same", [
              user.localUserId,
              actor.userId,
              actor.companyId,
            ])
          ).rows[0].same
        )
          throw new ApiError(
            403,
            "SELF_ACCESS_CHANGE",
            "Another administrator must migrate your identity.",
          );
        const protectedAccount = (
          await c.query(
            "SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 AND (login_policy IN ('emergency','directory') OR status!='active')",
            [actor.companyId, user.localUserId],
          )
        ).rowCount;
        const platform = await (
          await runtimeEnv()
        ).CONTROL_DB.prepare(
          "SELECT 1 FROM platform_assignments WHERE user_id=? AND revoked_at IS NULL",
        )
          .bind(user.localUserId)
          .first();
        if (protectedAccount || platform)
          throw new ApiError(
            403,
            "PROTECTED_IDENTITY",
            "Emergency and platform identities cannot be replaced.",
          );
        for (const grant of user.grants) {
          const normalized = grantInput.parse({
            ...grant,
            userId: user.localUserId,
          });
          await validateIamGrant(c, actor, normalized);
          grant.validUntil = normalized.validUntil;
          grant.expectedRoleRevision = normalized.expectedRoleRevision;
        }
      }
      const id = crypto.randomUUID();
      await c.query(
        "INSERT INTO iam_migrations(id,company_id,connection_id,users_json,status,created_by,created_at) VALUES($1,$2,$3,$4,'awaiting_review',$5,$6)",
        [
          id,
          actor.companyId,
          input.connectionId,
          JSON.stringify(input.users),
          actor.userId,
          Date.now(),
        ],
      );
      return {
        ...(await proposeIamReview(
          c,
          actor,
          "migration.activate",
          { id, version: 1, parentId: actor.assignmentId },
          input.reason,
        )),
        batchId: id,
      };
    },
  );
}
export async function applyMigration(
  c: PoolClient,
  actor: IamActor,
  p: Record<string, unknown>,
) {
  const batch = (
    await c.query(
      "SELECT * FROM iam_migrations WHERE id=$1 AND company_id=$2 FOR UPDATE",
      [p.id, actor.companyId],
    )
  ).rows[0];
  if (
    !batch ||
    batch.status !== "awaiting_review" ||
    Number(batch.version) !== p.version
  )
    throw new ApiError(
      409,
      "MIGRATION_CHANGED",
      "The migration batch changed.",
    );
  const connection = (
    await c.query(
      "SELECT * FROM iam_connections WHERE id=$1 AND company_id=$2 AND status='active' AND last_sync_at>$3 FOR SHARE",
      [batch.connection_id, actor.companyId, Date.now() - 300_000],
    )
  ).rows[0];
  if (!connection)
    throw new ApiError(
      409,
      "DIRECTORY_NOT_READY",
      "Refresh the active directory before migrating.",
    );
  const emergency = (
    await c.query(
      "SELECT m.user_id FROM iam_members m JOIN assignments a ON a.user_id=m.user_id AND a.company_id=m.company_id WHERE m.company_id=$1 AND m.login_policy='emergency' AND m.status='active' AND a.role='holding_owner' AND iam_assignment_active(a.id,$2)",
      [actor.companyId, Date.now()],
    )
  ).rows;
  const env = await runtimeEnv();
  let ready = false;
  for (const row of emergency) {
    if (await localRecoveryReady(row.user_id)) ready = true;
  }
  if (!ready)
    throw new ApiError(
      409,
      "RECOVERY_NOT_READY",
      "An active local recovery administrator with an enrolled authenticator is required.",
    );
  const users = migrationInput.shape.users.parse(batch.users_json);
  for (const u of users) {
    if (
      u.localUserId === actor.userId ||
      (
        await c.query("SELECT iam_same_person($1,$2,$3) same", [
          u.localUserId,
          actor.userId,
          actor.companyId,
        ])
      ).rows[0].same
    )
      throw new ApiError(
        403,
        "SELF_REVIEW_FORBIDDEN",
        "The target employee cannot approve this migration.",
      );
    const person = (
      await c.query(
        "SELECT * FROM hr_people WHERE id=$1 AND company_id=$2 AND user_id=$3 FOR UPDATE",
        [u.personId, actor.companyId, u.localUserId],
      )
    ).rows[0];
    const identity = (
      await c.query(
        "SELECT * FROM iam_identities WHERE id=$1 AND company_id=$2 AND connection_id=$3 AND enabled AND user_id IS NULL FOR UPDATE",
        [u.identityId, actor.companyId, connection.id],
      )
    ).rows[0];
    if (!person || person.status !== "active" || !identity)
      throw new ApiError(
        409,
        "IDENTITY_LINK_CHANGED",
        "The employee or directory identity changed.",
      );
    if (
      (
        await c.query(
          "SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 AND (login_policy IN ('emergency','directory') OR status!='active')",
          [actor.companyId, u.localUserId],
        )
      ).rowCount
    )
      throw new ApiError(
        403,
        "PROTECTED_IDENTITY",
        "An emergency account cannot be replaced.",
      );
    const digest = createHash("sha256")
      .update(
        JSON.stringify([
          actor.holdingId,
          connection.provider,
          connection.config.authority,
          identity.external_id,
        ]),
      )
      .digest("hex");
    const userId = `dir_${digest.slice(0, 40)}`,
      email = `${digest}@identity.erpx.invalid`,
      now = Date.now();
    // The control-plane identity may survive a retried holding transaction; it has no
    // grants until this transaction commits, and its deterministic key is resumable.
    await env.CONTROL_DB.prepare(
      'INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?) ON CONFLICT(id) DO NOTHING',
    )
      .bind(userId, identity.name, email, now, now)
      .run();
    await env.CONTROL_DB.prepare(
      "INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?) ON CONFLICT(id) DO NOTHING",
    )
      .bind(`principal-${userId}`, userId, identity.name, now)
      .run();
    if (connection.config.mode === "oidc")
      await env.CONTROL_DB.prepare(
        'INSERT INTO account(id,"accountId","providerId","userId","createdAt","updatedAt",issuer) VALUES(?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING',
      )
        .bind(
          `directory-${connection.id}-${userId}`,
          identity.external_id,
          `directory-${connection.id}`,
          userId,
          now,
          now,
          connection.config.issuer,
        )
        .run();
    await c.query(
      "INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy) VALUES($1,$2,$3,$4,'superseded','local') ON CONFLICT(company_id,user_id) DO UPDATE SET status='superseded',person_id=excluded.person_id,version=iam_members.version+1",
      [actor.companyId, u.localUserId, `local:${u.localUserId}`, u.personId],
    );
    await c.query(
      "INSERT INTO iam_members(company_id,user_id,principal_id,person_id,login_policy,connection_id,identity_id) VALUES($1,$2,$3,$4,'directory',$5,$6) ON CONFLICT(company_id,user_id) DO UPDATE SET person_id=excluded.person_id,connection_id=excluded.connection_id,identity_id=excluded.identity_id,login_policy='directory',status='active',version=iam_members.version+1",
      [
        actor.companyId,
        userId,
        `directory:${digest}`,
        u.personId,
        connection.id,
        identity.id,
      ],
    );
    await c.query(
      "UPDATE iam_identities SET user_id=$1,person_id=$2 WHERE id=$3",
      [userId, u.personId, identity.id],
    );
    await c.query(
      "UPDATE hr_people SET user_id=$1,version=version+1 WHERE id=$2 AND company_id=$3",
      [userId, u.personId, actor.companyId],
    );
    await c.query(
      "UPDATE employees SET user_id=$1 WHERE user_id=$2 AND company_id=$3",
      [userId, u.localUserId, actor.companyId],
    );
    await c.query(
      "UPDATE assignments SET revoked_at=$1,version=version+1 WHERE user_id=$2 AND company_id=$3 AND revoked_at IS NULL",
      [now, u.localUserId, actor.companyId],
    );
    await c.query(
      "UPDATE active_work_contexts SET assignment_id=NULL,assignment_version=NULL,revision=revision+1 WHERE company_id=$1 AND user_id=$2",
      [actor.companyId, u.localUserId],
    );
    for (const grant of u.grants)
      await validateIamGrant(
        c,
        actor,
        grantInput.parse({ ...grant, userId }),
        String(p.parentId),
      );
    for (const grant of u.grants)
      await insertIamGrant(
        c,
        actor,
        grantInput.parse({ ...grant, userId }),
        String(p.parentId),
      );
  }
  await c.query(
    "UPDATE iam_migrations SET status='complete',version=version+1 WHERE id=$1",
    [batch.id],
  );
}
async function localRecoveryReady(userId: string) {
  const db = (await runtimeEnv()).CONTROL_DB;
  return !!(await db
    .prepare(
      'SELECT 1 FROM iam_mfa m JOIN account a ON a."userId"=m.user_id WHERE m.user_id=? AND m.enabled AND a."providerId"=\'credential\' AND a.password IS NOT NULL',
    )
    .bind(userId)
    .first());
}
async function validateRecovery(
  c: PoolClient,
  actor: IamActor,
  userId: string,
) {
  const table = actor.platform ? "platform_assignments" : "assignments",
    owner = actor.platform ? "platform_admin" : "holding_owner";
  const target = (
    await c.query(
      `SELECT id FROM ${table} WHERE user_id=$1 AND role=$2 AND iam_assignment_active(id,$3) ${actor.platform ? "" : "AND company_id=$4"} FOR SHARE`,
      actor.platform
        ? [userId, owner, Date.now()]
        : [userId, owner, Date.now(), actor.companyId],
    )
  ).rowCount;
  if (!target)
    throw new ApiError(
      409,
      "RECOVERY_ROLE_REQUIRED",
      "Choose an active owner account.",
    );
  const existing = (
    await c.query(
      "SELECT * FROM iam_members WHERE company_id=$1 AND user_id=$2 FOR SHARE",
      [actor.companyId, userId],
    )
  ).rows[0];
  if (
    existing &&
    (existing.login_policy === "directory" || existing.status !== "active")
  )
    throw new ApiError(
      409,
      "LOCAL_ACCOUNT_REQUIRED",
      "Recovery requires an active local identity.",
    );
  if (!(await localRecoveryReady(userId)))
    throw new ApiError(
      409,
      "RECOVERY_NOT_READY",
      "The local administrator must have a password and an enrolled authenticator.",
    );
}
export async function proposeEmergency(
  db: PostgresDatabase,
  actor: IamActor,
  userId: string,
  key: string,
) {
  db = iamDb(db);
  assertCompanyScope(actor);
  await requireFreshMfa(actor);
  return iamCommand(
    db,
    actor,
    key,
    "emergency.propose",
    { userId },
    "iam.recovery",
    async (c) => {
      await validateRecovery(c, actor, userId);
      return proposeIamReview(
        c,
        actor,
        "emergency.designate",
        { userId },
        "Designate protected local emergency administrator.",
      );
    },
  );
}
export async function applyEmergency(
  c: PoolClient,
  actor: IamActor,
  p: Record<string, unknown>,
) {
  const userId = z.string().parse(p.userId);
  if (
    (
      await c.query("SELECT iam_same_person($1,$2,$3) same", [
        userId,
        actor.userId,
        actor.companyId,
      ])
    ).rows[0].same
  )
    throw new ApiError(
      403,
      "INDEPENDENT_REVIEW_REQUIRED",
      "Another person must approve your emergency designation.",
    );
  await validateRecovery(c, actor, userId);
  await c.query(
    "INSERT INTO iam_members(company_id,user_id,principal_id,login_policy) VALUES($1,$2,$3,'emergency') ON CONFLICT(company_id,user_id) DO UPDATE SET login_policy='emergency',version=iam_members.version+1",
    [actor.companyId, userId, `local:${userId}`],
  );
}
