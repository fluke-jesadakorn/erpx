import {createDirectorySynchronization} from '@erpx/server-identity_access/application/directory-synchronization';
import { z } from 'zod';
import { connectionInput, mappingInput, type ConnectionRow, type ConnectionInput, type MappingInput, type DirectoryMappingFact } from '@erpx/server-identity_access/domain/directories';
import { grantInput, type GrantInput, type IamAssignmentFact } from '@erpx/server-identity_access/domain/iam-service';
import type { IamActor } from '@erpx/server-identity_access/domain/iam-actor';
import type { ProviderSnapshot } from '@erpx/server-identity_access/domain/providers';
import { type DirectoryHierarchy } from "@erpx/feature-organization/lib/hierarchy";
import type { IdentityError } from '@erpx/server-identity_access/application/authorization';
export type DirectoryTestResult = {
    userCount: number;
    groupCount: number;
    snapshot: ProviderSnapshot;
};
export interface DirectoryTransaction {
    lockTrustConnection(facts: {
        id: string;
    }): Promise<ConnectionRow | null>;
    replaceTrustConnection(facts: {
        input: ConnectionInput;
        id: string;
    }): Promise<void>;
    insertTrustConnection(facts: {
        id: string;
        input: ConnectionInput;
        observedNow: number;
    }): Promise<void>;
    recordTestedConnection(facts: {
        id: string;
        version: number;
    }): Promise<{
        changed: boolean;
    }>;
    storeTestPreview(facts: {
        id: string;
        version: number;
        result: DirectoryTestResult;
        observedNow: number;
    }): Promise<void>;
    activationEvidence(facts: {
        id: string;
    }): Promise<ConnectionRow | null>;
    disableTrustConnection(facts: {
        id: string;
        version: number;
    }): Promise<{
        changed: boolean;
    }>;
    invalidateConnectionMembers(facts: {
        id: string;
    }): Promise<void>;
    clearConnectionSelections(facts: {
        id: string;
    }): Promise<void>;
    lockSnapshotConnection(facts: {
        connection: ConnectionRow;
    }): Promise<ConnectionRow | null>;
    storeDirectoryHierarchy(facts: {
        connection: ConnectionRow;
        hierarchy: DirectoryHierarchy;
        now: number;
    }): Promise<void>;
    observeDirectoryGroup(facts: {
        connection: ConnectionRow;
        group: ProviderSnapshot["groups"][number];
        now: number;
    }): Promise<void>;
    observeDirectoryIdentity(facts: {
        identityId: string;
        connection: ConnectionRow;
        user: ProviderSnapshot["users"][number];
        now: number;
    }): Promise<void>;
    disableMissingIdentities(facts: {
        now: number;
        connection: ConnectionRow;
        snapshot: ProviderSnapshot;
    }): Promise<void>;
    removeMissingGroups(facts: {
        connection: ConnectionRow;
        snapshot: ProviderSnapshot;
    }): Promise<void>;
    revokeRemovedMemberships(facts: {
        connection: ConnectionRow;
        now: number;
    }): Promise<void>;
    mappingCandidates(facts: {
        connection: ConnectionRow;
        now: number;
    }): Promise<DirectoryMappingFact[]>;
    existingMappingGrant(facts: {
        connection: ConnectionRow;
        m: DirectoryMappingFact;
    }): Promise<number | null>;
    mappingParent(facts: {
        connection: ConnectionRow;
        m: DirectoryMappingFact;
    }): Promise<IamAssignmentFact>;
    mappingParentPolicy(facts: {
        parent: IamAssignmentFact;
        connection: ConnectionRow;
    }): Promise<{
        grantable_json: string[];
    } | null>;
    recordHealthySync(facts: {
        now: number;
        connection: ConnectionRow;
    }): Promise<void>;
    recordSyncAudit(facts: {
        identityId: string;
        connection: ConnectionRow;
        snapshot: ProviderSnapshot;
        now: number;
    }): Promise<void>;
    knownMappingGroup(facts: {
        input: MappingInput;
    }): Promise<number | null>;
    mappingPreview(facts: {
        input: MappingInput;
    }): Promise<{
        snapshot_json: ProviderSnapshot;
    } | null>;
    activateTestedConnection(facts: {
        p: Record<string, unknown>;
    }): Promise<{
        changed: boolean;
    }>;
    scheduleConnectionSync(facts: {
        p: Record<string, unknown>;
    }): Promise<void>;
    reviewerBenefitsFromMapping(facts: {
        input: MappingInput;
    }): Promise<number | null>;
    insertReviewedMapping(facts: {
        identityId: string;
        input: MappingInput;
        p: Record<string, unknown>;
    }): Promise<void>;
}
export interface DirectoryPorts {
    error: IdentityError;
    now(): number;
    id(): string;
    companyScope(actor: IamActor): void;
    fresh(actor: IamActor): Promise<void>;
    apiError(error: unknown): error is Error & {
        code: string;
    };
    connection(companyId: string, id: string): Promise<ConnectionRow>;
    route(id: string, actor: IamActor): Promise<void>;
    test(connection: ConnectionRow): Promise<DirectoryTestResult>;
    read(connection: ConnectionRow): Promise<ProviderSnapshot>;
    testFailure(id: string, version: number, code: string): Promise<void>;
    syncFailure(connection: ConnectionRow, startedAt: number, code: string, cutoff: number): Promise<void>;
    command<T>(actor: IamActor, key: string, operation: string, payload: unknown, permission: string, run: (tx: DirectoryTransaction) => Promise<T>): Promise<T>;
    snapshotTransaction<T>(connection: ConnectionRow, run: (tx: DirectoryTransaction) => Promise<T>): Promise<T>;
    validateGrant(tx: DirectoryTransaction, actor: IamActor, input: GrantInput, parentId?: string): Promise<unknown>;
    insertGrant(tx: DirectoryTransaction, actor: IamActor, input: GrantInput, parentId: string, source?: string, sourceId?: string): Promise<unknown>;
    review(tx: DirectoryTransaction, actor: IamActor, kind: string, payload: unknown, reason: string): Promise<{
        id: string;
        status: string;
        message: string;
    }>;
}
export function createDirectories(ports: DirectoryPorts) {
    async function saveConnection(actor: IamActor, input: z.infer<typeof connectionInput>, key: string) {
        ports.companyScope(actor);
        if ((input.provider === "active_directory" && input.config.mode !== "ldaps") ||
            (input.provider === "entra_id" && input.config.mode !== "oidc"))
            throw ports.error(422, "PROVIDER_MODE_INVALID", "Select a supported authentication method.");
        if (input.provider === "entra_id") {
            if (!input.config.tenantId)
                throw ports.error(422, "TENANT_REQUIRED", "Choose a concrete Entra tenant.");
            input.config.issuer = `https://login.microsoftonline.com/${input.config.tenantId}/v2.0`;
        }
        if (input.provider !== "entra_id" &&
            (!input.config.ldapUrl?.startsWith("ldaps://") || !input.config.baseDn))
            throw ports.error(422, "LDAP_CONFIGURATION_REQUIRED", "LDAPS URL and base DN are required.");
        const result = await ports.command(actor, key, "connection.save", input, "iam.directory.manage", async (tx) => {
            const id = input.id ?? ports.id();
            const existing = (await tx.lockTrustConnection({ id }));
            if (existing?.status === "active")
                throw ports.error(409, "CONNECTION_ACTIVE", "Disable this connection before replacing its trust configuration.");
            if (Number(existing?.version ?? 0) !== input.expectedVersion)
                throw ports.error(409, "CONNECTION_CHANGED", "This connection changed. Refresh first.");
            if (existing &&
                (existing.provider !== input.provider ||
                    existing.config.authority !== input.config.authority))
                throw ports.error(409, "AUTHORITY_IMMUTABLE", "Create a new connection to change directory authority.");
            if (existing)
                (await tx.replaceTrustConnection({ input, id }));
            else
                (await tx.insertTrustConnection({ id, input, observedNow: ports.now() }));
            return { id, version: input.expectedVersion + 1, status: "draft" };
        });
        await ports.route(result.id, actor);
        return result;
    }
    async function testConnection(actor: IamActor, id: string, version: number, key: string) {
        ports.companyScope(actor);
        const connection = await ports.connection(actor.companyId, id);
        // Authorize before any outbound connection.
        await ports.command(actor, `${key}:authorize`, "connection.test.authorize", { id, version }, "iam.directory.manage", async () => ({ id }));
        let result: DirectoryTestResult;
        try {
            result = await ports.test(connection);
        }
        catch (error) {
            await ports.testFailure(id, version, ports.apiError(error) ? error.code : 'DIRECTORY_TEST_FAILED');
            throw ports.apiError(error)
                ? error
                : ports.error(502, "DIRECTORY_TEST_FAILED", "Connection test failed. Check endpoint, trust and credential profile.");
        }
        return ports.command(actor, key, "connection.test", { id, version }, "iam.directory.manage", async (tx) => {
            const update = (await tx.recordTestedConnection({ id, version }));
            if (!update.changed)
                throw ports.error(409, "CONNECTION_CHANGED", "Configuration changed during testing.");
            (await tx.storeTestPreview({ id, version, result, observedNow: ports.now() }));
            return {
                id,
                userCount: result.userCount,
                groupCount: result.groupCount,
                status: "tested",
            };
        });
    }
    async function proposeConnectionActivation(actor: IamActor, id: string, version: number, key: string) {
        ports.companyScope(actor);
        await ports.fresh(actor);
        return ports.command(actor, key, "connection.activate.propose", { id, version }, "iam.directory.manage", async (tx) => {
            const r = (await tx.activationEvidence({ id }));
            if (!r ||
                r.status !== "tested" ||
                Number(r.version) !== version ||
                Number(r.tested_version) !== version)
                throw ports.error(409, "CONNECTION_TEST_REQUIRED", "Test the exact configuration before activation.");
            return ports.review(tx, actor, "connection.activate", { id, version }, "Activate tested directory connection.");
        });
    }
    async function disableConnection(actor: IamActor, id: string, version: number, key: string) {
        ports.companyScope(actor);
        await ports.fresh(actor);
        return ports.command(actor, key, "connection.disable", { id, version }, "iam.directory.manage", async (tx) => {
            const changed = (await tx.disableTrustConnection({ id, version }));
            if (!changed.changed)
                throw ports.error(409, "CONNECTION_CHANGED", "This connection changed.");
            (await tx.invalidateConnectionMembers({ id }));
            if (!actor.platform)
                (await tx.clearConnectionSelections({ id }));
            return { id, status: "disabled" };
        });
    }
    const {applyDirectorySnapshot,synchronizeConnection}=createDirectorySynchronization(ports);

    async function proposeMapping(actor: IamActor, input: z.infer<typeof mappingInput>, key: string) {
        await ports.fresh(actor);
        return ports.command(actor, key, "mapping.propose", input, "iam.manage", async (tx) => {
            const group = (await tx.knownMappingGroup({ input }));
            if (!group) {
                const preview = (await tx.mappingPreview({ input }));
                if (!preview?.snapshot_json.groups.some((g: {
                    externalId: string;
                }) => g.externalId === input.groupId))
                    throw ports.error(404, "GROUP_NOT_FOUND", "Test and preview the selected group first.");
            }
            const grant = grantInput.parse({
                ...input,
                userId: "directory-mapping-target",
            });
            await ports.validateGrant(tx, actor, grant);
            return ports.review(tx, actor, "mapping.activate", {
                ...input,
                expectedRoleRevision: grant.expectedRoleRevision,
                parentId: actor.assignmentId,
            }, input.reason);
        });
    }
    async function applyDirectoryReview(tx: DirectoryTransaction, actor: IamActor, kind: string, p: Record<string, unknown>) {
        if (kind === "connection.activate") {
            const changed = (await tx.activateTestedConnection({ p }));
            if (!changed.changed)
                throw ports.error(409, "CONNECTION_CHANGED", "Retest and propose the current configuration.");
            (await tx.scheduleConnectionSync({ p }));
        }
        else if (kind === "mapping.activate") {
            const input = mappingInput.parse(p);
            if ((await tx.reviewerBenefitsFromMapping({ input })))
                throw ports.error(403, "INDEPENDENT_REVIEW_REQUIRED", "A member benefiting from this group mapping cannot approve it.");
            await ports.validateGrant(tx, actor, grantInput.parse({ ...input, userId: "directory-mapping-target" }), String(p.parentId));
            (await tx.insertReviewedMapping({ identityId: ports.id(), input, p }));
        }
        else
            throw ports.error(409, "REVIEW_UNSUPPORTED", "The review kind is not supported.");
    }
    return { saveConnection, testConnection, proposeConnectionActivation, disableConnection, applyDirectorySnapshot, synchronizeConnection, proposeMapping, applyDirectoryReview };
}
export function createDirectoryJobDrain(ports: {
    one(): Promise<void | false>;
}) { return async () => { let remaining = 48; await Promise.all(Array.from({ length: 3 }, async () => { while (remaining-- > 0)
    if (await ports.one() === false)
        break; })); }; }




export interface DirectoryJobPorts {
    now(): number;
    token(): string;
    claim(now: number, token: string, leaseUntil: number): Promise<{ connection_id: string } | null>;
    connection(id: string): Promise<ConnectionRow | null>;
    renewing(connection: ConnectionRow, token: string, run: () => Promise<void>): Promise<void>;
    synchronize(connection: ConnectionRow): Promise<void>;
    complete(connectionId: string, token: string, nextRun: number): Promise<void>;
}
export function createRunDirectoryJob(ports: DirectoryJobPorts) {
    return async (): Promise<void | false> => {
        const now = ports.now(), token = ports.token();
        const row = await ports.claim(now, token, now + 120000);
        if (!row) return false;
        const connection = await ports.connection(row.connection_id);
        if (!connection) return false;
        try {
            await ports.renewing(connection, token, () => ports.synchronize(connection));
        } catch {
            // Synchronization records bounded provider failures before the lease ends.
        } finally {
            await ports.complete(connection.id, token, ports.now() + 300000);
        }
    };
}
