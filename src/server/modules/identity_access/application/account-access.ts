import type { WorkContext } from '../domain/authorization';
import { assertGrantScope, type Invitation, type PersonFact, type PersonGrant, type InvitationAcceptance } from '../domain/account-access';
import type { IdentityError } from './authorization';
import type { BusinessRole } from "@/lib/identity_access/business-roles";
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
    throttleActivation?(row: Invitation, userId: string): Promise<void>;
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
    reviewed(row: Invitation): Promise<{
        id: string;
    } | null>;
    prepareActivation(row: Invitation): Promise<void>;
    eligible(row: Invitation, userId: string, now: number): Promise<{
        role: string;
    } | null>;
    credential(userId: string): Promise<{
        password: string;
    } | null>;
    verify(password: string, hash: string): Promise<boolean>;
    createAccount(row: Invitation, userId: string, password: string, now: number): Promise<void>;
    activate(row: Invitation, reviewId: string, userId: string, now: number, grant: {
        role: BusinessRole;
        expires: number;
    }, role: string): Promise<void>;
    accepted(row: Invitation, userId: string): Promise<void>;
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
        acceptInvitation: async (input: InvitationAcceptance) => { const row = await invitation(input.token), reviewed = await ports.reviewed(row); if (!reviewed)
            throw ports.error(409, 'INVITATION_REVIEW_REQUIRED', 'An independent administrator must approve this invitation before activation.'); await ports.prepareActivation(row); const existingUserId = JSON.parse(row.role).direct === true ? JSON.parse(row.role).existingUserId as string | undefined : undefined; if (!existingUserId && input.password.length < 12) throw ports.error(422, 'PASSWORD_TOO_SHORT', 'Use at least 12 characters for a new account.'); const userId = existingUserId ?? 'usr_invite_' + row.id, now = ports.now(), grant = JSON.parse(row.role) as {
            role: BusinessRole;
            expires: number;
        }, eligible = await ports.eligible(row, userId, now); if (!eligible || grant.expires <= now)
            throw ports.error(409, 'INVITATION_NO_LONGER_ELIGIBLE', 'The employee or authorizing assignment is no longer active.'); await ports.throttleActivation?.(row,userId); const account = await ports.credential(userId); if (existingUserId && !account) throw ports.error(409, 'ACCOUNT_VERIFICATION_REQUIRED', 'Use the verified account sign-in method.'); if (account && !await ports.verify(input.password, account.password))
            throw ports.error(409, 'ACTIVATION_RETRY_PASSWORD', existingUserId ? 'Verify the existing account password.' : 'Use the password from your first activation attempt.'); if (!account)
            await ports.createAccount(row, userId, input.password, now); await ports.activate(row, reviewed.id, userId, now, grant, eligible.role); await ports.accepted(row, userId); return { activated: true }; }
    };
}
