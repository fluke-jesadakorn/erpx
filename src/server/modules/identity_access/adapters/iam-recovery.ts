import "server-only";
import { createHash } from "node:crypto";
import type { PoolClient } from "pg";
import { iamDb } from "@/server/modules/identity_access/public/iam-db";
import { requireFreshMfa } from "@/server/modules/identity_access/public/iam-mfa";
import { runtimeEnv, activeHoldingBindings, holdingResources, holdingBindingBySlug } from "@/server/modules/runtime/public/runtime";
import { ApiError } from "@/server/modules/runtime/public/http";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { createRecoveryCommands, createEmergencyAccessList, recoveryWorkspaceScope } from '../application/recovery';
import type { EmergencyActor, RecoveryScope, RecoveryInput } from '../domain/recovery';
export { recoveryInput, emergencyRevocationInput } from '../domain/recovery';
type NativeRecoveryScope = RecoveryScope & {
    db: PostgresDatabase;
};
async function recoveryScope(input: RecoveryInput): Promise<NativeRecoveryScope> {
    const holding = input.holdingSlug
        ? await holdingBindingBySlug(input.holdingSlug)
        : null;
    const scope = recoveryWorkspaceScope(input, holding, (...args) => new ApiError(...args));
    const env = await runtimeEnv();
    return {
        ...scope,
        db: (holding
            ? (await holdingResources(holding.id)).write
            : env.CONTROL_DB) as PostgresDatabase,
    };
}
function requestHash(actor: EmergencyActor, scope: NativeRecoveryScope, operation: string, input: RecoveryInput) {
    return createHash("sha256")
        .update(JSON.stringify({
        actor: actor.userId,
        session: actor.sessionId,
        company: scope.companyId,
        holding: scope.holdingSlug,
        operation,
        input,
    }))
        .digest("hex");
}
async function releaseIdempotencyLock(client: PoolClient, key: string, actor: EmergencyActor, scope: NativeRecoveryScope) {
    await client
        .query("SELECT pg_advisory_unlock(hashtextextended($1,0))", [
        `${scope.companyId}:${actor.userId}:${key}`,
    ])
        .catch(() => undefined);
}
async function saveReceipt(client: PoolClient, key: string, actor: EmergencyActor, scope: NativeRecoveryScope, hash: string, result: Record<string, unknown>, now: number) {
    await client.query("INSERT INTO iam_receipts VALUES($1,$2,$3,$4,$5,$6)", [
        key,
        scope.companyId,
        actor.userId,
        hash,
        JSON.stringify(result),
        now,
    ]);
}
async function recoveryCommand(operation: 'emergency.activate' | 'emergency.revoke', actor: EmergencyActor, input: RecoveryInput, key: string) {
    let resolved: NativeRecoveryScope;
    return createRecoveryCommands({ error: (...args) => new ApiError(...args), scope: async (input) => (resolved = await recoveryScope(input)), transaction: async (_scope, actor, key, operation, input, run) => {
            const scope = resolved!, db = iamDb(scope.db), client = await db.pool.connect(), now = Date.now(), hash = requestHash(actor, scope, operation, input);
            try {
                await client.query('SELECT pg_advisory_lock(hashtextextended($1,0))', [scope.companyId + ':' + actor.userId + ':' + key]);
                await client.query('BEGIN ISOLATION LEVEL SERIALIZABLE');
                const result = await run({ receipt: async () => (await client.query("SELECT company_id,actor_id,request_hash,result FROM iam_receipts WHERE id=$1 FOR UPDATE", [key])).rows[0] ?? null, freshMfa: () => requireFreshMfa(actor), member: async () => !!(await client.query("SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 AND status='active' AND login_policy='emergency' FOR SHARE", [scope.companyId, actor.userId])).rowCount, grant: async () => (await client.query(`SELECT valid_until FROM ${scope.holdingSlug ? "assignments" : "platform_assignments"} WHERE user_id=$1 AND role=$2 AND iam_assignment_active(id,$3) ${scope.holdingSlug ? "AND company_id=$4" : ""} ORDER BY valid_until DESC LIMIT 1 FOR SHARE`, scope.holdingSlug
                        ? [actor.userId, "holding_owner", now, scope.companyId]
                        : [actor.userId, "platform_admin", now])).rows[0], activate: async (expiresAt) => {
                        await client.query("INSERT INTO iam_emergency_sessions VALUES($1,$2,$3,$4,$5) ON CONFLICT(session_id,company_id) DO UPDATE SET expires_at=excluded.expires_at,reason=excluded.reason", [actor.sessionId, scope.companyId, actor.userId, expiresAt, input.reason]);
                    }, revoke: async () => {
                        const deleted = await client.query("DELETE FROM iam_emergency_sessions WHERE session_id=$1 AND user_id=$2 AND company_id=$3 AND expires_at>$4 RETURNING session_id", [actor.sessionId, actor.userId, scope.companyId, now]);
                        return (deleted.rowCount ?? 0) > 0;
                    }, audit: async (operation, detail) => { await client.query('INSERT INTO iam_audit VALUES($1,$2,$3,$4,$5,$6,$7,$8)', [crypto.randomUUID(), scope.companyId, actor.userId, 'recovery', operation, actor.sessionId, JSON.stringify(detail), now]); }, record: async (result) => saveReceipt(client, key, actor, scope, hash, result, now) }, now, hash);
                await client.query('COMMIT');
                return result;
            }
            catch (e) {
                await client.query('ROLLBACK');
                throw e;
            }
            finally {
                await releaseIdempotencyLock(client, key, actor, scope);
                client.release();
            }
        } })(operation, actor, input, key);
}
export const activateEmergency = (actor: EmergencyActor, input: RecoveryInput, key: string) => recoveryCommand('emergency.activate', actor, input, key);
export const revokeEmergency = (actor: EmergencyActor, input: RecoveryInput, key: string) => recoveryCommand('emergency.revoke', actor, input, key);
export async function listEmergencyAccess(actor: EmergencyActor) {
    const env = await runtimeEnv(), scopes = new Map<RecoveryScope, PostgresDatabase>();
    return createEmergencyAccessList({ now: Date.now, error: (...args) => new ApiError(...args), platform: async () => { const scope = { companyId: 'platform', holdingSlug: null, scopeLabel: 'Platform administration' }; scopes.set(scope, env.CONTROL_DB as PostgresDatabase); return scope; }, holdings: activeHoldingBindings, holding: async (holding) => { const scope = { companyId: '*', holdingSlug: holding.slug, scopeLabel: holding.displayName }; scopes.set(scope, (await holdingResources(holding.id as Parameters<typeof holdingResources>[0])).write as PostgresDatabase); return scope; }, sessions: async (scope, actor, now) => {
            const db = scopes.get(scope)!;
            return (await iamDb(db)
                .prepare("SELECT company_id,expires_at,reason FROM iam_emergency_sessions WHERE session_id=? AND user_id=? AND expires_at>? ORDER BY company_id")
                .bind(actor.sessionId, actor.userId, now)
                .all<{
                company_id: string;
                expires_at: number;
                reason: string;
            }>()).results;
        } })(actor);
}
