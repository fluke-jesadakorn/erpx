import { grantTransaction } from '@erpx/server-identity_access/adapters/iam-grant-transaction';
import {preparePeopleLifecycleIamReview} from "@erpx/server-people_payroll/public/iam-purchases";
import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import { createIamServiceCommands, type IamServiceTransaction } from '@erpx/server-identity_access/application/iam-service-commands';
const extensionClients = new WeakMap<IamServiceTransaction, PoolClient>();
import { iamDb } from "@erpx/server-identity_access/public/iam-db";
import "server-only";
import { z } from "zod";
import type { PoolClient } from "pg";
import { iamCommand } from "@erpx/server-identity_access/public/iam-transactions";
import { type IamActor, iamPermission, assertCompanyScope } from "@erpx/server-identity_access/public/iam-context";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { requireFreshMfa } from "@erpx/server-identity_access/public/iam-mfa";
import { ApiError } from "@erpx/server-runtime/public/http";
import { runtimeEnv } from "@erpx/server-runtime/public/runtime";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
export { roleInput, grantInput, reviewInput, accessReviewCampaignInput, accessReviewDecisionInput } from '@erpx/server-identity_access/domain/iam-service';
import { roleInput, grantInput, reviewInput, accessReviewCampaignInput, accessReviewDecisionInput } from '@erpx/server-identity_access/domain/iam-service';

export async function startAccessReviewCampaign(db: PostgresDatabase, actor: IamActor, input: z.infer<typeof accessReviewCampaignInput>, key: string) { return serviceCommands(db).startAccessReviewCampaign(actor, input, key); }
export async function decideAccessReviewItem(db: PostgresDatabase, actor: IamActor, itemId: string, input: z.infer<typeof accessReviewDecisionInput>, key: string) { return serviceCommands(db).decideAccessReviewItem(actor, itemId, input, key); }
export async function cancelAccessReviewCampaign(db: PostgresDatabase, actor: IamActor, id: string, expectedVersion: number, reason: string, key: string) { return serviceCommands(db).cancelAccessReviewCampaign(actor, id, expectedVersion, reason, key); }
async function review(client: PoolClient, actor: IamActor, kind: string, payload: unknown, reason: string) { return serviceCommands().review(serviceTransaction(client, actor), actor, kind, payload, reason); }
export async function saveIamRole(db: PostgresDatabase, actor: IamActor, input: z.infer<typeof roleInput>, key: string) { return serviceCommands(db).saveIamRole(actor, input, key); }

export async function grantIamAccess(db: PostgresDatabase, actor: IamActor, input: z.infer<typeof grantInput>, key: string) { return serviceCommands(db).grantIamAccess(actor, input, key); }
export async function revokeIamAssignment(db: PostgresDatabase, actor: IamActor, id: string, version: number, reason: string, key: string) { return serviceCommands(db).revokeIamAssignment(actor, id, version, reason, key); }
export async function applyIamRevocation(c: PoolClient, actor: IamActor, id: string, version: number, reason: string, approved = false) { return serviceCommands().applyIamRevocation(serviceTransaction(c, actor), actor, id, version, reason, approved); }
export async function decideIamReview(db: PostgresDatabase, actor: IamActor, id: string, input: z.infer<typeof reviewInput>, key: string, applyExtension?: (c: PoolClient, kind: string, payload: Record<string, unknown>) => Promise<void>, rejectExtension?: (c: PoolClient, kind: string, payload: Record<string, unknown>) => Promise<void>) { return serviceCommands(db,c=>preparePeopleLifecycleIamReview(c,actor,id,input.decision,key)).decideIamReview(actor, id, input, key, applyExtension ? async (tx, kind, payload) => applyExtension(extensionClients.get(tx)!, kind, payload) : undefined, rejectExtension ? async (tx, kind, payload) => rejectExtension(extensionClients.get(tx)!, kind, payload) : undefined); }
export { review as proposeIamReview };
function serviceTransaction(c: PoolClient, actor: IamActor): IamServiceTransaction {
    return { ...grantTransaction(c, actor),
activeReviewAssignments: async (facts) => { const { observedNow } = facts; return (await c.query("SELECT id,user_id,role,scope_kind,scope_node_id,source,valid_until,version FROM assignments WHERE company_id=$1 AND revoked_at IS NULL AND iam_assignment_active(id,$2) AND iam_principal_active(user_id,$1,$2) ORDER BY user_id,role,id", [actor.companyId, observedNow])).rows; },
insertReviewCampaign: async (facts) => { const { id, input, now } = facts; return void await c.query("INSERT INTO iam_access_review_campaigns(id,company_id,name,due_at,status,version,created_by,reason,created_at,updated_at) VALUES($1,$2,$3,$4,'open',1,$5,$6,$7,$7)", [id, actor.companyId, input.name, input.dueAt, actor.userId, input.reason, now]); },
insertReviewItem: async (facts) => { const { key, index, id, a } = facts; return void await c.query("INSERT INTO iam_access_review_items(id,campaign_id,company_id,assignment_id,assignment_version,user_id,role,scope_kind,scope_node_id,source,valid_until) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)", [`access-review-item-${key}-${index}`, id, actor.companyId, a.id, a.version, a.user_id, a.role, a.scope_kind, a.scope_node_id, a.source, a.valid_until]); },
insertCampaignStartedEvent: async (facts) => { const { key, id, input, assignments, now } = facts; return void await c.query("INSERT INTO iam_access_review_events(id,campaign_id,company_id,action,actor_id,snapshot_json,created_at) VALUES($1,$2,$3,'started',$4,$5,$6)", [`access-review-event-${key}`, id, actor.companyId, actor.userId, JSON.stringify({ name: input.name, dueAt: input.dueAt, reason: input.reason, assignmentIds: assignments.map(a => a.id) }), now]); },
lockReviewItem: async (facts) => { const { itemId } = facts; return (await c.query("SELECT i.*,c.status campaign_status,c.version campaign_version,c.created_by FROM iam_access_review_items i JOIN iam_access_review_campaigns c ON c.id=i.campaign_id WHERE i.id=$1 AND i.company_id=$2 FOR UPDATE OF i,c", [itemId, actor.companyId])).rows[0] ?? null; },
reviewerSamePerson: async (facts) => { const { item } = facts; return (await c.query("SELECT iam_same_person($1,$2,$3) same", [item.user_id, actor.userId, actor.companyId])).rows[0] ?? null; },
lockReviewAssignment: async (facts) => { const { item } = facts; return (await c.query("SELECT * FROM assignments WHERE id=$1 AND company_id=$2 FOR UPDATE", [item.assignment_id, actor.companyId])).rows[0] ?? null; },
reviewAssignmentActive: async (facts) => { const { item, observedNow } = facts; return (await c.query("SELECT iam_assignment_active($1,$2) active", [item.assignment_id, observedNow])).rows[0] ?? null; },
reviewRecoveryMember: async (facts) => { const { item } = facts; return (await c.query("SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 AND login_policy='emergency' AND status='active'", [actor.companyId, item.user_id])).rowCount; },
recordItemDecision: async (facts) => { const { input, now, itemId } = facts; return void await c.query("UPDATE iam_access_review_items SET decision=$1,decided_by=$2,decision_reason=$3,decided_at=$4 WHERE id=$5", [input.decision, actor.userId, input.reason, now, itemId]); },
insertDecisionEvent: async (facts) => { const { key, item, itemId, input, now } = facts; return void await c.query("INSERT INTO iam_access_review_events(id,campaign_id,item_id,company_id,action,actor_id,snapshot_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)", [`access-review-event-${key}`, item.campaign_id, itemId, actor.companyId, input.decision === "keep" ? "kept" : "revoked", actor.userId, JSON.stringify({ assignmentId: item.assignment_id, assignmentVersion: item.assignment_version, userId: item.user_id, role: item.role, scopeKind: item.scope_kind, scopeNodeId: item.scope_node_id, source: item.source, validUntil: item.valid_until, reason: input.reason }), now]); },
pendingReviewItems: async (facts) => { const { item } = facts; return (await c.query("SELECT count(*) n FROM iam_access_review_items WHERE campaign_id=$1 AND decision='pending'", [item.campaign_id])).rows[0] ?? null; },
completeReviewCampaign: async (facts) => { const { remaining, campaignVersion, now, item } = facts; return void await c.query("UPDATE iam_access_review_campaigns SET status=$1,version=$2,completed_at=$3,updated_at=$4 WHERE id=$5", [remaining ? "open" : "completed", campaignVersion, remaining ? null : now, now, item.campaign_id]); },
insertCampaignCompletedEvent: async (facts) => { const { key, item, now } = facts; return void await c.query("INSERT INTO iam_access_review_events(id,campaign_id,company_id,action,actor_id,snapshot_json,created_at) VALUES($1,$2,$3,'completed',$4,$5,$6)", [`${`access-review-event-${key}`}-complete`, item.campaign_id, actor.companyId, actor.userId, JSON.stringify({ completedItems: true }), now]); },
lockReviewCampaign: async (facts) => { const { id } = facts; return (await c.query("SELECT * FROM iam_access_review_campaigns WHERE id=$1 AND company_id=$2 FOR UPDATE", [id, actor.companyId])).rows[0] ?? null; },
decidedReviewItems: async (facts) => { const { id } = facts; return (await c.query("SELECT count(*) n FROM iam_access_review_items WHERE campaign_id=$1 AND decision!='pending'", [id])).rows[0] ?? null; },
cancelReviewCampaign: async (facts) => { const { version, now, id } = facts; return void await c.query("UPDATE iam_access_review_campaigns SET status='cancelled',version=$1,updated_at=$2 WHERE id=$3", [version, now, id]); },
insertCampaignCancelledEvent: async (facts) => { const { key, id, reason, now } = facts; return void await c.query("INSERT INTO iam_access_review_events(id,campaign_id,company_id,action,actor_id,snapshot_json,created_at) VALUES($1,$2,$3,'cancelled',$4,$5,$6)", [`access-review-event-${key}`, id, actor.companyId, actor.userId, JSON.stringify({ reason }), now]); },
insertIndependentReview: async (facts) => {
            const { id, kind, payload, observedNow, reason } = facts;
            return void await c.query("INSERT INTO iam_reviews(id,company_id,kind,payload,created_by,created_person,created_at,reason) VALUES($1,$2,$3,$4,$5,$6,$7,$8)", [
                id,
                actor.companyId,
                kind,
                JSON.stringify({
                    ...(payload as Record<string, unknown>),
                    authorizingAssignmentId: actor.assignmentId,
                }),
                actor.userId,
                actor.iam?.personId ?? actor.userId,
                observedNow,
                reason,
            ]);
        },
lockProposedRole: async (facts) => {
            const { id } = facts;
            return (await c.query("SELECT * FROM iam_roles WHERE id=$1 AND company_id IN ($2,'*') FOR UPDATE", [id, actor.companyId])).rows[0] ?? null;
        },
createProposedRole: async (facts) => {
            const { id, input } = facts;
            return void await c.query("INSERT INTO iam_roles(company_id,id,name) VALUES($1,$2,$3)", [actor.companyId, id, input.name]);
        },
nextRoleRevision: async (facts) => {
            const { id } = facts;
            return (await c.query("SELECT COALESCE(MAX(revision),0)+1 next FROM iam_role_revisions WHERE company_id=$1 AND role_id=$2", [actor.companyId, id])).rows[0] ?? null;
        },
appendRoleRevision: async (facts) => {
            const { id, revision, input, observedNow } = facts;
            return void await c.query("INSERT INTO iam_role_revisions VALUES($1,$2,$3,$4,$5,$6,$7,$8)", [
                actor.companyId,
                id,
                revision,
                JSON.stringify([...new Set(input.permissions)]),
                JSON.stringify([...new Set(input.grantablePermissions)]),
                JSON.stringify(input.scopes),
                actor.userId,
                observedNow,
            ]);
        },
affectedRoleUsers: async (facts) => {
            const { id } = facts;
            return (await c.query(`SELECT DISTINCT user_id FROM ${actor.platform ? "platform_assignments" : "assignments"} WHERE role=$1 AND revoked_at IS NULL ${actor.platform ? "" : "AND company_id=$2"}`, actor.platform ? [id] : [id, actor.companyId])).rows;
        },
lockRevokedAssignment: async (facts) => {
            const { id } = facts;
            return (await c.query(`SELECT * FROM ${actor.platform ? "platform_assignments" : "assignments"} WHERE id=$1 ${actor.platform ? "" : "AND company_id=$2"} FOR UPDATE`, actor.platform ? [id] : [id, actor.companyId])).rows[0] ?? null;
        },
revokeSamePerson: async (facts) => {
            const { row } = facts;
            return (await c.query("SELECT iam_same_person($1,$2,$3) same", [
                row.user_id,
                actor.userId,
                actor.companyId,
            ])).rows[0] ?? null;
        },
revokeRecoveryMember: async (facts) => {
            const { row } = facts;
            return (await c.query("SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 AND login_policy='emergency' AND status='active'", [actor.companyId, row.user_id])).rowCount;
        },
otherRecoveryAdministrators: async (facts) => {
            const { row, observedNow } = facts;
            return (await c.query(`SELECT 1 FROM iam_members m JOIN ${actor.platform ? "platform_assignments" : "assignments"} a ON a.user_id=m.user_id WHERE m.company_id=$1 AND m.user_id!=$2 AND m.login_policy='emergency' AND m.status='active' AND a.role=$3 AND iam_assignment_active(a.id,$4) ${actor.platform ? "" : "AND a.company_id=m.company_id"}`, [
                actor.companyId,
                row.user_id,
                actor.platform ? "platform_admin" : "holding_owner",
                observedNow,
            ])).rowCount;
        },
otherAdministrators: async (facts) => {
            const { row, observedNow } = facts;
            return (await c.query(`SELECT id FROM ${actor.platform ? "platform_assignments" : "assignments"} WHERE role=$1 AND user_id!=$2 AND iam_assignment_active(id,$3) ${actor.platform ? "" : "AND company_id=$4"} FOR UPDATE`, actor.platform
                ? [row.role, row.user_id, observedNow]
                : [row.role, row.user_id, observedNow, actor.companyId])).rows;
        },
revokeAssignment: async (facts) => {
            const { observedNow, id } = facts;
            return void await c.query(`UPDATE ${actor.platform ? "platform_assignments" : "assignments"} SET revoked_at=$1,version=version+1 WHERE id=$2`, [observedNow, id]);
        },
clearRevokedWorkSelection: async (facts) => {
            const { id } = facts;
            return void await c.query("UPDATE active_work_contexts SET revision=revision+1,assignment_id=NULL,assignment_version=NULL WHERE company_id=$1 AND assignment_id=$2", [actor.companyId, id]);
        },
lockIndependentReview: async (facts) => {
            const { id } = facts;
            return (await c.query("SELECT * FROM iam_reviews WHERE id=$1 AND company_id=$2 FOR UPDATE", [id, actor.companyId])).rows[0] ?? null;
        },
independentSamePerson: async (facts) => {
            const { r } = facts;
            return (await c.query("SELECT iam_same_person($1,$2,$3) same", [
                r.created_by,
                actor.userId,
                actor.companyId,
            ])).rows[0] ?? null;
        },
branchCoversReview: async (facts) => {
            const { p } = facts;
            return (await c.query(`SELECT 1 ${organizationSources.iam_service_organization_nodes_from} WHERE id=$1 AND company_id=$2 AND branch_id=$3`, [p.scopeNodeId, actor.companyId, actor.scopeNodeId])).rowCount;
        },
proposerCurrentAuthority: async (facts) => {
            const { p, r, observedNow } = facts;
            return (await c.query(`SELECT a.role,v.grantable_json FROM ${actor.platform ? "platform_assignments" : "assignments"} a JOIN iam_roles r ON r.id=a.role AND r.company_id IN ('*',$2) AND r.status='active' JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE a.id=$1 AND a.user_id=$3 AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,$2,$4)`, [
                p.authorizingAssignmentId,
                actor.companyId,
                r.created_by,
                observedNow,
            ])).rows[0] ?? null;
        },
proposerHasPermission: async (facts) => {
            const { original, required } = facts;
            return (await c.query("SELECT iam_role_allows($1,$2,$3) allowed", [
                original.role,
                actor.companyId,
                iamPermission(actor, required),
            ])).rows[0] ?? null;
        },
reviewerAffectedByRole: async (facts) => {
            const { p, observedNow } = facts;
            return (await c.query(`SELECT 1 FROM ${actor.platform ? "platform_assignments" : "assignments"} WHERE role=$1 AND iam_assignment_active(id,$2) AND iam_same_person(user_id,$3,$4) ${actor.platform ? "" : "AND company_id=$4"}`, [p.id, observedNow, actor.userId, actor.companyId])).rowCount;
        },
lockRolePublication: async (facts) => {
            const { p } = facts;
            return (await c.query("SELECT * FROM iam_roles WHERE company_id=$1 AND id=$2 FOR UPDATE", [actor.companyId, p.id])).rows[0] ?? null;
        },
publishRoleRevision: async (facts) => {
            const { p } = facts;
            return void await c.query("UPDATE iam_roles SET published_revision=$1,name=$2 WHERE company_id=$3 AND id=$4", [p.revision, p.name, actor.companyId, p.id]);
        },
invalidateRoleAssignments: async (facts) => {
            const { p } = facts;
            return void await c.query(`UPDATE ${actor.platform ? "platform_assignments" : "assignments"} SET version=version+1 WHERE role=$1 ${actor.platform ? "" : "AND company_id=$2"}`, actor.platform ? [p.id] : [p.id, actor.companyId]);
        },
clearPublishedWorkSelections: async (facts) => {
            const { p } = facts;
            return void await c.query("UPDATE active_work_contexts SET assignment_id=NULL,assignment_version=NULL,revision=revision+1 WHERE company_id=$1 AND assignment_id IN (SELECT id FROM assignments WHERE role=$2 AND company_id=$1)", [actor.companyId, p.id]);
        },
recordIndependentDecision: async (facts) => {
            const { input, observedNow, id } = facts;
            return void await c.query("UPDATE iam_reviews SET status=$1,reviewed_by=$2,reviewed_at=$3,reason=reason||$4,version=version+1 WHERE id=$5", [
                input.decision === "approve" ? "approved" : "rejected",
                actor.userId,
                observedNow,
                `\nReview: ${input.reason}`,
                id,
            ]);
        }};
}
function serviceCommands(db?: PostgresDatabase,prepareOwner?:import("@erpx/server-identity_access/public/iam-transactions").IamOwnerPreparation) {
    return createIamServiceCommands({ error: (...args) => new ApiError(...args), now: Date.now, id: () => crypto.randomUUID(), companyScope: assertCompanyScope, authorize: requirePermission, fresh: requireFreshMfa, userExists: async (userId) => !!await (await runtimeEnv()).CONTROL_DB.prepare('SELECT id FROM "user" WHERE id=?').bind(userId).first(), command: async (actor, key, operation, payload, permission, run) => iamCommand(iamDb(db!), actor, key, operation, payload, permission, async (c) => {
            const tx = serviceTransaction(c, actor);
            extensionClients.set(tx, c);
            try {
                return await run(tx);
            }
            finally {
                extensionClients.delete(tx);
            }
        },prepareOwner) });
}
