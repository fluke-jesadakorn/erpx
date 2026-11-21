import {readNativeAccountContext,nativeWorkReadModel} from './native-account-context';
import {createNativeAccountCompaniesReader} from './native-account-companies';
import{requestedHoldingScope}from '@erpx/server-identity_access/public/requested-holding-scope';
export{requestedHoldingScope};
import {providerContextSelectedAssignment} from '@erpx/server-identity_access/adapters/provider-context-selection';
import {productHostForHost} from '@erpx/feature-runtime/lib/product-hosts';
import { accountAuthoritySnapshot } from '@erpx/server-identity_access/adapters/account-authority-snapshot';
import { memoizeInRequest } from "@erpx/server-runtime/public/runtime/request-memo";
import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import { measureReadPhase } from "@erpx/server-runtime/public/runtime/read-metrics";
import { loadIamAuthority, iamRoles } from "@erpx/server-identity_access/public/iam-store";
import { readWorkspacePreference } from "@erpx/server-work_projection/public/workspace-preferences";
import { cache } from "react";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { RoleRevision } from "@erpx/feature-identity_access/lib/iam";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import { headers } from "next/headers";
import type { HoldingWorkspaceScope } from "@erpx/feature-work_projection/lib/workspace";
export { assignmentApplies, isRoleValidAtScope } from "@erpx/server-identity_access/public/rules/workspace-access";
import { createIdentitySessionAuthenticator } from "./native-session";
import { runtimeEnv, holdingResources, type HoldingId } from "../../../../packages/server-runtime/src/public/runtime";
export type { AssignmentScopeKind, ResolvedWorkspaceScope, WorkContext, AuthorizedWorkspace, PlatformContext, WorkAssignment, WorkspaceCompany, WorkspaceResolutionOptions, AccountAccessContext } from "@erpx/server-identity_access/domain/authorization";
export { publicScope } from "@erpx/server-identity_access/domain/authorization";
import type { WorkspaceGrantFact, WorkContext, PlatformContext, WorkspaceCompany, WorkspaceResolutionOptions, AccountAccessContext } from "@erpx/server-identity_access/domain/authorization";
import { createIdentitySession, createPlatformContext, createCanEnterManageErpx, createManageErpxContext } from "@erpx/server-identity_access/application/authorization";
const identityError = (status: number, code: string, message: string) => new ApiError(status, code, message);
const sessionMemoKey = Symbol.for("erpx.identity.session");
export const currentSession = cache(() => memoizeInRequest(sessionMemoKey, () => measureReadPhase('session', readCurrentSession)));
async function readCurrentSession() { return createIdentitySession({ headers, authenticate: createIdentitySessionAuthenticator() })(); }
/** Render-only presentation facts. They never replace a command authority check. */
export type WorkspaceCompanyPresentationFact = WorkspaceCompany & { slug: string };
async function nativeAccountCompanies(holdingId?:HoldingId){
    const raw=process.env.ERPX_NATIVE_FIELD_SERVICE_WORKSPACE_CONFIG;
    if(!raw||!process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG)throw identityError(503,'NATIVE_ACCOUNT_COMPANIES_TARGET_UNAVAILABLE','The native Company selection is not configured.');
    const {session}=await currentSession();
    return createNativeAccountCompaniesReader(raw)(await headers(),{sessionId:session.session.id,userId:session.user.id},holdingId);
}
// Resource/time arguments are retained only for callers whose old fixture
// signatures have not moved. Neither can authorize or time-travel a Company read.
export async function workspaceCompaniesForAccount(_write:SqlDatabase,userId:string,_now=Date.now()):Promise<WorkspaceCompany[]>{
    void _write;void _now;const result=await nativeAccountCompanies();
    if(result.userId!==userId)throw identityError(401,'SESSION_REVOKED','The current personal account changed.');
    return result.companies.map(({id,name})=>({id,name}));
}
/** Same render-pass cache boundary; source and eligibility are native-only. */
export const listWorkspaceCompanyPresentationFacts = cache(async(holdingId:HoldingId):Promise<WorkspaceCompanyPresentationFact[]> =>
    (await nativeAccountCompanies(holdingId)).companies);
export const listWorkspaceCompanies = cache(async (holdingId: HoldingId): Promise<WorkspaceCompany[]> =>
    (await listWorkspaceCompanyPresentationFacts(holdingId)).map(({ id, name }) => ({ id, name })));
export async function resolveWorkspaceScope(requested:HoldingWorkspaceScope,options:WorkspaceResolutionOptions={}) {return (await requireAccountAccess(requested,options)).scope;}
/** Current grants and their published definitions share one invocation-local SQL snapshot. */
export async function workspaceGrantsForAccount(write: SqlDatabase, companyId: string, userId: string, now = Date.now()): Promise<{ rows: WorkspaceGrantFact[]; roles: Map<string, RoleRevision> }> {
    const {rows,roles}=await accountAuthoritySnapshot(write,companyId,userId,now);
    return {rows,roles};
}

export async function requireAccountAccess(requested:HoldingWorkspaceScope|string,options:WorkspaceResolutionOptions={}):Promise<AccountAccessContext>{
 const {session,requestHeaders}=await currentSession();
 const scope=typeof requested==='string'?await requestedHoldingScope(requested):requested;
 return (await readNativeAccountContext(session,requestHeaders,scope,options,false)).access;
}
export async function requireWorkContext(requested: HoldingWorkspaceScope | string, options: WorkspaceResolutionOptions = {}): Promise<WorkContext> {
    try {
        return await cachedWorkContext(JSON.stringify({ requested: typeof requested === "string" ? await requestedHoldingScope(requested) : requested, options }));
    }
    catch (error) {
        if ((await headers()).get("x-erpx-api") === "false") {
            const { pageError } = await import("@erpx/server-work_projection/public/page-error");
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
    const {session,requestHeaders}=await currentSession();
    // The previous production-only mutating route policy has not been ported
    // yet. Keep those ingress requests unavailable rather than bypassing it.
    const method=requestHeaders.get('x-erpx-method')??'GET';
    if(process.env.NODE_ENV==='production'&&!['GET','HEAD','OPTIONS'].includes(method))throw identityError(503,'NATIVE_ROUTE_ADMISSION_UNAVAILABLE','The native production route admission is not enabled yet.');
    const {state,access}=await readNativeAccountContext(session,requestHeaders,requested,options,true);
    return nativeWorkReadModel(state,access);
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
        }>()).results, revoked: async (sessionId, userId) => Boolean(await env.CONTROL_DB.prepare("SELECT 1 FROM iam_revoked_sessions WHERE session_id=? AND company_id='platform' AND user_id=?").bind(sessionId, userId).first()), preference: userId => readWorkspacePreference(env.CONTROL_DB, userId), selection: (userId,sessionId,requestHeaders) => productHostForHost(requestHeaders.get('host'))==='admin'?providerContextSelectedAssignment(env.CONTROL_DB,userId,sessionId):Promise.resolve(null), authority: (userId, role, sessionId) => loadIamAuthority(env.CONTROL_DB, 'platform', userId, role, sessionId) })();
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
export async function firstAuthorizedWorkspace(holdingSlug:string):Promise<HoldingWorkspaceScope|null>{
 const {session,requestHeaders}=await currentSession();const companies=await nativeAccountCompanies();
 for(const company of companies.companies){
  const {state}=await readNativeAccountContext(session,requestHeaders,{kind:'holding',holdingSlug},{companyId:company.id},false);
  const w=state.workspaces[0];if(!w)continue;
  if(w.kind==='holding')return {kind:'holding',holdingSlug};
  if(w.kind==='branch'&&w.branchSlug)return {kind:'branch',holdingSlug,branchSlug:w.branchSlug};
  if(w.kind==='department'&&w.departmentSlug)return {kind:'department',holdingSlug,departmentSlug:w.departmentSlug,...(w.branchSlug?{branchSlug:w.branchSlug}:{})};
 }
 return null;
}
