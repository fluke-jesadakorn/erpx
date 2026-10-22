import "server-only";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { transactionDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { ApiError } from "@erpx/server-runtime/public/http";
import { iamRole, loadIamAuthority } from "@erpx/server-identity_access/adapters/iam-store";
import { sessionLifetime } from "@erpx/server-identity_access/domain/product-sso";

export type PlatformCommercialActor = { userId: string; sessionId: string; assignmentId: string; role: string; roleRevision: number };
export type PlatformCommercialLease = { actor: PlatformCommercialActor; canManage: boolean; operation: "preview" | "apply"; assertCurrent(): Promise<{ userId: string }> };

/** Holds current Control authority until the Holding command commits. Control has
 * no writes and is deliberately never retried around a cross-database side effect.
 * These rare provider commands take short SHARE locks, including absent-member
 * and tombstone predicates; revocation/role publication cannot race the commit. */
export async function withPlatformCommercialAuthority<T>(db: PostgresDatabase, actor: PlatformCommercialActor,
  permission: "platform.commercial.read" | "platform.commercial.manage", freshMfa: boolean,
  run: (lease: PlatformCommercialLease) => Promise<T>, now = Date.now): Promise<T> {
  const client = await db.pool.connect();
  try {
    await client.query("BEGIN ISOLATION LEVEL READ COMMITTED");
    await client.query("SET LOCAL lock_timeout='5s'");
    // Match own-context selection and normal family deletion before either can
    // acquire a session row/cascading context lock. LOCK takes no data snapshot.
    await client.query('LOCK TABLE session IN EXCLUSIVE MODE');
    let providerContextAvailable = true;
    await client.query('SAVEPOINT provider_context_schema');
    try {
      await client.query('LOCK TABLE admin_provider_contexts IN SHARE MODE');
      await client.query('RELEASE SAVEPOINT provider_context_schema');
    } catch (error) {
      await client.query('ROLLBACK TO SAVEPOINT provider_context_schema');
      await client.query('RELEASE SAVEPOINT provider_context_schema');
      if ((error as {code?:string}).code !== '42P01') throw error;
      providerContextAvailable = false;
    }
    await client.query(`LOCK TABLE platform_assignments,iam_roles,iam_role_revisions,iam_members,iam_identities,iam_connections,
      iam_revoked_sessions,iam_emergency_sessions,iam_mfa,iam_mfa_sessions,sso_session_bindings IN SHARE MODE`);
    const binding = (await client.query(`SELECT b.parent_session_id FROM sso_session_bindings b WHERE b.session_id=$1
      AND b.audience='admin' AND b.environment='development'`, [actor.sessionId])).rows[0];
    if (!binding?.parent_session_id) throw new ApiError(401, "PRODUCT_SESSION_REQUIRED", "Sign in to Administration through ERPX Account.");
    // Ordered row locks also serialize family/all-device deletion and expiry edits.
    await client.query('SELECT id FROM session WHERE id=ANY($1::text[]) ORDER BY id FOR SHARE', [[actor.sessionId, binding.parent_session_id]]);
    const database = transactionDatabase(client);
    let active = true;
    let canManage = false;
    async function assertCurrent() {
      if (!active) throw new Error("PLATFORM_AUTHORITY_LEASE_CLOSED");
      const instant = now();
      const row = (await client.query(`SELECT a.role,a.version,
        iam_assignment_active(a.id,$4) assignment_active,
        s."userId" AS user_id,p."userId" AS parent_user_id,s."expiresAt" AS expires_at,p."expiresAt" AS parent_expires_at,
        b.authenticated_at,b.absolute_expires_at,pb.authenticated_at AS parent_authenticated_at,pb.absolute_expires_at AS parent_absolute_expires_at,
        EXISTS(SELECT 1 FROM iam_revoked_sessions r WHERE r.company_id='platform' AND r.user_id=$2 AND r.session_id IN(s.id,p.id)) revoked,
        EXISTS(SELECT 1 FROM iam_mfa_sessions f JOIN iam_mfa m ON m.user_id=f.user_id AND m.enabled
          WHERE f.user_id=$2 AND f.session_id=s.id AND f.verified_at>$4-300000 AND f.verified_at<=$4) fresh_mfa
        FROM platform_assignments a JOIN session s ON s.id=$1 JOIN sso_session_bindings b ON b.session_id=s.id
        JOIN session p ON p.id=b.parent_session_id JOIN sso_session_bindings pb ON pb.session_id=p.id
        WHERE a.id=$3 AND a.user_id=$2 AND b.audience='admin' AND b.environment='development'
          AND pb.audience='account' AND pb.environment='development' AND pb.parent_session_id IS NULL`,
        [actor.sessionId, actor.userId, actor.assignmentId, instant])).rows[0];
      if (!row || row.user_id !== actor.userId || row.parent_user_id !== actor.userId || row.revoked ||
        Date.parse(row.expires_at) <= instant || Date.parse(row.parent_expires_at) <= instant ||
        Number(row.absolute_expires_at) <= instant || Number(row.parent_absolute_expires_at) < Number(row.absolute_expires_at) ||
        Number(row.authenticated_at) > instant || Number(row.parent_absolute_expires_at) > Number(row.authenticated_at) + sessionLifetime ||
        Number(row.authenticated_at) !== Number(row.parent_authenticated_at))
        throw new ApiError(401, "SESSION_REVOKED", "Sign in again to continue.");
      if (!row.assignment_active || row.role !== actor.role) throw new ApiError(403, "PLATFORM_AUTHORITY_REQUIRED", "Your selected Platform assignment is unavailable.");
      if (providerContextAvailable) {
        const selected = (await client.query('SELECT user_id,assignment_id FROM admin_provider_contexts WHERE session_id=$1', [actor.sessionId])).rows[0];
        if (selected && (selected.user_id !== actor.userId || selected.assignment_id !== actor.assignmentId))
          throw new ApiError(409, 'PROVIDER_CONTEXT_CHANGED', 'Your selected Platform context changed. Reload and review before continuing.');
      }
      const role = await iamRole(database, "platform", actor.role);
      if (!role?.scopes.includes("platform")) throw new ApiError(403, "PLATFORM_AUTHORITY_REQUIRED", "Your selected role is not published for Platform scope.");
      const iam = await loadIamAuthority(database, "platform", actor.userId, actor.role, actor.sessionId);
      if (iam.roleRevision !== actor.roleRevision) throw new ApiError(409, "POLICY_CHANGED", "Your Platform policy changed. Refresh and review again.");
      if (!iam.permissions.includes(permission)) throw new ApiError(403, "PLATFORM_COMMERCIAL_REQUIRED", "Your Platform role does not grant this Commercial operation.");
      canManage = iam.permissions.includes("platform.commercial.manage");
      if (freshMfa && !row.fresh_mfa) throw new ApiError(403, "FRESH_MFA_REQUIRED", "Verify an authenticator code before applying a package migration.");
      return { userId: actor.userId };
    }
    try {
      await assertCurrent();
      const result = await run({ actor, canManage, operation: permission === "platform.commercial.manage" && freshMfa ? "apply" : "preview", assertCurrent });
      // All mutation guards run inside the Holding transaction. Do not introduce
      // a fallible authorization check after its commit.
      await client.query("COMMIT");
      return result;
    } finally { active = false; }
  } catch (error) {
    await client.query("ROLLBACK").catch(() => {});
    if (["55P03", "40P01"].includes((error as { code?: string }).code ?? "")) throw new ApiError(409, "AUTHORITY_BUSY", "Access is changing. Refresh and review again.");
    throw error;
  } finally { client.release(); }
}
