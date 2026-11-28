import 'server-only';
import { samePersonQuerySql, assignmentActiveQuerySql, publishedRoleSql } from "./iam-probe-sql";
import { assignmentValidUntilSql, memberByCompanyUserSql } from "./read-tail-family-r27";
import { nativeIdentityWrite } from '@erpx/identity-access-server/public/native-identity-write';
import type {PoolClient} from 'pg';
import type {IamActor} from '@erpx/identity-access-server/domain/iam-actor';
import type {IamGrantTransaction} from '@erpx/identity-access-server/application/iam-grants';
import {identitySnapshotSources} from '@erpx/organization-server/public/identity-snapshot-sources';
const organizationSources=identitySnapshotSources();
export function grantTransaction(c: PoolClient, actor: IamActor): IamGrantTransaction {
return {grantSamePerson: async (facts) => {
            const { input } = facts;
            return (await c.query(samePersonQuerySql, [
                input.userId,
                actor.userId,
                actor.companyId,
            ])).rows[0] ?? null;
        },
lockGrantParent: async (facts) => { const { parentId } = facts; return (await c.query(`SELECT * FROM ${actor.platform ? "platform_assignments" : "assignments"} WHERE id=$1 FOR SHARE`, [parentId])).rows[0] ?? null; },
grantParentActive: async (facts) => {
            const { parentId, observedNow } = facts;
            return (await c.query(assignmentActiveQuerySql, [
                parentId,
                observedNow,
            ])).rows[0] ?? null;
        },
grantReviewerExpiry: async (facts) => {
            const {} = facts;
            return (await c.query(assignmentValidUntilSql(actor.platform ? "platform_assignments" : "assignments"), [
                actor.assignmentId,
            ])).rows[0] ?? null;
        },
grantParentPermissions: async (facts) => {
            const { parent } = facts;
            return (await c.query("SELECT v.grantable_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.id=$1 AND r.company_id IN ('*',$2) AND r.status='active'", [parent.role, actor.companyId])).rows[0] ?? null;
        },
grantTargetMember: async (facts) => {
            const { input } = facts;
            return (await c.query(memberByCompanyUserSql('status,login_policy', '$'), [actor.companyId, input.userId])).rows[0] ?? null;
        },
grantPublishedRole: async (facts) => {
            const { input } = facts;
            return (await c.query(publishedRoleSql('$1', '$2'), [actor.companyId, input.roleId])).rows[0] ?? null;
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
            return void await nativeIdentityWrite("w_4c188dafc6ad", [
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
            return void await nativeIdentityWrite("w_iam_grant_insert_nodes", [
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
