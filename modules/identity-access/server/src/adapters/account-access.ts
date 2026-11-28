import { nativePrepare } from '@erpx/identity-access-server/public/native-identity-write';
import { assignmentVersionSql } from './read-tail-family-r28';
import { createHmac } from "node:crypto";
import { iamCommand } from "@erpx/identity-access-server/public/iam-transactions";
import { requireFreshMfa } from "@erpx/identity-access-server/public/iam-mfa";
import type { PostgresDatabase } from "@erpx/platform-runtime-server/public/runtime/postgres";
import { iamActorFromWork } from "@erpx/identity-access-server/public/iam-context";
import { grantIamAccess, grantInput, revokeIamAssignment, proposeIamReview } from "@erpx/identity-access-server/public/iam-service";
import {validateIamGrant} from '@erpx/identity-access-server/public/iam-grants';
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { runtimeEnv, holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { directoryPerson } from "@erpx/people-payroll-server/public/people-directory";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { createAccountAccessService } from '@erpx/identity-access-server/application/account-access';
import type { PersonGrant } from '@erpx/identity-access-server/domain/account-access';
import {nativeEmailExists,nativeInvitationByToken,nativeInvitationStatus} from './native-lane-10/client';
export { grantSchema, acceptInvitationSchema } from '@erpx/identity-access-server/domain/account-access';
async function hash(value: string) {
    const b = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
    return Array.from(new Uint8Array(b), (v) => v.toString(16).padStart(2, "0")).join("");
}
function service() {
    let write: Awaited<ReturnType<typeof holdingResources>>['write'], env: Awaited<ReturnType<typeof runtimeEnv>>, iam: ReturnType<typeof iamActorFromWork>, scope: WorkContext | undefined;
    const environment = async () => env ??= await runtimeEnv();
    const workspace = async (actor: WorkContext) => { iam = iamActorFromWork(actor); write = (await holdingResources(actor.holdingId)).write; };
    return createAccountAccessService({ error: (...args) => new ApiError(...args), now: Date.now, owner: (actor) => { scope = actor; return requirePermission(actor, 'iam.manage'); }, person: directoryPerson, digest: hash, token: async (id) => createHmac('sha256', (await environment()).BETTER_AUTH_SECRET).update('invitation:' + id).digest('hex'), emailExists: async (email) => nativeEmailExists(email, scope), prepare: workspace, fresh: async (actor) => requireFreshMfa(iamActorFromWork(actor)),
        grant: async (_actor, input, key) => grantIamAccess(write as PostgresDatabase, iam, grantInput.parse(input), key), target: async (actor, userId, assignmentId) => write.prepare(assignmentVersionSql('company_id=?')).bind(assignmentId, userId, actor.companyId).first<{
            version: number;
        }>(), revoke: async (_actor, id, version, key) => revokeIamAssignment(write as PostgresDatabase, iam, id, version, 'Revoked from employee profile', key), reserve: async (_actor, key, operation, payload, id) => { await iamCommand(write as PostgresDatabase, iam, key, operation, payload, 'iam.manage', async () => ({ id })); },
        store: async (actor, personId, person, input, id, tokenHash, expiry) => {
            await Promise.all([
                nativePrepare("w_49035caf9839").bind(actor.holdingId, personId, id),
                nativePrepare("invitation_delivery_cancel_superseded").bind(Date.now(), actor.holdingId, personId, id),
                nativePrepare("w_a45387e378ed").bind(id, tokenHash, actor.holdingId, actor.companyId, personId, person.email, person.legal_name, JSON.stringify({ role: input.role, expires: expiry }), input.scope, input.scope !== "holding" ? person.branch_id : null, actor.userId, actor.assignmentId, Date.now() + 48 * 3600000, Date.now(), input.scope === "department" ? person.department_id : null),
                nativePrepare("w_66d09ee20563").bind(`account-invite-delivery-${id}`, id, actor.holdingId, actor.companyId, Date.now(), Date.now(), Date.now()),
            ]);
        },
        propose: async (_actor, id, input) => { const requested = grantInput.parse(input); return iamCommand(write as PostgresDatabase, iam, `invite-review-${id}`, 'invitation.propose', { invitationId: id, ...requested }, 'iam.manage', async (c) => { await validateIamGrant(c, iam, requested); return proposeIamReview(c, iam, 'invitation.authorize', { invitationId: id, ...requested, parentId: iam.assignmentId }, 'Authorize an employee invitation and initial grant.'); }); },
        retry: async (actor, personId, invitationId) => { const env = await environment(); return !!await nativePrepare("w_ab23e7e4dc0e").bind(Date.now(), Date.now(), invitationId, actor.holdingId, actor.companyId, invitationId, personId, Date.now()).first(); }, revokeInvitation: async (actor, personId, invitationId) => { const env = await environment(); return !!await nativePrepare("w_8fa09e34f16f").bind(invitationId, actor.holdingId, actor.companyId, personId).first(); }, invitationStatus: async (actor, personId, invitationId) => nativeInvitationStatus(actor, personId, invitationId),
        cancelDelivery: async (invitationId) => { const env = await environment(); await nativePrepare("w_7304093cba71").bind(Date.now(), invitationId).run(); },
        invitation: async (tokenHash, now) => nativeInvitationByToken(tokenHash, now) });
}
export const grantPersonAccess = (actor: WorkContext, personId: string, input: PersonGrant, key: string) => service().grantPersonAccess(actor, personId, input, key);
export const revokePersonAccess = (actor: WorkContext, personId: string, assignmentId: string, key: string) => service().revokePersonAccess(actor, personId, assignmentId, key);
export const invitePerson = (actor: WorkContext, personId: string, input: PersonGrant, key: string) => service().invitePerson(actor, personId, input, key);
export const retryInvitationDelivery = (actor: WorkContext, personId: string, invitationId: string, key: string) => service().retryInvitationDelivery(actor, personId, invitationId, key);
export const revokeInvitation = (actor: WorkContext, personId: string, invitationId: string, key: string) => service().revokeInvitation(actor, personId, invitationId, key);
export const inspectInvitation = (token: string) => service().inspectInvitation(token);
