import { accountAuthoritySnapshot } from './account-authority-snapshot';
import { memoizeInRequest } from "../../runtime/public/runtime/request-memo";
import {identitySnapshotSources as organizationSnapshotSources} from '../../organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import { measureReadPhase } from "@/server/modules/runtime/public/runtime/read-metrics";
import { accountDirectoryGate } from "@/server/modules/identity_access/public/iam-directory-refresh";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { loadIamAuthority, iamRoles } from "@/server/modules/identity_access/public/iam-store";
import { readCompanyPreference, readWorkspacePreference } from "@/server/modules/work_projection/public/workspace-preferences";
import { cache } from "react";
import { ApiError } from "@/server/modules/runtime/public/http";
import { initializeWorkSelection } from "@/server/modules/work_projection/public/work-role-store";
import type { RoleRevision } from "@/lib/iam";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { betaAccess } from "@/server/modules/identity_access/public/beta-access";
import { assertProductionFeatureRoute } from "@/server/modules/pms/public/hospitality-rollout";
import "server-only";
import { headers } from "next/headers";
import type { HoldingWorkspaceScope } from "@/lib/workspace";
export { assignmentApplies, isRoleValidAtScope } from "@/lib/workspace-access";
import { createAuth } from "@/server/modules/identity_access/public/auth";
import { runtimeEnv, holdingBindingBySlug, holdingResources, type HoldingId } from "@/server/modules/runtime/public/runtime";
export type { AssignmentScopeKind, ResolvedWorkspaceScope, WorkContext, AuthorizedWorkspace, PlatformContext, WorkAssignment, WorkspaceCompany, WorkspaceResolutionOptions, AccountAccessContext } from "../domain/authorization";
export { publicScope } from "../domain/authorization";
import type { WorkspaceGrantFact, WorkContext, AuthorizedWorkspace, PlatformContext, WorkAssignment, OrganizationNode, WorkspaceCompany, WorkspaceResolutionOptions, AccountAccessContext } from "../domain/authorization";
import { createValidateNodeLineage, createResolveWorkspaceScope, createAssertFrontCompany, createWorkspaceCompanies, createRequestedHoldingScope, createAccountAccess, createWorkContext, createAuthorizedWorkspaces, createIdentitySession, createPlatformContext, createCanEnterManageErpx, createManageErpxContext, createFirstAuthorizedWorkspace } from "../application/authorization";
const identityError = (status: number, code: string, message: string) => new ApiError(status, code, message);
const sessionMemoKey = Symbol.for("erpx.identity.session");
export const currentSession = cache(() => memoizeInRequest(sessionMemoKey, () => measureReadPhase('session', readCurrentSession)));
async function readCurrentSession() { return createIdentitySession({ headers, authenticate: async (origin, requestHeaders) => (await createAuth(origin)).api.getSession({ headers: requestHeaders }) })(); }
async function findNode(database: SqlDatabase, kind: OrganizationNode["kind"], slug: string, companyId?: string) {
    return database
        .prepare(`SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
       ${organizationSources.authorization_organization_nodes_from} WHERE kind=? AND slug=? AND status='active'
         ${companyId ? "AND company_id=?" : ""}`)
        .bind(kind, slug, ...(companyId ? [companyId] : []))
        .first<OrganizationNode>();
}
async function validateNodeLineage(database: SqlDatabase, node: OrganizationNode) {
    return createValidateNodeLineage({ parent: async (parentId) => await database
            .prepare(`SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
         ${organizationSources.authorization_organization_nodes_from} WHERE id=? AND status='active'`)
            .bind(parentId)
            .first<OrganizationNode>() })(node);
}
export async function workspaceCompaniesForAccount(write: SqlDatabase, userId: string, now = Date.now()): Promise<WorkspaceCompany[]> {
    return createWorkspaceCompanies({ now: Date.now, companies: async (userId, now) => (await write.prepare(`SELECT n.id,n.display_name ${organizationSources.authorization_organization_nodes_n_from}
     JOIN assignments a ON a.company_id=n.id
     WHERE n.kind='company' AND n.status='active' AND a.user_id=?
       AND a.scope_kind='holding' AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
       AND a.valid_from<=? AND a.valid_until>?
       AND EXISTS(SELECT 1 FROM iam_roles role_row JOIN iam_role_revisions role_revision
         ON role_revision.company_id=role_row.company_id AND role_revision.role_id=role_row.id AND role_revision.revision=role_row.published_revision
         WHERE role_row.id=a.role AND role_row.company_id IN ('*',a.company_id) AND role_row.status='active'
           AND role_revision.scopes_json @> '["holding"]'::jsonb)
     ORDER BY n.display_name,n.id`).bind(userId, now, now).all<{
            id: string;
            display_name: string;
        }>()).results })(userId, now);
}
export const listWorkspaceCompanies = cache(async (holdingId: HoldingId) => {
    const { write } = await holdingResources(holdingId);
    const { session } = await currentSession();
    return workspaceCompaniesForAccount(write, session.user.id);
});
export async function resolveWorkspaceScope(requested: HoldingWorkspaceScope, options: WorkspaceResolutionOptions = {}) {
    let write: SqlDatabase;
    return createResolveWorkspaceScope({ binding: async (slug) => { const binding = await holdingBindingBySlug(slug); write = (await holdingResources(binding.id)).write; return binding; }, headers, session: currentSession, preference: async (userId, holdingId) => readCompanyPreference((await runtimeEnv()).CONTROL_DB, userId, holdingId), companies: listWorkspaceCompanies, companyBySlug: async (slug) => write.prepare(`SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
             ${organizationSources.authorization_organization_nodes_from} WHERE kind='company' AND slug=? AND status='active'`).bind(slug).first<OrganizationNode>(), companyById: async (id) => write.prepare(`SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
             ${organizationSources.authorization_organization_nodes_from} WHERE kind='company' AND id=? AND status='active'`).bind(id).first<OrganizationNode>(), branch: async (slug, companyId, frontCompanySlug) => frontCompanySlug ? write.prepare(`SELECT n.id,n.kind,n.slug,n.display_name,n.parent_id,n.company_id,n.branch_id ${organizationSources.authorization_organization_nodes_n_from} ` + `${organizationSources.authorization_organization_nodes_c_join} ON c.id=n.company_id AND c.kind='company' AND c.slug=? AND c.status='active' ` + "WHERE n.kind='branch' AND n.slug=? AND n.status='active'").bind(frontCompanySlug, slug).first<OrganizationNode>() : findNode(write, 'branch', slug, companyId), department: async (departmentSlug, branch, companyId, frontCompanySlug) => write.prepare(branch ? `SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
           ${organizationSources.authorization_organization_nodes_from} WHERE kind='department' AND slug=? AND company_id=?
           AND status='active' AND branch_id=? AND parent_id=?` : companyId ? `SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
             ${organizationSources.authorization_organization_nodes_from} WHERE kind='department' AND slug=? AND company_id=?
             AND status='active' AND branch_id IS NULL` : frontCompanySlug ? `SELECT n.id,n.kind,n.slug,n.display_name,n.parent_id,n.company_id,n.branch_id
             ${organizationSources.authorization_organization_nodes_n_from} ${organizationSources.authorization_organization_nodes_c_join} ON c.id=n.company_id AND c.kind='company' AND c.slug=? AND c.status='active'
             WHERE n.kind='department' AND n.slug=? AND n.status='active' AND n.branch_id IS NULL` : `SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
             ${organizationSources.authorization_organization_nodes_from} WHERE kind='department' AND slug=?
             AND status='active' AND branch_id IS NULL`).bind(...(companyId && !branch ? [departmentSlug, companyId] : frontCompanySlug && !branch ? [frontCompanySlug, departmentSlug] : [departmentSlug]), ...(branch ? [branch.company_id, branch.id, branch.id] : [])).first<OrganizationNode>(), lineage: node => validateNodeLineage(write, node), frontCompany: companyId => assertFrontCompany(write, companyId) })(requested, options);
}
async function assertFrontCompany(database: SqlDatabase, companyId: string) {
    return createAssertFrontCompany({ slug: async () => (await headers()).get('x-erpx-front-company'), matches: async (companyId, requestedSlug) => Boolean(await database
            .prepare(`SELECT 1 ${organizationSources.authorization_organization_nodes_from} WHERE id=? AND slug=? AND kind='company' AND status='active'`)
            .bind(companyId, requestedSlug)
            .first()), error: identityError })(companyId);
}
export async function requestedHoldingScope(holdingSlug: string) { return createRequestedHoldingScope({ headers })(holdingSlug); }
async function listAuthorizedWorkspaces(database: SqlDatabase, holdingSlug: string, assignments: WorkAssignment[]): Promise<AuthorizedWorkspace[]> {
    return createAuthorizedWorkspaces({ nodes: async (companyId) => (await database
            .prepare(`SELECT n.id,n.kind,n.slug,n.display_name,n.parent_id,n.branch_id,b.slug AS branch_slug
       ${organizationSources.authorization_organization_nodes_n_from}
       LEFT ${organizationSources.authorization_organization_nodes_b_join} ON n.branch_id=b.id AND b.kind='branch'
       WHERE n.kind IN ('branch','department') AND n.status='active' AND n.company_id=?
       ORDER BY CASE n.kind WHEN 'branch' THEN 1 ELSE 2 END,n.display_name,n.id`)
            .bind(companyId)
            .all<{
            id: string;
            kind: "branch" | "department";
            slug: string;
            display_name: string;
            parent_id: string | null;
            branch_id: string | null;
            branch_slug: string | null;
        }>()).results })(holdingSlug, assignments);
}
/** Current grants and their published definitions share one invocation-local SQL snapshot. */
export async function workspaceGrantsForAccount(write: SqlDatabase, companyId: string, userId: string, now = Date.now()): Promise<{ rows: WorkspaceGrantFact[]; roles: Map<string, RoleRevision> }> {
    const {rows,roles}=await accountAuthoritySnapshot(write,companyId,userId,now);
    return {rows,roles};
}

export async function requireAccountAccess(requested: HoldingWorkspaceScope | string, options: WorkspaceResolutionOptions = {}): Promise<AccountAccessContext> {
    let write: SqlDatabase;
    let definitions = new Map<string,RoleRevision>();
    let readiness: Awaited<ReturnType<typeof accountDirectoryGate>>;
    let observedNow=0;
    return createAccountAccess({ session: currentSession, requested: requestedHoldingScope, scope: resolveWorkspaceScope, ready: async (scope) => { write = (await holdingResources(scope.holdingId)).write; }, now: () => (observedNow=Date.now()), error: identityError,
        revoked: async (scope, sessionId, userId) => { readiness=await accountDirectoryGate(write as PostgresDatabase,scope.companyId,userId,sessionId,{includeAccountGrants:true,observedNow}); return readiness.revoked; },
        refresh: async () => readiness.refresh(),
        assignments: async () => { const facts=readiness.accountGrants; if(!facts)throw identityError(500,'ACCOUNT_ACCESS_UNAVAILABLE','Unable to read current account access.'); definitions=facts.roles; return facts.rows; },
        roles: async (_scope,ids) => new Map(ids.flatMap(id => {const definition=definitions.get(id); return definition ? [[id,definition] as const] : [];})),
    })(requested, options);
}
export async function requireWorkContext(requested: HoldingWorkspaceScope | string, options: WorkspaceResolutionOptions = {}): Promise<WorkContext> {
    try {
        return await cachedWorkContext(JSON.stringify({ requested: typeof requested === "string" ? await requestedHoldingScope(requested) : requested, options }));
    }
    catch (error) {
        if ((await headers()).get("x-erpx-api") === "false") {
            const { pageError } = await import("@/server/modules/work_projection/public/page-error");
            return pageError(error, typeof requested === "string" ? requested : requested.holdingSlug);
        }
        throw error;
    }
}
const cachedWorkContext = cache(async (key: string): Promise<WorkContext> => {
    const { requested, options } = JSON.parse(key) as {
        requested: HoldingWorkspaceScope;
        options: WorkspaceResolutionOptions;
    };
    let write: SqlDatabase;
    return createWorkContext({ access: (requested, options) => measureReadPhase('account_access', () => requireAccountAccess(requested, options)), ready: async (scope) => { write = (await holdingResources(scope.holdingId)).write; }, selection: (access, eligible) => initializeWorkSelection(write, { sessionId: access.sessionId, userId: access.userId, companyId: access.scope.companyId, expiresAt: access.expiresAt }, access.assignments, eligible), now: Date.now, error: identityError, beta: (scope, requestHeaders) => betaAccess(write, scope.companyId, requestHeaders), authority: (scope, userId, role, sessionId) => loadIamAuthority(write, scope.companyId, userId, role, sessionId), feature: (scope, destination, method) => assertProductionFeatureRoute(write, scope.companyId, destination, method), workspaces: (scope, assignments) => listAuthorizedWorkspaces(write, scope.holdingSlug, assignments) })(requested, options);
});
export async function requirePlatformContext(): Promise<PlatformContext> {
    let env: Awaited<ReturnType<typeof runtimeEnv>>;
    return createPlatformContext({ session: currentSession, ready: async () => { env = await runtimeEnv(); }, now: Date.now, error: identityError, assignments: async (userId, now) => (await env.CONTROL_DB.prepare(`SELECT id,role,modules_json FROM platform_assignments
     WHERE user_id=? AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,'platform',floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND revoked_at IS NULL AND valid_from<=? AND valid_until>?
     ORDER BY valid_until ASC,id ASC`)
            .bind(userId, now, now)
            .all<{
            id: string;
            role: string;
            modules_json: string;
        }>()).results, revoked: async (sessionId, userId) => Boolean(await env.CONTROL_DB.prepare("SELECT 1 FROM iam_revoked_sessions WHERE session_id=? AND company_id='platform' AND user_id=?").bind(sessionId, userId).first()), preference: userId => readWorkspacePreference(env.CONTROL_DB, userId), authority: (userId, role, sessionId) => loadIamAuthority(env.CONTROL_DB, 'platform', userId, role, sessionId) })();
}
export async function canEnterManageErpx() {
    let write: SqlDatabase;
    return createCanEnterManageErpx({ session: currentSession, platform: requirePlatformContext, ready: async () => { write = (await holdingResources('h003')).write; }, now: Date.now, grants: async (userId, now) => (await write.prepare(`SELECT a.role,a.company_id FROM assignments a
    ${organizationSources.authorization_organization_nodes_c_join} ON c.id=a.company_id AND c.kind='company' AND c.status='active'
    WHERE a.user_id=? AND a.scope_kind='holding' AND a.revoked_at IS NULL
      AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
      AND a.valid_from<=? AND a.valid_until>?`)
            .bind(userId, now, now, now, now).all<{
            role: string;
            company_id: string;
        }>()).results, roles: (companyId, ids) => iamRoles(write, companyId, ids) })();
}
export async function requireManageErpxContext() { return createManageErpxContext({ headers, work: () => requireWorkContext('erpx'), account: () => requireAccountAccess('erpx'), platform: requirePlatformContext, error: identityError, isApiError: (error): error is ApiError => error instanceof ApiError })(); }
export async function firstAuthorizedWorkspace(holdingSlug: string) {
    return createFirstAuthorizedWorkspace({ nodes: async (holdingSlug) => {
            const binding = await holdingBindingBySlug(holdingSlug), { write } = await holdingResources(binding.id);
            return (await write
                .prepare(`SELECT n.id,n.kind,n.slug,n.branch_id,b.slug AS branch_slug
       ${organizationSources.authorization_organization_nodes_n_from}
       LEFT ${organizationSources.authorization_organization_nodes_b_join} ON n.branch_id=b.id AND b.kind='branch'
       WHERE n.kind IN ('branch','department') AND n.status='active'
       ORDER BY CASE n.kind WHEN 'branch' THEN 1 ELSE 2 END,n.display_name,n.id`)
                .all<{
                id: string;
                kind: "branch" | "department";
                slug: string;
                branch_id: string | null;
                branch_slug: string | null;
            }>()).results;
        }, account: requireAccountAccess })(holdingSlug);
}
