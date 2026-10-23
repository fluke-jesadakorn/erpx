import 'server-only';
import type {PoolClient} from 'pg';
import type {IamActor} from '@erpx/server-identity_access/domain/iam-actor';
import type {IamGrantTransaction} from '@erpx/server-identity_access/application/iam-grants';
import {identitySnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
const organizationSources=identitySnapshotSources();
export function grantTransaction(c: PoolClient, actor: IamActor): IamGrantTransaction {
return {grantSamePerson: async (facts) => {
            const { input } = facts;
            return (await c.query("SELECT iam_same_person($1,$2,$3) same", [
                input.userId,
                actor.userId,
                actor.companyId,
            ])).rows[0] ?? null;
        },
lockGrantParent: async (facts) => { const { parentId } = facts; return (await c.query(`SELECT * FROM ${actor.platform ? "platform_assignments" : "assignments"} WHERE id=$1 FOR SHARE`, [parentId])).rows[0] ?? null; },
grantParentActive: async (facts) => {
            const { parentId, observedNow } = facts;
            return (await c.query("SELECT iam_assignment_active($1,$2) ok", [
                parentId,
                observedNow,
            ])).rows[0] ?? null;
        },
grantReviewerExpiry: async (facts) => {
            const {} = facts;
            return (await c.query(`SELECT valid_until FROM ${actor.platform ? "platform_assignments" : "assignments"} WHERE id=$1`, [
                actor.assignmentId,
            ])).rows[0] ?? null;
        },
grantParentPermissions: async (facts) => {
            const { parent } = facts;
            return (await c.query("SELECT v.grantable_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.id=$1 AND r.company_id IN ('*',$2) AND r.status='active'", [parent.role, actor.companyId])).rows[0] ?? null;
        },
grantTargetMember: async (facts) => {
            const { input } = facts;
            return (await c.query("SELECT status,login_policy FROM iam_members WHERE company_id=$1 AND user_id=$2", [actor.companyId, input.userId])).rows[0] ?? null;
        },
grantPublishedRole: async (facts) => {
            const { input } = facts;
            return (await c.query("SELECT r.*,v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id IN ('*',$1) AND r.id=$2 AND r.status='active'", [actor.companyId, input.roleId])).rows[0] ?? null;
        },
grantScopeNode: async (facts) => {
            const { input } = facts;
            return (await c.query(`SELECT * ${organizationSources.iam_service_organization_nodes_from} WHERE id=$1 AND company_id=$2 AND kind=$3 AND status='active'`, [input.scopeNodeId, actor.companyId, input.scopeKind])).rows[0] ?? null;
        },
grantModulePermissions: async (facts) => {
            const { input } = facts;
            return (await c.query("SELECT v.permissions_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.id=$1 AND r.company_id IN ('*',$2)", [input.roleId, actor.companyId])).rows;
        },
insertPlatformGrant: async (facts) => {
            const { id, input, modules, observedNow, parentId, source, sourceId } = facts;
            return void await c.query("INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until,parent_id,source,source_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)", [
                id,
                input.userId,
                input.roleId,
                JSON.stringify(modules),
                observedNow,
                input.validUntil,
                parentId,
                source,
                sourceId,
            ]);
        },
insertCompanyGrant: async (facts) => {
            const { id, input, modules, observedNow, parentId, source, sourceId } = facts;
            return void await c.query(`INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,scope_kind,scope_node_id,parent_id,source,source_id) VALUES($1,$2,$3,$4,$5,(SELECT CASE WHEN kind='branch' THEN id ELSE branch_id END ${organizationSources.iam_service_organization_nodes_from} WHERE id=$6),$7,$8,$9,$6,$10,$11,$12)`, [
                id,
                input.userId,
                input.roleId,
                JSON.stringify(modules),
                actor.companyId,
                input.scopeNodeId,
                observedNow,
                input.validUntil,
                input.scopeKind,
                parentId,
                source,
                sourceId,
            ]);
        }};
}
