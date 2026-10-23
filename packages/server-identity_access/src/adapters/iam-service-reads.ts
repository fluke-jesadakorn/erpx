import 'server-only';
import {createIamServiceReads} from '@erpx/server-identity_access/application/iam-service-reads';
import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
import {identitySnapshotSources as peoplepayrollSnapshotSources} from '@erpx/server-people_payroll/public/identity-snapshot-sources';
import {iamDb} from '@erpx/server-identity_access/public/iam-db';
import {ApiError} from '@erpx/server-runtime/public/http';
import {runtimeEnv} from '@erpx/server-runtime/public/runtime';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import type {RoleRevision} from '@erpx/feature-identity_access/lib/iam';
import {type IamActor,iamPermission} from '@erpx/server-identity_access/domain/iam-actor';
import {createAssertCompanyScope} from '@erpx/server-identity_access/application/iam-context';
const assertCompanyScope=createAssertCompanyScope({error:(...args)=>new ApiError(...args)});
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {iamRole} from '@erpx/server-identity_access/public/iam-store';
const organizationSources=organizationSnapshotSources(),peoplepayrollSources=peoplepayrollSnapshotSources();
export async function iamAuditHistory(db: PostgresDatabase, actor: IamActor, search: string, cursor?: string) { return readService(db).iamAuditHistory(actor, search, cursor); }
export async function listIamRoles(db: PostgresDatabase, companyId: string): Promise<RoleRevision[]> { return readService(db).listIamRoles(companyId); }
export async function iamOverview(db: PostgresDatabase, actor: IamActor, self = false) { return readService(db).iamOverview(actor, self); }
function readService(database: PostgresDatabase) {
    const db = iamDb(database);
    let environment: Awaited<ReturnType<typeof runtimeEnv>>;
    const control = async () => environment ??= await runtimeEnv();
    return createIamServiceReads({ error: (...args) => new ApiError(...args), now: Date.now, companyScope: assertCompanyScope, authorize: requirePermission, readyControl: async () => { await control(); }, roleIds: async (companyId) => (await db
            .prepare("SELECT id FROM iam_roles WHERE company_id IN ('*',?) AND status='active' AND published_revision>0 ORDER BY built_in DESC,name")
            .bind(companyId)
            .all<{
            id: string;
        }>()).results, role: async (companyId, id) => iamRole(db, companyId, id), auditPage: async (actor, needle, after) => (await db
            .prepare("SELECT * FROM iam_audit WHERE company_id=? AND (action ILIKE ? OR actor_id ILIKE ? OR target_id ILIKE ?) AND (?::bigint IS NULL OR created_at<? OR (created_at=? AND id<?)) ORDER BY created_at DESC,id DESC LIMIT 101")
            .bind(actor.companyId, needle, needle, needle, after?.createdAt ?? null, after?.createdAt ?? null, after?.createdAt ?? null, after?.id ?? null)
            .all<Record<string, unknown>>()).results, assignments: async (actor, self) => {
            const table = actor.platform ? "platform_assignments" : "assignments";
            return (await db
                .prepare(`SELECT a.*,${actor.platform ? "'platform' AS company_id,'platform' AS scope_kind,NULL AS scope_node_id,NULL AS branch_id," : ""} m.status identity_status,m.login_policy,m.person_id,m.connection_id,m.version identity_version,mp.group_id,mapping_group.name group_name,mp.connection_id mapping_connection_id,i.enabled directory_enabled,i.observed_at,c.status connection_status FROM ${table} a LEFT JOIN iam_members m ON m.user_id=a.user_id AND m.company_id=? LEFT JOIN iam_identities i ON i.id=m.identity_id LEFT JOIN iam_connections c ON c.id=m.connection_id LEFT JOIN iam_mappings mp ON a.source='directory' AND mp.id=a.source_id AND mp.company_id=${actor.platform ? "'platform'" : "a.company_id"} LEFT JOIN iam_groups mapping_group ON mapping_group.connection_id=mp.connection_id AND mapping_group.external_id=mp.group_id WHERE ${actor.platform ? "true" : "a.company_id=?"} AND (?::text IS NULL OR a.user_id=?) ${!actor.platform && actor.scopeKind !== "holding" ? `AND (a.scope_node_id=? OR a.scope_node_id IN (SELECT id ${organizationSources.iam_service_organization_nodes_from} WHERE branch_id=? AND company_id=?))` : ""} ORDER BY a.user_id,a.role`)
                .bind(actor.companyId, ...(actor.platform ? [] : [actor.companyId]), self ? actor.userId : null, self ? actor.userId : null, ...(!actor.platform && actor.scopeKind !== "holding"
                ? [actor.scopeNodeId, actor.scopeNodeId, actor.companyId]
                : []))
                .all<Record<string, unknown>>()).results;
        },
        people: async (actor) => {
            return (await db
                .prepare(`SELECT id,user_id,legal_name ${peoplepayrollSources.iam_service_hr_people_from} WHERE company_id=? AND status='active' AND (? OR EXISTS(SELECT 1 ${peoplepayrollSources.iam_service_hr_engagements_e_from} WHERE e.person_id=hr_people.id AND e.status='active' AND (e.branch_id=? OR e.department_id=?)))`)
                .bind(actor.companyId, actor.scopeKind === "holding", actor.scopeNodeId, actor.scopeNodeId)
                .all<{
                id: string;
                user_id: string | null;
                legal_name: string;
            }>()).results;
        },
        nodes: async (actor) => {
            return (await db
                .prepare(`SELECT id,display_name,kind,branch_id ${organizationSources.iam_service_organization_nodes_from} WHERE company_id=? AND status='active' AND (? OR id=? OR branch_id=?) ORDER BY kind,display_name`)
                .bind(actor.companyId, actor.scopeKind === "holding", actor.scopeNodeId, actor.scopeNodeId)
                .all()).results;
        },
        migrations: async (actor) => {
            return (await db
                .prepare("SELECT * FROM iam_migrations WHERE company_id=? ORDER BY created_at DESC LIMIT 100")
                .bind(actor.companyId)
                .all()).results;
        },
        users: async (ids) => {
            const env = await control();
            return (await env.CONTROL_DB.prepare(`SELECT id,name,email FROM "user" WHERE id IN (${ids.map(() => "?").join(",")})`)
                .bind(...ids)
                .all<{
                id: string;
                name: string;
                email: string;
            }>()).results;
        },
        mfaStatus: async (actor) => {
            const env = await control();
            return await env.CONTROL_DB.prepare("SELECT enabled FROM iam_mfa WHERE user_id=?")
                .bind(actor.userId)
                .first<{
                enabled: boolean;
            }>();
        },
        campaigns: async (actor) => { return (await db.prepare("SELECT * FROM iam_access_review_campaigns WHERE company_id=? ORDER BY created_at DESC LIMIT 50").bind(actor.companyId).all<Record<string, unknown>>()).results; },
        campaignItems: async (actor, accessReviewCampaigns) => { return (await db.prepare(`SELECT i.* FROM iam_access_review_items i JOIN iam_access_review_campaigns c ON c.id=i.campaign_id WHERE c.company_id=? AND c.id IN (${accessReviewCampaigns.map(() => "?").join(",")}) ORDER BY c.created_at DESC,i.user_id,i.role`).bind(actor.companyId, ...accessReviewCampaigns.map(c => String(c.id))).all<Record<string, unknown>>()).results; },
        sessions: async (actor) => {
            const env = await control();
            return (await env.CONTROL_DB.prepare('SELECT id,"createdAt","expiresAt","userAgent" FROM session WHERE "userId"=? ORDER BY "createdAt" DESC')
                .bind(actor.userId)
                .all()).results;
        },
        connections: async (actor, canManageDirectory) => {
            return (await db
                .prepare(`SELECT ${canManageDirectory ? "*" : "id,name,provider,status,health,last_sync_at,last_error,version"} FROM iam_connections WHERE company_id=? ORDER BY name`)
                .bind(actor.companyId)
                .all<Record<string, unknown>>()).results;
        },
        groups: async (actor) => {
            return (await db
                .prepare("SELECT g.* FROM iam_groups g JOIN iam_connections c ON c.id=g.connection_id WHERE c.company_id=? ORDER BY g.name")
                .bind(actor.companyId)
                .all()).results;
        },
        previews: async (actor) => {
            return (await db
                .prepare("SELECT p.* FROM iam_connection_previews p JOIN iam_connections c ON c.id=p.connection_id AND c.version=p.version WHERE c.company_id=?")
                .bind(actor.companyId)
                .all<Record<string, unknown>>()).results;
        },
        mappings: async (actor) => {
            return (await db
                .prepare(`SELECT * FROM iam_mappings WHERE company_id=? ${["holding", "platform"].includes(actor.scopeKind) ? "" : `AND (scope_node_id=? OR scope_node_id IN (SELECT id ${organizationSources.iam_service_organization_nodes_from} WHERE branch_id=? AND company_id=?))`} ORDER BY id`)
                .bind(actor.companyId, ...(["holding", "platform"].includes(actor.scopeKind)
                ? []
                : [actor.scopeNodeId, actor.scopeNodeId, actor.companyId]))
                .all()).results;
        },
        identities: async (actor) => {
            return (await db
                .prepare("SELECT * FROM iam_identities WHERE company_id=? ORDER BY name")
                .bind(actor.companyId)
                .all()).results;
        },
        reviews: async (actor) => {
            return (await db
                .prepare(`SELECT * FROM iam_reviews WHERE company_id=? AND (? OR created_by=? OR (? AND kind IN ('assignment.grant','invitation.authorize','mapping.activate','policy.grant') AND (payload->>'scopeNodeId'=? OR ${actor.platform ? "false AND ?::text IS NULL AND ?::text IS NULL" : `payload->>'scopeNodeId' IN (SELECT id ${organizationSources.iam_service_organization_nodes_from} WHERE company_id=? AND branch_id=?)`}))) ORDER BY created_at DESC LIMIT 100`)
                .bind(actor.companyId, hasPermission(actor, iamPermission(actor, "iam.review")) &&
                ["holding", "platform"].includes(actor.scopeKind), actor.userId, hasPermission(actor, iamPermission(actor, "iam.review")), actor.scopeNodeId, actor.companyId, actor.scopeNodeId)
                .all()).results;
        },
        recentAudit: async (actor) => {
            return (await db
                .prepare("SELECT * FROM iam_audit WHERE company_id=? ORDER BY created_at DESC,id DESC LIMIT 100")
                .bind(actor.companyId)
                .all()).results;
        },
        active: async (r, now) => {
            return await db
                .prepare("SELECT iam_assignment_active(?,?) active")
                .bind(r.id, now)
                .first<{
                active: boolean;
            }>();
        } });
}
