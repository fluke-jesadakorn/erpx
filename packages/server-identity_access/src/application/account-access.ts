import type { WorkContext } from '@erpx/server-identity_access/domain/authorization';
import { assertGrantScope, type Invitation, type PersonFact, type PersonGrant } from '@erpx/server-identity_access/domain/account-access';
import type { IdentityError } from '@erpx/server-identity_access/application/authorization';
import type { BusinessRole } from "@erpx/feature-identity_access/lib/business-roles";
export type PersonGrantRequest = {
    userId: string;
    roleId: BusinessRole;
    scopeKind: 'holding' | 'branch' | 'department';
    scopeNodeId: string | null;
    validUntil: number;
    reason: string;
};
export interface AccountAccessPorts {
    owner(actor: WorkContext): WorkContext;
    person(actor: WorkContext, id: string): Promise<PersonFact>;
    error: IdentityError;
    now(): number;
    digest(value: string): Promise<string>;
    token(id: string): Promise<string>;
    emailExists(email: string): Promise<boolean>;
    prepare(actor: WorkContext): Promise<void>;
    fresh(actor: WorkContext): Promise<void>;
    grant(actor: WorkContext, input: PersonGrantRequest, key: string): Promise<Record<string, unknown>>;
    target(actor: WorkContext, userId: string | null, assignmentId: string): Promise<{
        version: number;
    } | null>;
    revoke(actor: WorkContext, id: string, version: number, key: string): Promise<Record<string, unknown>>;
    reserve(actor: WorkContext, key: string, operation: string, payload: Record<string, unknown>, id: string): Promise<void>;
    store(actor: WorkContext, personId: string, person: PersonFact, input: PersonGrant, id: string, tokenHash: string, expiry: number): Promise<void>;
    propose(actor: WorkContext, id: string, input: PersonGrantRequest): Promise<{
        id: string;
    }>;
    retry(actor: WorkContext, personId: string, id: string): Promise<boolean>;
    revokeInvitation(actor: WorkContext, personId: string, id: string): Promise<boolean>;
    invitationStatus(actor: WorkContext, personId: string, id: string): Promise<string | null>;
    cancelDelivery(id: string): Promise<void>;
    invitation(tokenHash: string, now: number): Promise<Invitation | null>;
}
const requested = (person: PersonFact, input: PersonGrant, userId: string, reason: string): PersonGrantRequest => ({ userId, roleId: input.role, scopeKind: input.scope, scopeNodeId: (input.scope === 'holding' ? null : input.scope === 'branch' ? person.branch_id : person.department_id) as string | null, validUntil: Date.parse(input.expiresAt) + 86400000, reason });
export function createAccountAccessService(ports: AccountAccessPorts) {
    const invitation = async (token: string) => { const row = await ports.invitation(await ports.digest(token), ports.now()); if (!row)
        throw ports.error(410, 'INVITATION_UNAVAILABLE', 'This invitation is expired, revoked or already accepted.'); return row; };
    return {
        grantPersonAccess: async (actor: WorkContext, personId: string, input: PersonGrant, key: string) => { const person = await ports.person(actor, personId); if (person.user_id === actor.userId)
            throw ports.error(403, 'SELF_ACCESS_CHANGE', 'Another administrator must change your access.'); assertGrantScope(actor, input, person, ports.error); if (!person.user_id)
            throw ports.error(409, 'INVITE_REQUIRED', 'Invite and activate this account first.'); if (person.status !== 'active' || person.engagement_status !== 'active')
            throw ports.error(409, 'PERSON_INACTIVE', 'Activate the employee before granting access.'); await ports.prepare(actor); return { ...await ports.grant(actor, requested(person, input, String(person.user_id), 'Reviewed access requested from employee profile'), key), personId }; },
        revokePersonAccess: async (actor: WorkContext, personId: string, assignmentId: string, key: string) => { const person = await ports.person(actor, personId); await ports.prepare(actor); const target = await ports.target(actor, person.user_id as string | null, assignmentId); if (!target)
            throw ports.error(404, 'ASSIGNMENT_NOT_FOUND', 'Assignment not found for this employee.'); return { ...await ports.revoke(actor, assignmentId, target.version, key), personId }; },
        invitePerson: async (actor: WorkContext, personId: string, input: PersonGrant, key: string) => { actor = ports.owner(actor); const person = await ports.person(actor, personId); assertGrantScope(actor, input, person, ports.error); if (person.user_id)
            throw ports.error(409, 'ACCOUNT_ALREADY_LINKED', 'This employee already has a login account.'); if (!person.email || person.status !== 'active' || person.engagement_status !== 'active')
            throw ports.error(422, 'PERSON_NOT_READY', 'An active employee with a work email is required.'); if (await ports.emailExists(String(person.email)))
            throw ports.error(409, 'ACCOUNT_EXISTS', 'That email already has an account. Existing-account linking requires identity verification.'); const expiry = Date.parse(input.expiresAt) + 86400000; if (expiry <= ports.now())
            throw ports.error(422, 'EXPIRY_INVALID', 'Choose a future access expiry.'); await ports.prepare(actor); await ports.fresh(actor); const id = 'invite-' + (await ports.digest(actor.companyId + ':' + actor.userId + ':' + key)).slice(0, 40); await ports.reserve(actor, key, 'invitation.reserve', { personId, ...input }, id); const token = await ports.token(id); await ports.store(actor, personId, person, input, id, await ports.digest(token), expiry); const review = await ports.propose(actor, id, requested(person, input, 'usr_invite_' + id, 'Invite employee with explicitly reviewed access')); return { id, token, reviewId: review.id, deliveryState: 'awaiting_approval', message: 'Invitation prepared. Independent approval is required before email delivery.', expiresAt: ports.now() + 48 * 3600000 }; },
        retryInvitationDelivery: async (actor: WorkContext, personId: string, invitationId: string, key: string) => { actor = ports.owner(actor); await ports.person(actor, personId); await ports.fresh(actor); await ports.prepare(actor); await ports.reserve(actor, key, 'invitation.delivery.retry', { personId, invitationId }, invitationId); if (!await ports.retry(actor, personId, invitationId))
            throw ports.error(409, 'INVITATION_DELIVERY_NOT_RETRYABLE', 'Only a failed, current invitation can be retried.'); return { id: invitationId, status: 'queued' }; },
        revokeInvitation: async (actor: WorkContext, personId: string, invitationId: string, key: string) => { actor = ports.owner(actor); await ports.person(actor, personId); await ports.fresh(actor); await ports.prepare(actor); await ports.reserve(actor, key, 'invitation.revoke', { personId, invitationId }, invitationId); if (!await ports.revokeInvitation(actor, personId, invitationId) && await ports.invitationStatus(actor, personId, invitationId) !== 'revoked')
            throw ports.error(409, 'INVITATION_CHANGED', 'Only a pending invitation can be revoked.'); await ports.cancelDelivery(invitationId); return { id: invitationId, status: 'revoked' }; },
        inspectInvitation: async (token: string) => { const row = await invitation(token); return { name: row.name, email: row.email, existingAccount: Boolean(JSON.parse(row.role).existingUserId) }; },
    };
}
