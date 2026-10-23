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
            return (await c.query("SELECT iam_same_person($1,$2,$3) same", [
                user.localUserId,
                actor.userId,
                actor.companyId,
            ])).rows[0] ?? null;
        },
        protectedMigrationIdentity: async (facts) => {
            const { user } = facts;
            return (await c.query("SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 AND (login_policy IN ('emergency','directory') OR status!='active')", [actor.companyId, user.localUserId])).rowCount;
        },
        insertMigrationBatch: async (facts) => {
            const { id, input, observedNow } = facts;
            return void await c.query("INSERT INTO iam_migrations(id,company_id,connection_id,users_json,status,created_by,created_at) VALUES($1,$2,$3,$4,'awaiting_review',$5,$6)", [
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
            return (await c.query("SELECT iam_same_person($1,$2,$3) same", [
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
            return (await c.query("SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 AND (login_policy IN ('emergency','directory') OR status!='active')", [actor.companyId, u.localUserId])).rowCount;
        },
        supersedeLocalIdentity: async (facts) => {
            const { u } = facts;
            return void await c.query("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy) VALUES($1,$2,$3,$4,'superseded','local') ON CONFLICT(company_id,user_id) DO UPDATE SET status='superseded',person_id=excluded.person_id,version=iam_members.version+1", [actor.companyId, u.localUserId, `local:${u.localUserId}`, u.personId]);
        },
        activateDirectoryIdentity: async (facts) => {
            const { userId, digest, u, connection, identity } = facts;
            return void await c.query("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,login_policy,connection_id,identity_id) VALUES($1,$2,$3,$4,'directory',$5,$6) ON CONFLICT(company_id,user_id) DO UPDATE SET person_id=excluded.person_id,connection_id=excluded.connection_id,identity_id=excluded.identity_id,login_policy='directory',status='active',version=iam_members.version+1", [
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
            return void await c.query("UPDATE iam_identities SET user_id=$1,person_id=$2 WHERE id=$3", [userId, u.personId, identity.id]);
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
            return void await c.query("UPDATE assignments SET revoked_at=$1,version=version+1 WHERE user_id=$2 AND company_id=$3 AND revoked_at IS NULL", [now, u.localUserId, actor.companyId]);
        },
        clearMigratedWorkSelection: async (facts) => {
            const { u } = facts;
            return void await c.query("UPDATE active_work_contexts SET assignment_id=NULL,assignment_version=NULL,revision=revision+1 WHERE company_id=$1 AND user_id=$2", [actor.companyId, u.localUserId]);
        },
        completeMigrationBatch: async (facts) => {
            const { batch } = facts;
            return void await c.query("UPDATE iam_migrations SET status='complete',version=version+1 WHERE id=$1", [batch.id]);
        },
        currentRecoveryOwner: async (facts) => {
            const { userId, owner, observedNow } = facts;
            return (await c.query(`SELECT id FROM ${actor.platform ? "platform_assignments" : "assignments"} WHERE user_id=$1 AND role=$2 AND iam_assignment_active(id,$3) ${actor.platform ? "" : "AND company_id=$4"} FOR SHARE`, actor.platform
                ? [userId, owner, observedNow]
                : [userId, owner, observedNow, actor.companyId])).rowCount;
        },
        currentRecoveryIdentity: async (facts) => {
            const { userId } = facts;
            return (await c.query("SELECT * FROM iam_members WHERE company_id=$1 AND user_id=$2 FOR SHARE", [actor.companyId, userId])).rows[0] ?? null;
        },
        emergencySamePerson: async (facts) => {
            const { userId } = facts;
            return (await c.query("SELECT iam_same_person($1,$2,$3) same", [
                userId,
                actor.userId,
                actor.companyId,
            ])).rows[0] ?? null;
        },
        designateRecoveryIdentity: async (facts) => {
            const { userId } = facts;
            return void await c.query("INSERT INTO iam_members(company_id,user_id,principal_id,login_policy) VALUES($1,$2,$3,'emergency') ON CONFLICT(company_id,user_id) DO UPDATE SET login_policy='emergency',version=iam_members.version+1", [actor.companyId, userId, `local:${userId}`]);
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
            await env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?) ON CONFLICT(id) DO NOTHING')
                .bind(userId, identity.name, email, now, now)
                .run();
            await env.CONTROL_DB.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?) ON CONFLICT(id) DO NOTHING")
                .bind(`principal-${userId}`, userId, identity.name, now)
                .run();
            if (connection.config.mode === 'oidc')
                await env.CONTROL_DB.prepare('INSERT INTO account(id,"accountId","providerId","userId","createdAt","updatedAt",issuer) VALUES(?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING')
                    .bind(`directory-${connection.id}-${userId}`, identity.external_id, `directory-${connection.id}`, userId, now, now, connection.config.issuer)
                    .run();
        }, command: async (actor, key, operation, payload, permission, run) => iamCommand(db!, actor, key, operation, payload, permission, async (c) => run(migrationTx(c, actor))), validateGrant: (tx, actor, input, parentId) => validateIamGrant(clients.get(tx)!, actor, input, parentId), insertGrant: (tx, actor, input, parentId) => insertIamGrant(clients.get(tx)!, actor, input, parentId), review: (tx, actor, kind, payload, reason) => proposeIamReview(clients.get(tx)!, actor, kind, payload, reason) });
}
