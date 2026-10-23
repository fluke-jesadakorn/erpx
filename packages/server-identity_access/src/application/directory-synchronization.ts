import type {ConnectionRow} from '@erpx/server-identity_access/domain/directories';
import type {ProviderSnapshot} from '@erpx/server-identity_access/domain/providers';
import type {IamActor} from '@erpx/server-identity_access/domain/iam-actor';
import {grantInput} from '@erpx/server-identity_access/domain/iam-service';
import {emptyDirectoryHierarchy,validateDirectoryHierarchy} from '@erpx/feature-organization/lib/hierarchy';
import type {DirectoryPorts} from '@erpx/server-identity_access/application/directories';
export type DirectorySynchronizationPorts = Pick<DirectoryPorts,'error'|'now'|'id'|'apiError'|'read'|'syncFailure'|'snapshotTransaction'|'validateGrant'|'insertGrant'>;
export function createDirectorySynchronization(ports: DirectorySynchronizationPorts) {
async function applyDirectorySnapshot(connection: ConnectionRow, snapshot: ProviderSnapshot, observedAt = ports.now()) {
        if (snapshot.complete !== true)
            throw ports.error(502, "DIRECTORY_INCOMPLETE", "A complete directory snapshot is required.");
        const now = observedAt;
        return ports.snapshotTransaction(connection, async (tx) => {
            const row = (await tx.lockSnapshotConnection({ connection }));
            if (!row ||
                row.status !== "active" ||
                Number(row.version) !== Number(connection.version))
                throw ports.error(409, "CONNECTION_CHANGED", "The connection changed during synchronization.");
            if (Number(row.last_sync_at) > now) {
                return;
            }
            if (new Set(snapshot.users.map((u) => u.externalId)).size !==
                snapshot.users.length ||
                new Set(snapshot.groups.map((g) => g.externalId)).size !==
                    snapshot.groups.length)
                throw ports.error(502, "DIRECTORY_DUPLICATE_IDENTITY", "The directory returned duplicate immutable identifiers.");
            const hierarchy = snapshot.hierarchy ?? emptyDirectoryHierarchy();
            validateDirectoryHierarchy(hierarchy, snapshot.users.map(u => u.externalId), snapshot.groups.map(g => g.externalId));
            if (snapshot.users.some(u => u.groupIds.some(id => !snapshot.groups.some(g => g.externalId === id))))
                throw ports.error(502, "DIRECTORY_INCOMPLETE", "A membership refers to a missing directory group.");
            (await tx.storeDirectoryHierarchy({ connection, hierarchy, now }));
            for (const group of snapshot.groups)
                (await tx.observeDirectoryGroup({ connection, group, now }));
            for (const user of snapshot.users)
                (await tx.observeDirectoryIdentity({ identityId: ports.id(), connection, user, now }));
            (await tx.disableMissingIdentities({ now, connection, snapshot }));
            (await tx.removeMissingGroups({ connection, snapshot }));
            (await tx.revokeRemovedMemberships({ connection, now }));
            // Imports are pending identities. Mapping grants apply only after reviewed migration.
            const candidates = (await tx.mappingCandidates({ connection, now }));
            for (const m of candidates) {
                if ((await tx.existingMappingGrant({ connection, m })))
                    continue;
                const parent = (await tx.mappingParent({ connection, m }));
                if (Number(m.valid_until) > Number(parent.valid_until))
                    continue;
                const parentRole = (await tx.mappingParentPolicy({ parent, connection }));
                const actor = {
                    userId: parent.user_id,
                    assignmentId: parent.id,
                    scopeKind: parent.scope_kind ?? "platform",
                    scopeNodeId: parent.scope_node_id ?? null,
                    role: parent.role,
                    companyId: connection.company_id,
                    platform: connection.company_id === "platform",
                    iam: { grantablePermissions: parentRole?.grantable_json ?? [] },
                } as IamActor;
                try {
                    await ports.validateGrant(tx, actor, grantInput.parse({
                        userId: m.user_id,
                        roleId: m.role_id,
                        scopeKind: m.scope_kind,
                        scopeNodeId: m.scope_node_id,
                        validUntil: Number(m.valid_until),
                        reason: "Approved directory mapping",
                    }), m.parent_id);
                }
                catch (e) {
                    if (ports.apiError(e))
                        continue;
                    throw e;
                }
                await ports.insertGrant(tx, actor, grantInput.parse({
                    userId: m.user_id,
                    roleId: m.role_id,
                    scopeKind: m.scope_kind,
                    scopeNodeId: m.scope_node_id,
                    validUntil: Number(m.valid_until),
                    reason: "Approved directory mapping",
                }), m.parent_id, "directory", m.id);
            }
            (await tx.recordHealthySync({ now, connection }));
            (await tx.recordSyncAudit({ identityId: ports.id(), connection, snapshot, now }));
        });
    }
async function synchronizeConnection(connection: ConnectionRow) {
        const startedAt = ports.now();
        try {
            await applyDirectorySnapshot(connection, await ports.read(connection), startedAt);
        }
        catch (error) {
            await ports.syncFailure(connection, startedAt, ports.apiError(error) ? error.code : 'DIRECTORY_SYNC_FAILED', ports.now() - 300000);
            throw error;
        }
    }
return {applyDirectorySnapshot,synchronizeConnection};
}
