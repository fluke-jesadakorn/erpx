import type { HoldingWorkspaceScope } from "@erpx/feature-work_projection/lib/workspace";
import type { RoleRevision, PermissionActor } from "@erpx/feature-identity_access/lib/iam";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { modulesForRole } from "@erpx/feature-identity_access/lib/business-roles";
import { isRoleValidAtScope } from "@erpx/server-identity_access/domain/workspace-access";
import { cookieValue, type IdentitySession, type WorkContext, type PlatformContext, type WorkAssignment, type AccountAccessContext } from "@erpx/server-identity_access/domain/authorization";
export type IdentityError = (status: number, code: string, message: string) => Error;
export function createRequestedHoldingScope(ports: {
    headers(): Promise<Headers>;
}) { return async (holdingSlug: string): Promise<HoldingWorkspaceScope> => { const requestHeaders = await ports.headers(), kind = requestHeaders.get('x-erpx-workspace-kind'), branchSlug = requestHeaders.get('x-erpx-branch-slug') ?? undefined, departmentSlug = requestHeaders.get('x-erpx-department-slug') ?? undefined; if (kind === 'department' && departmentSlug)
    return { kind, holdingSlug, branchSlug, departmentSlug }; if (kind === 'branch' && branchSlug)
    return { kind, holdingSlug, branchSlug }; return { kind: 'holding', holdingSlug }; }; }
export function createIdentitySession<S extends IdentitySession>(ports: {
    headers(): Promise<Headers>;
    authenticate(origin: string | undefined, headers: Headers): Promise<S | null>;
}) { return async () => { const requestHeaders = await ports.headers(), host = requestHeaders.get('host'), localHost = host?.startsWith('localhost') || host?.startsWith('127.0.0.1') || host?.startsWith('[::1]'), protocol = requestHeaders.get('x-forwarded-proto') ?? (localHost ? 'http' : 'https'), session = await ports.authenticate(host ? `${protocol}://${host}` : undefined, requestHeaders); if (!session)
    throw new Error('UNAUTHENTICATED'); return { session, requestHeaders }; }; }
export function createPlatformContext(ports: {
    session(): Promise<{
        session: IdentitySession;
        requestHeaders: Headers;
    }>;
    ready(): Promise<void>;
    now(): number;
    assignments(userId: string, now: number): Promise<{
        id: string;
        role: string;
        modules_json: string;
    }[]>;
    revoked(sessionId: string, userId: string): Promise<boolean>;
    preference(userId: string): Promise<{
        platform_assignment_id: string | null;
    } | null>;
    selection?(userId: string,sessionId: string,requestHeaders: Headers): Promise<string | null>;
    authority(userId: string, role: string, sessionId: string): Promise<NonNullable<PermissionActor['iam']>>;
    error: IdentityError;
}) { return async (): Promise<PlatformContext> => { const { session, requestHeaders } = await ports.session(); await ports.ready(); const now = ports.now(), rows = await ports.assignments(session.user.id, now), assignments: WorkAssignment[] = rows.map(assignment => ({ id: assignment.id, role: assignment.role, companyId: null, branchId: null, modules: JSON.parse(assignment.modules_json) as string[], scopeKind: 'platform' as const, scopeNodeId: null })).filter(assignment => isRoleValidAtScope(assignment.role, 'platform')); if (await ports.revoked(session.session.id, session.user.id))
    throw ports.error(401, 'SESSION_REVOKED', 'Sign in again to continue.'); const saved = await ports.preference(session.user.id), selectedId = (await ports.selection?.(session.user.id,session.session.id,requestHeaders)) ?? cookieValue(requestHeaders.get('cookie'), 'erpx_assignment_platform') ?? saved?.platform_assignment_id, assignment = assignments.find(item => item.id === selectedId) ?? assignments[0]; if (!assignment)
    throw new Error('FORBIDDEN'); return { iam: await ports.authority(session.user.id, assignment.role, session.session.id), userId: session.user.id, name: session.user.name, email: session.user.email, assignmentId: assignment.id, role: assignment.role, scope: { kind: 'platform' }, modules: modulesForRole(assignment.role, assignment.modules), assignments }; }; }
export function createCanEnterManageErpx(ports: {
    session(): Promise<{
        session: IdentitySession;
    }>;
    platform(): Promise<PlatformContext>;
    ready(): Promise<void>;
    now(): number;
    grants(userId: string, now: number): Promise<{
        role: string;
        company_id: string;
    }[]>;
    roles(companyId: string, ids: string[]): Promise<Map<string, RoleRevision>>;
}) { return async () => { const [{ session }, platform] = await Promise.all([ports.session(), ports.platform()]); if (session.user.id !== platform.userId)
    return false; await ports.ready(); const rows = await ports.grants(session.user.id, ports.now()); for (const companyId of new Set(rows.map(row => row.company_id))) {
    const roles = await ports.roles(companyId, rows.filter(row => row.company_id === companyId).map(row => row.role));
    if (rows.some(row => row.company_id === companyId && roles.get(row.role)?.scopes.includes('holding') && roles.get(row.role)?.permissions.includes('manage_erpx.access')))
        return true;
} return false; }; }
export function createManageErpxContext(ports: {
    headers(): Promise<Headers>;
    work(): Promise<WorkContext>;
    account(): Promise<AccountAccessContext>;
    platform(): Promise<PlatformContext>;
    error: IdentityError;
    isApiError(error: unknown): error is Error & {
        code: string;
        status: number;
    };
}) { return async (): Promise<{
    work: WorkContext;
    platform: PlatformContext;
}> => { const requestHeaders = await ports.headers(), method = requestHeaders.get('x-erpx-method') ?? requestHeaders.get('x-erpx-request-method'); if (method && !['GET', 'HEAD', 'OPTIONS'].includes(method) && (!requestHeaders.has('x-erpx-context-revision') || !requestHeaders.has('x-erpx-platform-assignment')))
    throw ports.error(409, 'WORK_CONTEXT_CHANGED', 'Reload Manage ERPX before submitting.'); let work: WorkContext; try {
    work = await ports.work();
}
catch (error) {
    if (ports.isApiError(error) && ['WORK_ROLE_REQUIRED', 'WORK_ROLE_UNAVAILABLE'].includes(error.code)) {
        const access = await ports.account();
        if (!access.assignments.some(assignment => assignment.scopeKind === 'holding'))
            throw ports.error(403, 'MANAGE_ERPX_FORBIDDEN', 'An ERPX Holding assignment is required.');
    }
    throw error;
} if (work.holdingId !== 'h003' || work.scope.kind !== 'holding' || !hasPermission(work, 'manage_erpx.access'))
    throw ports.error(403, 'MANAGE_ERPX_FORBIDDEN', 'The selected ERPX Holding role cannot access Manage ERPX.'); const platform = await ports.platform(); if (platform.userId !== work.userId)
    throw ports.error(403, 'MANAGE_ERPX_FORBIDDEN', 'The Platform and Holding identities must match.'); if (requestHeaders.has('x-erpx-platform-assignment') && requestHeaders.get('x-erpx-platform-assignment') !== platform.assignmentId)
    throw ports.error(409, 'PLATFORM_CONTEXT_CHANGED', 'Your Platform role changed. Reload and review.'); return { work, platform }; }; }
