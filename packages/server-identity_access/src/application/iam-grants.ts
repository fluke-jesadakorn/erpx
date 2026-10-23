import type { z } from 'zod';
import type { grantInput } from '@erpx/server-identity_access/domain/iam-service';
import type { IamActor } from '@erpx/server-identity_access/domain/iam-actor';
import { permissionCatalog } from '@erpx/feature-identity_access/lib/permissions';
import type { IamServiceTransaction, IamServiceCommandPorts } from '@erpx/server-identity_access/application/iam-service-commands';
export type IamGrantTransaction = Pick<IamServiceTransaction, 'grantSamePerson' | 'lockGrantParent' | 'grantParentActive' | 'grantReviewerExpiry' | 'grantParentPermissions' | 'grantTargetMember' | 'grantPublishedRole' | 'grantScopeNode' | 'grantModulePermissions' | 'insertPlatformGrant' | 'insertCompanyGrant'>;
export function createIamGrantPolicy(ports: Pick<IamServiceCommandPorts, 'error' | 'now' | 'id'>) {
    function fail(code: string, message: string): never { throw ports.error(409, code, message); }
async function validateIamGrant(tx: IamGrantTransaction, actor: IamActor, input: z.infer<typeof grantInput>, parentId = actor.assignmentId) {
        if (input.userId === actor.userId ||
            (await tx.grantSamePerson({ input })).same)
            throw ports.error(403, "SELF_ACCESS_CHANGE", "Another administrator must change your access.");
        const parent = (await tx.lockGrantParent({ parentId }));
        if (!parent ||
            !(await tx.grantParentActive({ parentId, observedNow: ports.now() })).ok)
            fail("PARENT_EXPIRED", "The authorizing assignment is no longer active.");
        const reviewer = (await tx.grantReviewerExpiry({}));
        if (input.validUntil === 0)
            input.validUntil = Math.min(ports.now() + 90 * 86400000, Number(parent.valid_until), Number(reviewer?.valid_until ?? parent.valid_until));
        if (input.validUntil <= ports.now() ||
            input.validUntil > Number(parent.valid_until) ||
            (reviewer && input.validUntil > Number(reviewer.valid_until)))
            throw ports.error(422, "EXPIRY_INVALID", "Access must expire within the authorizing assignment.");
        const parentRole = (await tx.grantParentPermissions({ parent }));
        if (!parentRole)
            fail("PARENT_ROLE_UNAVAILABLE", "The authorizing role is unavailable.");
        const member = (await tx.grantTargetMember({ input }));
        if (input.userId.startsWith("dir_") && member?.login_policy !== "directory")
            throw ports.error(403, "DIRECTORY_IDENTITY_REVIEW_REQUIRED", "Connect and review this directory identity in this company before assigning access.");
        if (member && member.status !== "active")
            throw ports.error(403, "IDENTITY_INACTIVE", "Suspended or superseded identities cannot receive new access.");
        const role = (await tx.grantPublishedRole({ input }));
        if (!role || !role.scopes_json.includes(input.scopeKind))
            fail("ROLE_UNAVAILABLE", "Choose an active role and supported scope.");
        if (input.expectedRoleRevision !== undefined &&
            input.expectedRoleRevision !== Number(role.published_revision))
            fail("ROLE_CHANGED", "The role changed. Review its current permissions before granting access.");
        input.expectedRoleRevision = Number(role.published_revision);
        if ([...role.permissions_json, ...role.grantable_json].some((p: string) => !actor.iam?.grantablePermissions.includes(p) ||
            !parentRole.grantable_json.includes(p)))
            throw ports.error(403, "DELEGATION_EXCEEDED", "This role exceeds your delegated grant permissions.");
        if (!actor.platform) {
            if (parent.company_id !== actor.companyId)
                throw ports.error(403, "SCOPE_FORBIDDEN", "The parent belongs to another company.");
            if (input.scopeKind === "platform")
                throw ports.error(403, "SCOPE_FORBIDDEN", "Platform access is separate.");
            if (input.scopeKind !== "holding") {
                const node = (await tx.grantScopeNode({ input }));
                if (!node)
                    fail("SCOPE_UNAVAILABLE", "Choose an active organization scope.");
                if ((parent.scope_kind === "department" &&
                    parent.scope_node_id !== node.id) ||
                    (parent.scope_kind === "branch" &&
                        parent.scope_node_id !==
                            (node.kind === "branch" ? node.id : node.branch_id)) ||
                    (actor.scopeKind === "department" && actor.scopeNodeId !== node.id) ||
                    (actor.scopeKind === "branch" &&
                        actor.scopeNodeId !==
                            (node.kind === "branch" ? node.id : node.branch_id)))
                    throw ports.error(403, "SCOPE_FORBIDDEN", "The grant exceeds its parent scope.");
            }
            else if (parent.scope_kind !== "holding" || actor.scopeKind !== "holding")
                throw ports.error(403, "SCOPE_FORBIDDEN", "The grant exceeds its parent scope.");
        }
        return role;
    }
async function insertIamGrant(tx: IamGrantTransaction, actor: IamActor, input: z.infer<typeof grantInput>, parentId: string, source = "manual", sourceId: string | null = null) {
        const id = `iam-${ports.id()}`, modules = [
            ...new Set(input.roleId === "holding_owner"
                ? permissionCatalog
                    .filter((p) => p.id.startsWith("module."))
                    .map((p) => p.module)
                : (await tx.grantModulePermissions({ input })).flatMap((r) => (r.permissions_json as string[])
                    .filter((p) => p.startsWith("module."))
                    .map((p) => p.split(".")[1]))),
        ];
        if (actor.platform)
            (await tx.insertPlatformGrant({ id, input, modules, observedNow: ports.now(), parentId, source, sourceId }));
        else
            (await tx.insertCompanyGrant({ id, input, modules, observedNow: ports.now(), parentId, source, sourceId }));
        return id;
    }
return {validateIamGrant,insertIamGrant};
}
