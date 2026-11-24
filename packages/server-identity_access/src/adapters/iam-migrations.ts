import { nativeIdentityWrite } from '@erpx/server-identity_access/public/native-identity-write';
import { samePersonQuerySql, memberForShareSql, protectedMigrationIdentitySql } from "./iam-probe-sql";
import { nativePrepare } from '@erpx/server-identity_access/public/native-identity-write';
import {linkMigratedPerson,linkMigratedEmployee} from "@erpx/server-people_payroll/public/identity-person-link-effects";
import {identitySnapshotSources as peoplepayrollSnapshotSources} from '@erpx/server-people_payroll/public/identity-snapshot-sources';
const peoplepayrollSources=peoplepayrollSnapshotSources();
import { createMigrationService, type MigrationTransaction } from '@erpx/server-identity_access/application/migrations';
const clients = new WeakMap<MigrationTransaction, PoolClient>();
import { iamDb } from "@erpx/server-identity_access/public/iam-db";
import "server-only";
import { createHash } from "node:crypto";
import { z } from "zod";
import type { PoolClient } from "pg";
import { ApiError } from "@erpx/server-runtime/public/http";
import { runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { assertCompanyScope, type IamActor } from "@erpx/server-identity_access/public/iam-context";
import { requireFreshMfa } from "@erpx/server-identity_access/public/iam-mfa";
import { iamCommand } from "@erpx/server-identity_access/public/iam-transactions";
import { proposeIamReview } from "@erpx/server-identity_access/public/iam-service";
import {insertIamGrant,validateIamGrant} from '@erpx/server-identity_access/public/iam-grants';
export { migrationInput } from '@erpx/server-identity_access/domain/migrations';
import { migrationInput } from '@erpx/server-identity_access/domain/migrations';
export async function prepareMigration(db: PostgresDatabase, actor: IamActor, input: z.infer<typeof migrationInput>, key: string) { return migrationService(db).prepareMigration(actor, input, key); }
export async function applyMigration(c: PoolClient, actor: IamActor, p: Record<string, unknown>) { return migrationService().applyMigration(migrationTx(c, actor), actor, p); }
async function localRecoveryReady(userId: string) {
    const db = (await runtimeEnv()).CONTROL_DB;
    return !!(await db
        .prepare('SELECT 1 FROM iam_mfa m JOIN account a ON a."userId"=m.user_id WHERE m.user_id=? AND m.enabled AND a."providerId"=\'credential\' AND a.password IS NOT NULL')
        .bind(userId)
        .first());
}
export async function proposeEmergency(db: PostgresDatabase, actor: IamActor, userId: string, key: string) { return migrationService(db).proposeEmergency(actor, userId, key); }
export async function applyEmergency(c: PoolClient, actor: IamActor, p: Record<string, unknown>) { return migrationService().applyEmergency(migrationTx(c, actor), actor, p); }
function migrationTx(c: PoolClient, actor: IamActor): MigrationTransaction {
    const tx: MigrationTransaction = { freshMigrationConnection: async (facts) => {
            const { input, observedNow } = facts;
            return (await c.query("SELECT * FROM iam_connections WHERE id=$1 AND company_id=$2 AND status='active' AND last_sync_at>$3", [input.connectionId, actor.companyId, observedNow - 300000])).rows[0] ?? null;
        },
        linkedMigrationPerson: async (facts) => {
            const { user } = facts;
            return (await c.query(`SELECT id ${peoplepayrollSources.iam_migrations_hr_people_from} WHERE id=$1 AND company_id=$2 AND user_id=$3 AND status='active'`, [user.personId, actor.companyId, user.localUserId])).rows[0] ?? null;
        },
        unlinkedMigrationIdentity: async (facts) => {
            const { user, input } = facts;
            return (await c.query("SELECT id FROM iam_identities WHERE id=$1 AND company_id=$2 AND connection_id=$3 AND enabled AND user_id IS NULL", [user.identityId, actor.companyId, input.connectionId])).rows[0] ?? null;
        },
        migrationSamePerson: async (facts) => {
            const { user } = facts;
            return (await c.query(samePersonQuerySql, [
                user.localUserId,
                actor.userId,
                actor.companyId,
            ])).rows[0] ?? null;
        },
        protectedMigrationIdentity: async (facts) => {
            const { user } = facts;
            return (await c.query(protectedMigrationIdentitySql, [actor.companyId, user.localUserId])).rowCount;
        },
        insertMigrationBatch: async (facts) => {
            const { id, input, observedNow } = facts;
            return void await nativeIdentityWrite("w_ec0f720aa686", [
                id,
                actor.companyId,
                input.connectionId,
                JSON.stringify(input.users),
                actor.userId,
                observedNow,
            ]);
        },
        lockMigrationBatch: async (facts) => {
            const { p } = facts;
            return (await c.query("SELECT * FROM iam_migrations WHERE id=$1 AND company_id=$2 FOR UPDATE", [p.id, actor.companyId])).rows[0] ?? null;
        },
        freshReviewedConnection: async (facts) => {
            const { batch, observedNow } = facts;
            return (await c.query("SELECT * FROM iam_connections WHERE id=$1 AND company_id=$2 AND status='active' AND last_sync_at>$3 FOR SHARE", [batch.connection_id, actor.companyId, observedNow - 300000])).rows[0] ?? null;
        },
        recoveryAdministrators: async (facts) => {
            const { observedNow } = facts;
            return (await c.query("SELECT m.user_id FROM iam_members m JOIN assignments a ON a.user_id=m.user_id AND a.company_id=m.company_id WHERE m.company_id=$1 AND m.login_policy='emergency' AND m.status='active' AND a.role='holding_owner' AND iam_assignment_active(a.id,$2)", [actor.companyId, observedNow])).rows;
        },
        migrationReviewerSamePerson: async (facts) => {
            const { u } = facts;
            return (await c.query(samePersonQuerySql, [
                u.localUserId,
                actor.userId,
                actor.companyId,
            ])).rows[0] ?? null;
        },
        lockMigrationPerson: async (facts) => {
            const { u } = facts;
            return (await c.query(`SELECT * ${peoplepayrollSources.iam_migrations_hr_people_from} WHERE id=$1 AND company_id=$2 AND user_id=$3 FOR UPDATE`, [u.personId, actor.companyId, u.localUserId])).rows[0] ?? null;
        },
        lockMigrationIdentity: async (facts) => {
            const { u, connection } = facts;
            return (await c.query("SELECT * FROM iam_identities WHERE id=$1 AND company_id=$2 AND connection_id=$3 AND enabled AND user_id IS NULL FOR UPDATE", [u.identityId, actor.companyId, connection.id])).rows[0] ?? null;
        },
        protectReviewedMigrationIdentity: async (facts) => {
            const { u } = facts;
            return (await c.query(protectedMigrationIdentitySql, [actor.companyId, u.localUserId])).rowCount;
        },
        supersedeLocalIdentity: async (facts) => {
            const { u } = facts;
            return void await nativeIdentityWrite("w_ea3bd27b980b", [actor.companyId, u.localUserId, `local:${u.localUserId}`, u.personId]);
        },
        activateDirectoryIdentity: async (facts) => {
            const { userId, digest, u, connection, identity } = facts;
            return void await nativeIdentityWrite("w_b2fa6734a05b", [
                actor.companyId,
                userId,
                `directory:${digest}`,
                u.personId,
                connection.id,
                identity.id,
            ]);
        },
        linkMigratedIdentity: async (facts) => {
            const { userId, u, identity } = facts;
            return void await nativeIdentityWrite("w_5b2a2aefb278", [userId, u.personId, identity.id]);
        },
        linkMigratedPerson: async (facts) => {
            const { userId, u } = facts;
            return linkMigratedPerson(c,userId,u.personId,actor.companyId);
        },
        linkMigratedEmployee: async (facts) => {
            const { userId, u } = facts;
            return linkMigratedEmployee(c,userId,u.localUserId,actor.companyId);
        },
        revokeMigratedLocalGrants: async (facts) => {
            const { now, u } = facts;
            return void await nativeIdentityWrite("w_e64cd0fcfd71", [now, u.localUserId, actor.companyId]);
        },
        clearMigratedWorkSelection: async (facts) => {
            const { u } = facts;
            return void await nativeIdentityWrite("w_8774a0e28e6e", [actor.companyId, u.localUserId]);
        },
        completeMigrationBatch: async (facts) => {
            const { batch } = facts;
            return void await nativeIdentityWrite("w_35a97c23eda2", [batch.id]);
        },
        currentRecoveryOwner: async (facts) => {
            const { userId, owner, observedNow } = facts;
            return (await c.query(`SELECT id FROM ${actor.platform ? "platform_assignments" : "assignments"} WHERE user_id=$1 AND role=$2 AND iam_assignment_active(id,$3) ${actor.platform ? "" : "AND company_id=$4"} FOR SHARE`, actor.platform
                ? [userId, owner, observedNow]
                : [userId, owner, observedNow, actor.companyId])).rowCount;
        },
        currentRecoveryIdentity: async (facts) => {
            const { userId } = facts;
            return (await c.query(memberForShareSql, [actor.companyId, userId])).rows[0] ?? null;
        },
        emergencySamePerson: async (facts) => {
            const { userId } = facts;
            return (await c.query(samePersonQuerySql, [
                userId,
                actor.userId,
                actor.companyId,
            ])).rows[0] ?? null;
        },
        designateRecoveryIdentity: async (facts) => {
            const { userId } = facts;
            return void await nativeIdentityWrite("w_bb125e66f021", [actor.companyId, userId, `local:${userId}`]);
        } };
    clients.set(tx, c);
    return tx;
}
function migrationService(raw?: PostgresDatabase) {
    const db = raw ? iamDb(raw) : undefined;
    let environment: Awaited<ReturnType<typeof runtimeEnv>>;
    const control = async () => environment ??= await runtimeEnv();
    return createMigrationService({ error: (...args) => new ApiError(...args), now: Date.now, id: () => crypto.randomUUID(), digest: (value) => createHash('sha256').update(value).digest('hex'), companyScope: assertCompanyScope, fresh: requireFreshMfa, platformIdentity: async (userId) => !!await (await control()).CONTROL_DB.prepare('SELECT 1 FROM platform_assignments WHERE user_id=? AND revoked_at IS NULL').bind(userId).first(), readyControl: async () => { await control(); }, localRecoveryReady, controlIdentity: async (connection, identity, userId, email, now) => {
            const env = await control();
            await nativePrepare("w_bccf47b0a2b9")
                .bind(userId, identity.name, email, now, now)
                .run();
            await nativePrepare("w_9e7d979fc417")
                .bind(`principal-${userId}`, userId, identity.name, now)
                .run();
            if (connection.config.mode === 'oidc')
                await nativePrepare("w_3b27f567191b")
                    .bind(`directory-${connection.id}-${userId}`, identity.external_id, `directory-${connection.id}`, userId, now, now, connection.config.issuer)
                    .run();
        }, command: async (actor, key, operation, payload, permission, run) => iamCommand(db!, actor, key, operation, payload, permission, async (c) => run(migrationTx(c, actor))), validateGrant: (tx, actor, input, parentId) => validateIamGrant(clients.get(tx)!, actor, input, parentId), insertGrant: (tx, actor, input, parentId) => insertIamGrant(clients.get(tx)!, actor, input, parentId), review: (tx, actor, kind, payload, reason) => proposeIamReview(clients.get(tx)!, actor, kind, payload, reason) });
}
