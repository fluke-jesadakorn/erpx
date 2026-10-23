import 'server-only';
import type {PoolClient} from 'pg';
import type {IamActor} from '@erpx/server-identity_access/domain/iam-actor';
import type {DirectoryTransaction} from '@erpx/server-identity_access/application/directories';
export function directoryTx(c: PoolClient, actor?: IamActor): DirectoryTransaction {
    const tx: DirectoryTransaction = { lockTrustConnection: async (facts) => {
            if (!actor)
                throw new Error("IAM actor required for command transaction");
            const { id } = facts;
            return (await c.query("SELECT * FROM iam_connections WHERE id=$1 AND company_id=$2 FOR UPDATE", [id, actor.companyId])).rows[0] ?? null;
        },
        replaceTrustConnection: async (facts) => {
            const { input, id } = facts;
            return void await c.query("UPDATE iam_connections SET name=$1,config=$2,status='draft',version=version+1,tested_version=NULL,health='untested',last_error=NULL WHERE id=$3", [input.name, JSON.stringify(input.config), id]);
        },
        insertTrustConnection: async (facts) => {
            if (!actor)
                throw new Error("IAM actor required for command transaction");
            const { id, input, observedNow } = facts;
            return void await c.query("INSERT INTO iam_connections(id,company_id,name,provider,config,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)", [
                id,
                actor.companyId,
                input.name,
                input.provider,
                JSON.stringify(input.config),
                actor.userId,
                observedNow,
            ]);
        },
        recordTestedConnection: async (facts) => {
            if (!actor)
                throw new Error("IAM actor required for command transaction");
            const { id, version } = facts;
            return { changed: !!(await c.query("UPDATE iam_connections SET tested_version=version,status=CASE WHEN status='active' THEN status ELSE 'tested' END,health='ready',last_error=NULL WHERE id=$1 AND company_id=$2 AND version=$3 RETURNING id", [id, actor.companyId, version])).rowCount };
        },
        storeTestPreview: async (facts) => {
            const { id, version, result, observedNow } = facts;
            return void await c.query("INSERT INTO iam_connection_previews VALUES($1,$2,$3,$4) ON CONFLICT(connection_id) DO UPDATE SET version=excluded.version,snapshot_json=excluded.snapshot_json,created_at=excluded.created_at", [id, version, JSON.stringify(result.snapshot), observedNow]);
        },
        activationEvidence: async (facts) => {
            if (!actor)
                throw new Error("IAM actor required for command transaction");
            const { id } = facts;
            return (await c.query("SELECT * FROM iam_connections WHERE id=$1 AND company_id=$2", [id, actor.companyId])).rows[0] ?? null;
        },
        disableTrustConnection: async (facts) => {
            if (!actor)
                throw new Error("IAM actor required for command transaction");
            const { id, version } = facts;
            return { changed: !!(await c.query("UPDATE iam_connections SET status='disabled',version=version+1,health='disabled' WHERE id=$1 AND company_id=$2 AND version=$3 RETURNING id", [id, actor.companyId, version])).rowCount };
        },
        invalidateConnectionMembers: async (facts) => {
            if (!actor)
                throw new Error("IAM actor required for command transaction");
            const { id } = facts;
            return void await c.query("UPDATE iam_members SET version=version+1 WHERE connection_id=$1 AND company_id=$2", [id, actor.companyId]);
        },
        clearConnectionSelections: async (facts) => {
            if (!actor)
                throw new Error("IAM actor required for command transaction");
            const { id } = facts;
            return void await c.query("UPDATE active_work_contexts SET assignment_id=NULL,assignment_version=NULL,revision=revision+1 WHERE company_id=$1 AND user_id IN (SELECT user_id FROM iam_members WHERE connection_id=$2)", [actor.companyId, id]);
        },
        lockSnapshotConnection: async (facts) => {
            const { connection } = facts;
            return (await c.query("SELECT * FROM iam_connections WHERE id=$1 FOR UPDATE", [
                connection.id,
            ])).rows[0] ?? null;
        },
        storeDirectoryHierarchy: async (facts) => { const { connection, hierarchy, now } = facts; return void await c.query("INSERT INTO iam_directory_hierarchies(connection_id,hierarchy_json,observed_at) VALUES($1,$2,$3) ON CONFLICT(connection_id) DO UPDATE SET hierarchy_json=excluded.hierarchy_json,observed_at=excluded.observed_at", [connection.id, JSON.stringify(hierarchy), now]); },
        observeDirectoryGroup: async (facts) => {
            const { connection, group, now } = facts;
            return void await c.query("INSERT INTO iam_groups VALUES($1,$2,$3,$4) ON CONFLICT(connection_id,external_id) DO UPDATE SET name=excluded.name,observed_at=excluded.observed_at", [connection.id, group.externalId, group.name, now]);
        },
        observeDirectoryIdentity: async (facts) => {
            const { identityId, connection, user, now } = facts;
            return void await c.query("INSERT INTO iam_identities(id,company_id,connection_id,external_id,username,name,email,enabled,groups_json,observed_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(connection_id,external_id) DO UPDATE SET username=excluded.username,name=excluded.name,email=excluded.email,enabled=excluded.enabled,groups_json=excluded.groups_json,observed_at=excluded.observed_at", [
                identityId,
                connection.company_id,
                connection.id,
                user.externalId,
                user.username,
                user.name,
                user.email,
                user.enabled,
                JSON.stringify(user.groupIds),
                now,
            ]);
        },
        disableMissingIdentities: async (facts) => {
            const { now, connection, snapshot } = facts;
            return void await c.query("UPDATE iam_identities SET enabled=false,groups_json='[]',observed_at=$1 WHERE connection_id=$2 AND NOT (external_id=ANY($3::text[]))", [now, connection.id, snapshot.users.map((u) => u.externalId)]);
        },
        removeMissingGroups: async (facts) => {
            const { connection, snapshot } = facts;
            return void await c.query("DELETE FROM iam_groups WHERE connection_id=$1 AND NOT (external_id=ANY($2::text[]))", [connection.id, snapshot.groups.map((g) => g.externalId)]);
        },
        revokeRemovedMemberships: async (facts) => {
            const { connection, now } = facts;
            return void await c.query(`UPDATE ${connection.company_id === "platform" ? "platform_assignments" : "assignments"} a SET revoked_at=$1,version=version+1 WHERE a.source='directory' AND a.revoked_at IS NULL AND a.source_id IN (SELECT id FROM iam_mappings WHERE connection_id=$2) AND NOT EXISTS(SELECT 1 FROM iam_mappings m JOIN iam_identities i ON i.connection_id=m.connection_id AND i.user_id=a.user_id WHERE m.id=a.source_id AND m.status='active' AND m.valid_until>$1 AND i.enabled AND i.groups_json @> jsonb_build_array(m.group_id) AND iam_assignment_active(m.parent_id,$1))`, [now, connection.id]);
        },
        mappingCandidates: async (facts) => {
            const { connection, now } = facts;
            return (await c.query("SELECT m.*,i.user_id,i.person_id FROM iam_mappings m JOIN iam_identities i ON i.connection_id=m.connection_id JOIN iam_members u ON u.identity_id=i.id AND u.company_id=m.company_id AND u.user_id=i.user_id WHERE m.connection_id=$1 AND m.status='active' AND m.valid_until>$2 AND i.enabled AND i.groups_json @> jsonb_build_array(m.group_id) AND u.status='active' AND u.login_policy='directory' AND iam_assignment_active(m.parent_id,$2)", [connection.id, now])).rows;
        },
        existingMappingGrant: async (facts) => {
            const { connection, m } = facts;
            return (await c.query(`SELECT 1 FROM ${connection.company_id === "platform" ? "platform_assignments" : "assignments"} WHERE user_id=$1 AND source='directory' AND source_id=$2`, [m.user_id, m.id])).rowCount;
        },
        mappingParent: async (facts) => { const { connection, m } = facts; return (await c.query(`SELECT * FROM ${connection.company_id === "platform" ? "platform_assignments" : "assignments"} WHERE id=$1`, [m.parent_id])).rows[0] ?? null; },
        mappingParentPolicy: async (facts) => {
            const { parent, connection } = facts;
            return (await c.query("SELECT v.grantable_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.id=$1 AND r.company_id IN ('*',$2)", [parent.role, connection.company_id])).rows[0] ?? null;
        },
        recordHealthySync: async (facts) => {
            const { now, connection } = facts;
            return void await c.query("UPDATE iam_connections SET last_sync_at=$1,health='healthy',last_error=NULL WHERE id=$2", [now, connection.id]);
        },
        recordSyncAudit: async (facts) => {
            const { identityId, connection, snapshot, now } = facts;
            return void await c.query("INSERT INTO iam_audit VALUES($1,$2,$3,$4,$5,$6,$7,$8)", [
                identityId,
                connection.company_id,
                "directory-sync",
                "system",
                "directory.synchronized",
                connection.id,
                JSON.stringify({
                    users: snapshot.users.length,
                    groups: snapshot.groups.length,
                    version: connection.version,
                }),
                now,
            ]);
        },
        knownMappingGroup: async (facts) => {
            if (!actor)
                throw new Error("IAM actor required for command transaction");
            const { input } = facts;
            return (await c.query("SELECT 1 FROM iam_groups g JOIN iam_connections c ON c.id=g.connection_id WHERE c.company_id=$1 AND c.id=$2 AND g.external_id=$3", [actor.companyId, input.connectionId, input.groupId])).rowCount;
        },
        mappingPreview: async (facts) => {
            if (!actor)
                throw new Error("IAM actor required for command transaction");
            const { input } = facts;
            return (await c.query("SELECT p.snapshot_json FROM iam_connection_previews p JOIN iam_connections c ON c.id=p.connection_id AND c.version=p.version WHERE c.id=$1 AND c.company_id=$2", [input.connectionId, actor.companyId])).rows[0] ?? null;
        },
        activateTestedConnection: async (facts) => {
            if (!actor)
                throw new Error("IAM actor required for command transaction");
            const { p } = facts;
            return { changed: !!(await c.query("UPDATE iam_connections SET status='active',health='awaiting_sync' WHERE id=$1 AND company_id=$2 AND version=$3 AND tested_version=version AND status='tested' RETURNING id", [p.id, actor.companyId, p.version])).rowCount };
        },
        scheduleConnectionSync: async (facts) => {
            const { p } = facts;
            return void await c.query("INSERT INTO iam_sync_jobs(connection_id,next_run_at) VALUES($1,0) ON CONFLICT(connection_id) DO UPDATE SET next_run_at=0", [p.id]);
        },
        reviewerBenefitsFromMapping: async (facts) => {
            if (!actor)
                throw new Error("IAM actor required for command transaction");
            const { input } = facts;
            return (await c.query("SELECT 1 FROM iam_identities WHERE company_id=$1 AND connection_id=$2 AND groups_json @> jsonb_build_array($3::text) AND user_id IS NOT NULL AND iam_same_person(user_id,$4,$1)", [actor.companyId, input.connectionId, input.groupId, actor.userId])).rowCount;
        },
        insertReviewedMapping: async (facts) => {
            if (!actor)
                throw new Error("IAM actor required for command transaction");
            const { identityId, input, p } = facts;
            return void await c.query("INSERT INTO iam_mappings(id,company_id,connection_id,group_id,role_id,scope_kind,scope_node_id,valid_until,parent_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)", [
                identityId,
                actor.companyId,
                input.connectionId,
                input.groupId,
                input.roleId,
                input.scopeKind,
                input.scopeNodeId,
                input.validUntil,
                p.parentId,
            ]);
        } };
    return tx;
}
