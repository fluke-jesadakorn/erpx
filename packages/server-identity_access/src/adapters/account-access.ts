import { createHmac } from "node:crypto";
import { iamCommand } from "@erpx/server-identity_access/public/iam-transactions";
import { requireFreshMfa } from "@erpx/server-identity_access/public/iam-mfa";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { iamActorFromWork } from "@erpx/server-identity_access/public/iam-context";
import { grantIamAccess, grantInput, revokeIamAssignment, proposeIamReview } from "@erpx/server-identity_access/public/iam-service";
import {validateIamGrant} from '@erpx/server-identity_access/public/iam-grants';
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { runtimeEnv, holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { directoryPerson } from "@erpx/server-people_payroll/public/people-directory";
import { ApiError } from "@erpx/server-runtime/public/http";
import { createAccountAccessService } from '@erpx/server-identity_access/application/account-access';
import type { PersonGrant, Invitation } from '@erpx/server-identity_access/domain/account-access';
export { grantSchema, acceptInvitationSchema } from '@erpx/server-identity_access/domain/account-access';
async function hash(value: string) {
    const b = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
    return Array.from(new Uint8Array(b), (v) => v.toString(16).padStart(2, "0")).join("");
}
function service() {
    let write: Awaited<ReturnType<typeof holdingResources>>['write'], env: Awaited<ReturnType<typeof runtimeEnv>>, iam: ReturnType<typeof iamActorFromWork>;
    const environment = async () => env ??= await runtimeEnv();
    const workspace = async (actor: WorkContext) => { iam = iamActorFromWork(actor); write = (await holdingResources(actor.holdingId)).write; };
    return createAccountAccessService({ error: (...args) => new ApiError(...args), now: Date.now, owner: (actor) => requirePermission(actor, 'iam.manage'), person: directoryPerson, digest: hash, token: async (id) => createHmac('sha256', (await environment()).BETTER_AUTH_SECRET).update('invitation:' + id).digest('hex'), emailExists: async (email) => {
            const env = await environment();
            return !!await env.CONTROL_DB.prepare('SELECT 1 FROM "user" WHERE lower(email)=lower(?)')
                .bind(email)
                .first();
        }, prepare: workspace, fresh: async (actor) => requireFreshMfa(iamActorFromWork(actor)),
        grant: async (_actor, input, key) => grantIamAccess(write as PostgresDatabase, iam, grantInput.parse(input), key), target: async (actor, userId, assignmentId) => write.prepare('SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=?').bind(assignmentId, userId, actor.companyId).first<{
            version: number;
        }>(), revoke: async (_actor, id, version, key) => revokeIamAssignment(write as PostgresDatabase, iam, id, version, 'Revoked from employee profile', key), reserve: async (_actor, key, operation, payload, id) => { await iamCommand(write as PostgresDatabase, iam, key, operation, payload, 'iam.manage', async () => ({ id })); },
        store: async (actor, personId, person, input, id, tokenHash, expiry) => {
            const env = await environment();
            await env.CONTROL_DB.batch([
                env.CONTROL_DB.prepare("UPDATE account_invitations SET status='revoked' WHERE holding_id=? AND person_id=? AND status='pending' AND id!=?").bind(actor.holdingId, personId, id),
                env.CONTROL_DB.prepare("UPDATE account_invitation_delivery_jobs d SET state='cancelled',error='INVITATION_SUPERSEDED',lease_token=NULL,lease_until=NULL,updated_at=? FROM account_invitations i WHERE d.invitation_id=i.id AND i.holding_id=? AND i.person_id=? AND i.status='revoked' AND i.id!=? AND d.state!='sent'").bind(Date.now(), actor.holdingId, personId, id),
                env.CONTROL_DB.prepare("INSERT INTO account_invitations(id,token_hash,holding_id,company_id,person_id,email,name,role,scope_kind,branch_id,created_by,assignment_id,expires_at,created_at,department_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING").bind(id, tokenHash, actor.holdingId, actor.companyId, personId, person.email, person.legal_name, JSON.stringify({ role: input.role, expires: expiry }), input.scope, input.scope !== "holding" ? person.branch_id : null, actor.userId, actor.assignmentId, Date.now() + 48 * 3600000, Date.now(), input.scope === "department" ? person.department_id : null),
                env.CONTROL_DB.prepare("INSERT INTO account_invitation_delivery_jobs(id,invitation_id,holding_id,company_id,state,next_attempt_at,created_at,updated_at) VALUES(?,?,?,?,'awaiting_approval',?,?,?) ON CONFLICT(invitation_id) DO NOTHING").bind(`account-invite-delivery-${id}`, id, actor.holdingId, actor.companyId, Date.now(), Date.now(), Date.now()),
            ]);
        },
        propose: async (_actor, id, input) => { const requested = grantInput.parse(input); return iamCommand(write as PostgresDatabase, iam, `invite-review-${id}`, 'invitation.propose', { invitationId: id, ...requested }, 'iam.manage', async (c) => { await validateIamGrant(c, iam, requested); return proposeIamReview(c, iam, 'invitation.authorize', { invitationId: id, ...requested, parentId: iam.assignmentId }, 'Authorize an employee invitation and initial grant.'); }); },
        retry: async (actor, personId, invitationId) => { const env = await environment(); return !!await env.CONTROL_DB.prepare("UPDATE account_invitation_delivery_jobs SET state='queued',attempts=0,next_attempt_at=?,error=NULL,lease_token=NULL,lease_until=NULL,updated_at=? WHERE invitation_id=? AND holding_id=? AND company_id=? AND state='failed' AND EXISTS(SELECT 1 FROM account_invitations WHERE id=? AND person_id=? AND status='pending' AND expires_at>?) RETURNING id").bind(Date.now(), Date.now(), invitationId, actor.holdingId, actor.companyId, invitationId, personId, Date.now()).first(); }, revokeInvitation: async (actor, personId, invitationId) => { const env = await environment(); return !!await env.CONTROL_DB.prepare("UPDATE account_invitations SET status='revoked' WHERE id=? AND holding_id=? AND company_id=? AND person_id=? AND status='pending' RETURNING id").bind(invitationId, actor.holdingId, actor.companyId, personId).first(); }, invitationStatus: async (actor, personId, invitationId) => {
            const env = await environment();
            return (await env.CONTROL_DB.prepare("SELECT status FROM account_invitations WHERE id=? AND holding_id=? AND company_id=? AND person_id=?").bind(invitationId, actor.holdingId, actor.companyId, personId).first<{
                status: string;
            }>())?.status ?? null;
        }, cancelDelivery: async (invitationId) => { const env = await environment(); await env.CONTROL_DB.prepare("UPDATE account_invitation_delivery_jobs SET state='cancelled',error='INVITATION_REVOKED',lease_token=NULL,lease_until=NULL,updated_at=? WHERE invitation_id=? AND state!='sent'").bind(Date.now(), invitationId).run(); },
        invitation: async (tokenHash, now) => {
            const env = await environment();
            return await env.CONTROL_DB.prepare("SELECT * FROM account_invitations WHERE token_hash=? AND (status='pending' OR (status='accepted' AND accepted_at IS NULL)) AND expires_at>?")
                .bind(tokenHash, now)
                .first<Invitation>();
        } });
}
export const grantPersonAccess = (actor: WorkContext, personId: string, input: PersonGrant, key: string) => service().grantPersonAccess(actor, personId, input, key);
export const revokePersonAccess = (actor: WorkContext, personId: string, assignmentId: string, key: string) => service().revokePersonAccess(actor, personId, assignmentId, key);
export const invitePerson = (actor: WorkContext, personId: string, input: PersonGrant, key: string) => service().invitePerson(actor, personId, input, key);
export const retryInvitationDelivery = (actor: WorkContext, personId: string, invitationId: string, key: string) => service().retryInvitationDelivery(actor, personId, invitationId, key);
export const revokeInvitation = (actor: WorkContext, personId: string, invitationId: string, key: string) => service().revokeInvitation(actor, personId, invitationId, key);
export const inspectInvitation = (token: string) => service().inspectInvitation(token);
