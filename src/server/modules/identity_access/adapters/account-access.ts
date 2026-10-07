import {stageInvitationPersonLinks} from "../../people_payroll/public/identity-person-link-effects";
import {identitySnapshotSources as peoplepayrollSnapshotSources} from '../../people_payroll/public/identity-snapshot-sources';
const peoplepayrollSources=peoplepayrollSnapshotSources();
import { createHmac } from "node:crypto";
import { iamCommand } from "@/server/modules/identity_access/public/iam-transactions";
import { requireFreshMfa } from "@/server/modules/identity_access/public/iam-mfa";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { iamActorFromWork } from "@/server/modules/identity_access/public/iam-context";
import { grantIamAccess, grantInput, revokeIamAssignment, validateIamGrant, proposeIamReview } from "@/server/modules/identity_access/public/iam-service";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { organizationGuard } from "@/server/modules/organization/public/organization";
import { hashPassword, verifyPassword } from "better-auth/crypto";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { runtimeEnv, holdingResources } from "@/server/modules/runtime/public/runtime";
import { directoryPerson } from "@/server/modules/people_payroll/public/people-directory";
import { ApiError } from "@/server/modules/runtime/public/http";
import { assertion, executeCommand } from "@/server/modules/workflow/public/commands";
import { businessRoles } from "@/lib/business-roles";
import { createAccountAccessService } from '../application/account-access';
import type { PersonGrant, InvitationAcceptance, Invitation } from '../domain/account-access';
export { grantSchema, acceptInvitationSchema } from '../domain/account-access';
async function hash(value: string) {
    const b = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
    return Array.from(new Uint8Array(b), (v) => v.toString(16).padStart(2, "0")).join("");
}
function service() {
    let write: Awaited<ReturnType<typeof holdingResources>>['write'], env: Awaited<ReturnType<typeof runtimeEnv>>, iam: ReturnType<typeof iamActorFromWork>, guard: Awaited<ReturnType<typeof organizationGuard>>;
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
        }, reviewed: async (row) => { write = (await holdingResources(row.holding_id)).write; return await write.prepare("SELECT id FROM iam_reviews WHERE company_id=? AND kind='invitation.authorize' AND status='approved' AND payload->>'invitationId'=?").bind(row.company_id, row.id).first(); }, prepareActivation: async (row) => { guard = await organizationGuard(write, row.company_id, `activate-${row.id}`); }, eligible: async (row, userId, now) => await write
            .prepare(`SELECT a.role ${peoplepayrollSources.account_access_hr_people_p_from} JOIN assignments a ON a.id=? AND a.company_id=p.company_id WHERE p.id=? AND p.company_id=? AND p.status='active' AND EXISTS(SELECT 1 ${peoplepayrollSources.account_access_hr_engagements_e_from} WHERE e.person_id=p.id AND e.company_id=p.company_id AND e.status='active' AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)) AND (p.user_id IS NULL OR p.user_id=?) AND a.user_id=? AND iam_role_allows(a.role,a.company_id,'iam.manage') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND ? IN ('branch','department') AND a.scope_node_id=?) OR (a.scope_kind='department' AND ?='department' AND a.scope_node_id=?)) AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>?`)
            .bind(row.assignment_id, row.person_id, row.company_id, row.branch_id, row.branch_id, row.department_id, row.department_id, userId, row.created_by, row.scope_kind, row.branch_id, row.scope_kind, row.department_id, now, now)
            .first<{
            role: string;
        }>(), credential: async (userId) => {
            const env = await environment();
            return await env.CONTROL_DB.prepare("SELECT password FROM account WHERE userId=? AND providerId='credential'")
                .bind(userId)
                .first<{
                password: string;
            }>();
        }, verify: async (password, hash) => verifyPassword({ password, hash }), createAccount: async (row, userId, password, now) => {
            const env = await environment();
            await env.CONTROL_DB.batch([
                env.CONTROL_DB.prepare("INSERT INTO account_invitation_claims(invitation_id,eligible,claimed_at) VALUES(?,CASE WHEN EXISTS(SELECT 1 FROM account_invitations WHERE id=? AND status='pending' AND expires_at>?) THEN 1 ELSE 0 END,?)").bind(row.id, row.id, now, now),
                env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,0,?,?)').bind(userId, row.name, row.email, now, now),
                env.CONTROL_DB.prepare("INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,'credential','local:credential',?,?,?,?)").bind(`account-${row.id}`, userId, userId, await hashPassword(password), now, now),
                env.CONTROL_DB.prepare("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,?,'active',?)").bind(`principal-${row.id}`, userId, row.name, now),
                env.CONTROL_DB.prepare("UPDATE account_invitations SET status='accepted',accepted_user_id=? WHERE id=? AND status='pending'").bind(userId, row.id),
            ]);
        }, activate: async (row, reviewId, userId, now, grant, role) => {
            const actor = { userId: row.created_by, assignmentId: row.assignment_id, role, companyId: row.company_id, branchId: row.branch_id, departmentId: row.department_id };
            await executeCommand(write, actor, `activate-${row.id}`, "identity.account.activate", { invitationId: row.id }, row.person_id, 1, { userId }, [guard,
                assertion(write, `activate-${row.id}`, "reviewed-invitation", "EXISTS(SELECT 1 FROM iam_reviews review JOIN iam_roles r ON r.id=review.payload->>'roleId' AND r.company_id IN ('*',review.company_id) WHERE review.id=? AND review.company_id=? AND review.status='approved' AND r.published_revision=(review.payload->>'expectedRoleRevision')::integer)", [reviewId, row.company_id]),
                assertion(write, `activate-${row.id}`, "person", `EXISTS(SELECT 1 ${peoplepayrollSources.account_access_hr_people_from} WHERE id=? AND company_id=? AND status='active' AND (user_id IS NULL OR user_id=?))`, [row.person_id, row.company_id, userId]),
                ...stageInvitationPersonLinks(write,userId,row.person_id,now),
                write
                    .prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,scope_kind,scope_node_id,parent_id,source) VALUES(?,?,?,?,?,?,?,?,?,?,?,'manual')")
                    .bind(`assignment-${row.id}`, userId, grant.role, JSON.stringify(businessRoles[grant.role]), row.company_id, row.branch_id, now, grant.expires, row.scope_kind, row.scope_kind === "department" ? row.department_id : row.branch_id, row.assignment_id),
            ]);
        }, accepted: async (row, userId) => {
            const env = await environment();
            await env.CONTROL_DB.prepare("UPDATE account_invitations SET status='accepted',accepted_user_id=?,accepted_at=? WHERE id=? AND status='accepted'")
                .bind(userId, Date.now(), row.id)
                .run();
        } });
}
export const grantPersonAccess = (actor: WorkContext, personId: string, input: PersonGrant, key: string) => service().grantPersonAccess(actor, personId, input, key);
export const revokePersonAccess = (actor: WorkContext, personId: string, assignmentId: string, key: string) => service().revokePersonAccess(actor, personId, assignmentId, key);
export const invitePerson = (actor: WorkContext, personId: string, input: PersonGrant, key: string) => service().invitePerson(actor, personId, input, key);
export const retryInvitationDelivery = (actor: WorkContext, personId: string, invitationId: string, key: string) => service().retryInvitationDelivery(actor, personId, invitationId, key);
export const revokeInvitation = (actor: WorkContext, personId: string, invitationId: string, key: string) => service().revokeInvitation(actor, personId, invitationId, key);
export const inspectInvitation = (token: string) => service().inspectInvitation(token);
export const acceptInvitation = (input: InvitationAcceptance) => service().acceptInvitation(input);
