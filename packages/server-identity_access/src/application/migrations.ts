import { z } from 'zod';
import { migrationInput, type MigrationInput, type MigrationMember, type MigrationBatch, type MigrationPerson, type MigrationIdentity } from '@erpx/server-identity_access/domain/migrations';
import { grantInput, type GrantInput } from '@erpx/server-identity_access/domain/iam-service';
import type { ConnectionRow } from '@erpx/server-identity_access/domain/directories';
import type { IamActor } from '@erpx/server-identity_access/domain/iam-actor';
import type { IdentityError } from '@erpx/server-identity_access/application/authorization';
export interface MigrationTransaction {
    freshMigrationConnection(facts: {
        input: MigrationInput;
        observedNow: number;
    }): Promise<ConnectionRow | null>;
    linkedMigrationPerson(facts: {
        user: MigrationMember;
    }): Promise<MigrationPerson | null>;
    unlinkedMigrationIdentity(facts: {
        user: MigrationMember;
        input: MigrationInput;
    }): Promise<MigrationIdentity | null>;
    migrationSamePerson(facts: {
        user: MigrationMember;
    }): Promise<{
        same: boolean;
    }>;
    protectedMigrationIdentity(facts: {
        user: MigrationMember;
    }): Promise<number | null>;
    insertMigrationBatch(facts: {
        id: string;
        input: MigrationInput;
        observedNow: number;
    }): Promise<void>;
    lockMigrationBatch(facts: {
        p: Record<string, unknown>;
    }): Promise<MigrationBatch | null>;
    freshReviewedConnection(facts: {
        batch: MigrationBatch;
        observedNow: number;
    }): Promise<ConnectionRow | null>;
    recoveryAdministrators(facts: {
        observedNow: number;
    }): Promise<{
        user_id: string;
    }[]>;
    migrationReviewerSamePerson(facts: {
        u: MigrationMember;
    }): Promise<{
        same: boolean;
    }>;
    lockMigrationPerson(facts: {
        u: MigrationMember;
    }): Promise<MigrationPerson | null>;
    lockMigrationIdentity(facts: {
        u: MigrationMember;
        connection: ConnectionRow;
    }): Promise<MigrationIdentity | null>;
    protectReviewedMigrationIdentity(facts: {
        u: MigrationMember;
    }): Promise<number | null>;
    supersedeLocalIdentity(facts: {
        u: MigrationMember;
    }): Promise<void>;
    activateDirectoryIdentity(facts: {
        userId: string;
        digest: string;
        u: MigrationMember;
        connection: ConnectionRow;
        identity: MigrationIdentity;
    }): Promise<void>;
    linkMigratedIdentity(facts: {
        userId: string;
        u: MigrationMember;
        identity: MigrationIdentity;
    }): Promise<void>;
    linkMigratedPerson(facts: {
        userId: string;
        u: MigrationMember;
    }): Promise<void>;
    linkMigratedEmployee(facts: {
        userId: string;
        u: MigrationMember;
    }): Promise<void>;
    revokeMigratedLocalGrants(facts: {
        now: number;
        u: MigrationMember;
    }): Promise<void>;
    clearMigratedWorkSelection(facts: {
        u: MigrationMember;
    }): Promise<void>;
    completeMigrationBatch(facts: {
        batch: MigrationBatch;
    }): Promise<void>;
    currentRecoveryOwner(facts: {
        userId: string;
        owner: string;
        observedNow: number;
    }): Promise<number | null>;
    currentRecoveryIdentity(facts: {
        userId: string;
    }): Promise<{
        login_policy: string;
        status: string;
    } | null>;
    emergencySamePerson(facts: {
        userId: string;
    }): Promise<{
        same: boolean;
    }>;
    designateRecoveryIdentity(facts: {
        userId: string;
    }): Promise<void>;
}
export interface MigrationPorts {
    error: IdentityError;
    now(): number;
    id(): string;
    digest(value: string): string;
    companyScope(actor: IamActor): void;
    fresh(actor: IamActor): Promise<void>;
    platformIdentity(userId: string): Promise<boolean>;
    readyControl(): Promise<void>;
    localRecoveryReady(userId: string): Promise<boolean>;
    controlIdentity(connection: ConnectionRow, identity: MigrationIdentity, userId: string, email: string, now: number): Promise<void>;
    command<T>(actor: IamActor, key: string, operation: string, payload: unknown, permission: string, run: (tx: MigrationTransaction) => Promise<T>): Promise<T>;
    validateGrant(tx: MigrationTransaction, actor: IamActor, input: GrantInput, parentId?: string): Promise<unknown>;
    insertGrant(tx: MigrationTransaction, actor: IamActor, input: GrantInput, parentId: string): Promise<unknown>;
    review(tx: MigrationTransaction, actor: IamActor, kind: string, payload: unknown, reason: string): Promise<{
        id: string;
        status: string;
        message: string;
    }>;
}
export function createMigrationService(ports: MigrationPorts) {
    async function prepareMigration(actor: IamActor, input: z.infer<typeof migrationInput>, key: string) {
        ports.companyScope(actor);
        await ports.fresh(actor);
        if (actor.platform)
            throw ports.error(422, "PLATFORM_MIGRATION_UNAVAILABLE", "Customer employee migrations use a company workspace.");
        if (new Set(input.users.map((u) => u.localUserId)).size !==
            input.users.length ||
            new Set(input.users.map((u) => u.identityId)).size !== input.users.length)
            throw ports.error(422, "DUPLICATE_MIGRATION_MEMBER", "Each account and directory identity can occur once per batch.");
        return ports.command(actor, key, "migration.prepare", input, "iam.manage", async (tx) => {
            const connection = (await tx.freshMigrationConnection({ input, observedNow: ports.now() }));
            if (!connection)
                throw ports.error(409, "DIRECTORY_NOT_READY", "Activate and synchronize the directory first.");
            for (const user of input.users) {
                const person = (await tx.linkedMigrationPerson({ user }));
                const identity = (await tx.unlinkedMigrationIdentity({ user, input }));
                if (!person || !identity)
                    throw ports.error(409, "IDENTITY_LINK_INVALID", "Choose an active employee and an unlinked directory identity.");
                if (user.localUserId === actor.userId ||
                    (await tx.migrationSamePerson({ user })).same)
                    throw ports.error(403, "SELF_ACCESS_CHANGE", "Another administrator must migrate your identity.");
                const protectedAccount = (await tx.protectedMigrationIdentity({ user }));
                const platform = await ports.platformIdentity(user.localUserId);
                if (protectedAccount || platform)
                    throw ports.error(403, "PROTECTED_IDENTITY", "Emergency and platform identities cannot be replaced.");
                for (const grant of user.grants) {
                    const normalized = grantInput.parse({
                        ...grant,
                        userId: user.localUserId,
                    });
                    await ports.validateGrant(tx, actor, normalized);
                    grant.validUntil = normalized.validUntil;
                    grant.expectedRoleRevision = normalized.expectedRoleRevision;
                }
            }
            const id = ports.id();
            (await tx.insertMigrationBatch({ id, input, observedNow: ports.now() }));
            return {
                ...(await ports.review(tx, actor, "migration.activate", { id, version: 1, parentId: actor.assignmentId }, input.reason)),
                batchId: id,
            };
        });
    }
    async function applyMigration(tx: MigrationTransaction, actor: IamActor, p: Record<string, unknown>) {
        const batch = (await tx.lockMigrationBatch({ p }));
        if (!batch ||
            batch.status !== "awaiting_review" ||
            Number(batch.version) !== p.version)
            throw ports.error(409, "MIGRATION_CHANGED", "The migration batch changed.");
        const connection = (await tx.freshReviewedConnection({ batch, observedNow: ports.now() }));
        if (!connection)
            throw ports.error(409, "DIRECTORY_NOT_READY", "Refresh the active directory before migrating.");
        const emergency = (await tx.recoveryAdministrators({ observedNow: ports.now() }));
        await ports.readyControl();
        let ready = false;
        for (const row of emergency) {
            if (await ports.localRecoveryReady(row.user_id))
                ready = true;
        }
        if (!ready)
            throw ports.error(409, "RECOVERY_NOT_READY", "An active local recovery administrator with an enrolled authenticator is required.");
        const users = migrationInput.shape.users.parse(batch.users_json);
        for (const u of users) {
            if (u.localUserId === actor.userId ||
                (await tx.migrationReviewerSamePerson({ u })).same)
                throw ports.error(403, "SELF_REVIEW_FORBIDDEN", "The target employee cannot approve this migration.");
            const person = (await tx.lockMigrationPerson({ u }));
            const identity = (await tx.lockMigrationIdentity({ u, connection }));
            if (!person || person.status !== "active" || !identity)
                throw ports.error(409, "IDENTITY_LINK_CHANGED", "The employee or directory identity changed.");
            if ((await tx.protectReviewedMigrationIdentity({ u })))
                throw ports.error(403, "PROTECTED_IDENTITY", "An emergency account cannot be replaced.");
            const digest = ports.digest(JSON.stringify([actor.holdingId, connection.provider, connection.config.authority, identity.external_id]));
            const userId = `dir_${digest.slice(0, 40)}`, email = `${digest}@identity.erpx.invalid`, now = ports.now();
            // The control-plane identity may survive a retried holding transaction; it has no
            // grants until this transaction commits, and its deterministic key is resumable.
            await ports.controlIdentity(connection, identity, userId, email, now);
            (await tx.supersedeLocalIdentity({ u }));
            (await tx.activateDirectoryIdentity({ userId, digest, u, connection, identity }));
            (await tx.linkMigratedIdentity({ userId, u, identity }));
            (await tx.linkMigratedPerson({ userId, u }));
            (await tx.linkMigratedEmployee({ userId, u }));
            (await tx.revokeMigratedLocalGrants({ now, u }));
            (await tx.clearMigratedWorkSelection({ u }));
            for (const grant of u.grants)
                await ports.validateGrant(tx, actor, grantInput.parse({ ...grant, userId }), String(p.parentId));
            for (const grant of u.grants)
                await ports.insertGrant(tx, actor, grantInput.parse({ ...grant, userId }), String(p.parentId));
        }
        (await tx.completeMigrationBatch({ batch }));
    }
    async function validateRecovery(tx: MigrationTransaction, actor: IamActor, userId: string) {
        const owner = actor.platform ? "platform_admin" : "holding_owner";
        const target = (await tx.currentRecoveryOwner({ userId, owner, observedNow: ports.now() }));
        if (!target)
            throw ports.error(409, "RECOVERY_ROLE_REQUIRED", "Choose an active owner account.");
        const existing = (await tx.currentRecoveryIdentity({ userId }));
        if (existing &&
            (existing.login_policy === "directory" || existing.status !== "active"))
            throw ports.error(409, "LOCAL_ACCOUNT_REQUIRED", "Recovery requires an active local identity.");
        if (!(await ports.localRecoveryReady(userId)))
            throw ports.error(409, "RECOVERY_NOT_READY", "The local administrator must have a password and an enrolled authenticator.");
    }
    async function proposeEmergency(actor: IamActor, userId: string, key: string) {
        ports.companyScope(actor);
        await ports.fresh(actor);
        return ports.command(actor, key, "emergency.propose", { userId }, "iam.recovery", async (tx) => {
            await validateRecovery(tx, actor, userId);
            return ports.review(tx, actor, "emergency.designate", { userId }, "Designate protected local emergency administrator.");
        });
    }
    async function applyEmergency(tx: MigrationTransaction, actor: IamActor, p: Record<string, unknown>) {
        const userId = z.string().parse(p.userId);
        if ((await tx.emergencySamePerson({ userId })).same)
            throw ports.error(403, "INDEPENDENT_REVIEW_REQUIRED", "Another person must approve your emergency designation.");
        await validateRecovery(tx, actor, userId);
        (await tx.designateRecoveryIdentity({ userId }));
    }
    return { prepareMigration, applyMigration, proposeEmergency, applyEmergency };
}
