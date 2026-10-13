import type { HoldingWorkspaceScope } from "@/lib/work_projection/workspace";
import type { RoleRevision, PermissionActor } from "@/lib/identity_access/iam";
import { hasPermission } from "@/lib/identity_access/permissions";
import { modulesForRole } from "@/lib/identity_access/business-roles";
import { assignmentApplies, isRoleValidAtScope } from "../domain/workspace-access";
import type { ActiveWorkSelection } from "@/server/modules/work_projection/public/work-role-store";
import { cookieValue, type IdentitySession, type WorkspaceBinding, type OrganizationNode, type WorkspaceResolutionOptions, type ResolvedWorkspaceScope, type WorkspaceCompany, type AccountAccessContext, type WorkContext, type PlatformContext, type WorkAssignment, type AuthorizedWorkspace, type WorkspaceGrantFact, type WorkspaceNodeFact } from "../domain/authorization";
export type IdentityError = (status: number, code: string, message: string) => Error;
export function createValidateNodeLineage(ports: {
    parent(id: string): Promise<OrganizationNode | null>;
}) { return async (node: OrganizationNode) => { const seen = new Set<string>(); let current: OrganizationNode | null = node; for (let depth = 0; current && depth < 16; depth += 1) {
    if (seen.has(current.id) || current.company_id !== node.company_id)
        throw new Error('INVALID_ORGANIZATION_LINEAGE');
    seen.add(current.id);
    if (current.kind === 'company') {
        if (current.id !== node.company_id || current.parent_id)
            throw new Error('INVALID_ORGANIZATION_LINEAGE');
        return;
    }
    if (!current.parent_id)
        throw new Error('INVALID_ORGANIZATION_LINEAGE');
    current = await ports.parent(current.parent_id);
} throw new Error('INVALID_ORGANIZATION_LINEAGE'); }; }
export interface WorkspaceResolutionPorts {
    binding(slug: string): Promise<WorkspaceBinding>;
    headers(): Promise<Headers>;
    session(): Promise<{
        session: IdentitySession;
        requestHeaders: Headers;
    }>;
    preference(userId: string, holdingId: WorkspaceBinding['id']): Promise<{
        company_id: string;
    } | null>;
    companies(holdingId: WorkspaceBinding['id']): Promise<WorkspaceCompany[]>;
    companyBySlug(slug: string): Promise<OrganizationNode | null>;
    companyById(id: string): Promise<OrganizationNode | null>;
    branch(slug: string, companyId: string | undefined, frontCompanySlug: string | null): Promise<OrganizationNode | null>;
    department(slug: string, branch: OrganizationNode | null, companyId: string | undefined, frontCompanySlug: string | null): Promise<OrganizationNode | null>;
    lineage(node: OrganizationNode): Promise<void>;
    frontCompany(companyId: string): Promise<void>;
}
export function createResolveWorkspaceScope(ports: WorkspaceResolutionPorts) { return async (requested: HoldingWorkspaceScope, options: WorkspaceResolutionOptions = {}): Promise<Exclude<ResolvedWorkspaceScope, {
    kind: 'platform';
}>> => { const binding = await ports.binding(requested.holdingSlug); if (requested.kind === 'holding') {
    const requestHeaders = await ports.headers(), contextCompany = requestHeaders.get('x-erpx-context-company'), frontCompanySlug = requestHeaders.get('x-erpx-front-company'), { session } = await ports.session(), explicitCompany = options.companyId ?? contextCompany ?? cookieValue(requestHeaders.get('cookie'), `erpx_company_${binding.id}`), preferred = explicitCompany ?? (frontCompanySlug ? undefined : (await ports.preference(session.user.id, binding.id))?.company_id), available = !frontCompanySlug && !preferred ? await ports.companies(binding.id) : [], selectedCompany = preferred ?? (available.length === 1 ? available[0].id : undefined), company = frontCompanySlug ? await ports.companyBySlug(frontCompanySlug) : selectedCompany ? await ports.companyById(selectedCompany) : null;
    if (!company)
        throw new Error('COMPANY_REQUIRED');
    await ports.frontCompany(company.id);
    return { kind: 'holding', holdingId: binding.id, holdingSlug: binding.slug, holdingName: binding.displayName, companyId: company.id };
} if (requested.kind === 'branch') {
    const branch = await ports.branch(requested.branchSlug, options.companyId, (await ports.headers()).get('x-erpx-front-company'));
    if (!branch || options.companyId && branch.company_id !== options.companyId)
        throw new Error('WORKSPACE_NOT_FOUND');
    await ports.lineage(branch);
    await ports.frontCompany(branch.company_id);
    return { kind: 'branch', holdingId: binding.id, holdingSlug: binding.slug, holdingName: binding.displayName, companyId: branch.company_id, branchId: branch.id, branchSlug: branch.slug, branchName: branch.display_name };
} let branch: OrganizationNode | null = null; if (requested.branchSlug) {
    branch = await ports.branch(requested.branchSlug, options.companyId, (await ports.headers()).get('x-erpx-front-company'));
    if (!branch || options.companyId && branch.company_id !== options.companyId)
        throw new Error('WORKSPACE_NOT_FOUND');
} const department = await ports.department(requested.departmentSlug, branch, options.companyId, (await ports.headers()).get('x-erpx-front-company')); if (!department)
    throw new Error('WORKSPACE_NOT_FOUND'); await ports.lineage(department); await ports.frontCompany(department.company_id); return { kind: 'department', holdingId: binding.id, holdingSlug: binding.slug, holdingName: binding.displayName, companyId: department.company_id, branchId: department.branch_id, branchSlug: branch?.slug, branchName: branch?.display_name, departmentId: department.id, departmentSlug: department.slug, departmentName: department.display_name }; }; }
export function createAssertFrontCompany(ports: {
    slug(): Promise<string | null>;
    matches(companyId: string, slug: string): Promise<boolean>;
    error: IdentityError;
}) { return async (companyId: string) => { const slug = await ports.slug(); if (!slug)
    return; if (!await ports.matches(companyId, slug))
    throw ports.error(404, 'COMPANY_NOT_FOUND', 'This storefront does not match the selected Company.'); }; }
export function createWorkspaceCompanies(ports: {
    companies(userId: string, now: number): Promise<{
        id: string;
        display_name: string;
    }[]>;
    now(): number;
}) { return async (userId: string, now = ports.now()) => [...new Map((await ports.companies(userId, now)).map(row => [row.id, { id: row.id, name: row.display_name }])).values()]; }
export function createRequestedHoldingScope(ports: {
    headers(): Promise<Headers>;
}) { return async (holdingSlug: string): Promise<HoldingWorkspaceScope> => { const requestHeaders = await ports.headers(), kind = requestHeaders.get('x-erpx-workspace-kind'), branchSlug = requestHeaders.get('x-erpx-branch-slug') ?? undefined, departmentSlug = requestHeaders.get('x-erpx-department-slug') ?? undefined; if (kind === 'department' && departmentSlug)
    return { kind, holdingSlug, branchSlug, departmentSlug }; if (kind === 'branch' && branchSlug)
    return { kind, holdingSlug, branchSlug }; return { kind: 'holding', holdingSlug }; }; }
export function createAccountAccess(ports: {
    session(): Promise<{
        session: IdentitySession;
        requestHeaders: Headers;
    }>;
    requested(slug: string): Promise<HoldingWorkspaceScope>;
    scope(requested: HoldingWorkspaceScope, options: WorkspaceResolutionOptions): Promise<Exclude<ResolvedWorkspaceScope, {
        kind: 'platform';
    }>>;
    ready(scope: Exclude<ResolvedWorkspaceScope, {
        kind: 'platform';
    }>): Promise<void>;
    now(): number;
    error: IdentityError;
    revoked(scope: Exclude<ResolvedWorkspaceScope, {
        kind: 'platform';
    }>, sessionId: string, userId: string): Promise<boolean>;
    refresh(scope: Exclude<ResolvedWorkspaceScope, {
        kind: 'platform';
    }>, userId: string): Promise<void>;
    assignments(scope: Exclude<ResolvedWorkspaceScope, {
        kind: 'platform';
    }>, userId: string, now: number): Promise<WorkspaceGrantFact[]>;
    roles(scope: Exclude<ResolvedWorkspaceScope, {
        kind: 'platform';
    }>, ids: string[]): Promise<Map<string, RoleRevision>>;
}) { return async (requested: HoldingWorkspaceScope | string, options: WorkspaceResolutionOptions = {}): Promise<AccountAccessContext> => { const { session, requestHeaders } = await ports.session(), scope = await ports.scope(typeof requested === 'string' ? await ports.requested(requested) : requested, options); await ports.ready(scope); const now = ports.now(); if (await ports.revoked(scope, session.session.id, session.user.id))
    throw ports.error(401, 'SESSION_REVOKED', 'Sign in again to access this company.'); await ports.refresh(scope, session.user.id); const rows = await ports.assignments(scope, session.user.id, now), roles = await ports.roles(scope, rows.map(a => a.role)), validRows = []; for (const a of rows) {
    const definition = roles.get(a.role);
    if (definition?.scopes.includes(a.scope_kind))
        validRows.push({ ...a, roleName: definition.name });
} const assignments = validRows.map(a => ({ id: a.id, role: a.role, companyId: a.company_id, branchId: a.branch_id, modules: JSON.parse(a.modules_json) as string[], scopeKind: a.scope_kind, scopeNodeId: a.scope_node_id, version: a.version, validUntil: a.valid_until, label: [a.roleName, a.company_name, a.branch_name, a.scope_name].filter(Boolean).join(' · ') })); return { sessionId: session.session.id, expiresAt: new Date(session.session.expiresAt).getTime(), userId: session.user.id, name: session.user.name, email: session.user.email, scope, assignments, requestHeaders }; }; }
export function createWorkContext(ports: {
    access(requested: HoldingWorkspaceScope, options: WorkspaceResolutionOptions): Promise<AccountAccessContext>;
    ready(scope: AccountAccessContext['scope']): Promise<void>;
    selection(access: AccountAccessContext, eligible: WorkAssignment[]): Promise<ActiveWorkSelection | null>;
    now(): number;
    error: IdentityError;
    beta(scope: AccountAccessContext['scope'], headers: Headers): Promise<boolean>;
    authority(scope: AccountAccessContext['scope'], userId: string, role: string, sessionId: string): Promise<NonNullable<PermissionActor['iam']>>;
    feature(scope: AccountAccessContext['scope'], destination: string, method: string): Promise<void>;
    workspaces(scope: AccountAccessContext['scope'], assignments: WorkAssignment[]): Promise<AuthorizedWorkspace[]>;
}) { return async (requested: HoldingWorkspaceScope, options: WorkspaceResolutionOptions = {}): Promise<WorkContext> => { const access = await ports.access(requested, options), { scope } = access; await ports.ready(scope); const eligible = access.assignments.filter(a => assignmentApplies(a, scope)), selection = await ports.selection(access, eligible); if (!selection)
    throw ports.error(409, 'WORK_ROLE_REQUIRED', 'Choose a work role to continue.'); const assignment = eligible.find(a => a.id === selection.assignment_id && a.version === selection.assignment_version); if (!assignment || selection.expires_at <= ports.now())
    throw ports.error(409, 'WORK_ROLE_UNAVAILABLE', 'Your selected role no longer covers this workspace. Choose an available role.'); const method = access.requestHeaders.get('x-erpx-method'); if ((access.requestHeaders.has('x-erpx-context-revision') || method && !['GET', 'HEAD', 'OPTIONS'].includes(method)) && (access.requestHeaders.get('x-erpx-context-revision') !== String(selection.revision) || access.requestHeaders.get('x-erpx-context-company') !== scope.companyId || access.requestHeaders.get('x-erpx-context-assignment') !== assignment.id))
    throw ports.error(409, 'WORK_CONTEXT_CHANGED', 'Your work role changed. Reload and review before submitting.'); const [beta, iam] = await Promise.all([ports.beta(scope, access.requestHeaders), ports.authority(scope, access.userId, assignment.role, access.sessionId)]), assignedModules = beta ? [] : assignment.role.startsWith('custom_') ? [...new Set(iam.permissions.filter(p => p.startsWith('module.')).map(p => p.split('.')[1]))] : modulesForRole(assignment.role, assignment.modules), modules = iam.permissions.includes('module.people_payroll.read') ? [...new Set([...assignedModules, 'people_payroll'])] : assignedModules.filter(module => module !== 'people_payroll'); await ports.feature(scope, access.requestHeaders.get('x-erpx-destination') ?? '', method ?? 'GET'); return { ...(access.requestHeaders.get('x-erpx-api') === 'false' ? { pageRequest: true as const } : {}), iam, userId: access.userId, name: access.name, email: access.email, assignmentId: assignment.id, role: assignment.role, companyId: scope.companyId, branchId: scope.kind === 'holding' ? null : scope.branchId, departmentId: scope.kind === 'department' ? scope.departmentId : null, holdingId: scope.holdingId, holdingSlug: scope.holdingSlug, holdingName: scope.holdingName, beta, scope, assignmentScope: assignment.scopeKind, inherited: assignment.scopeKind !== scope.kind, modules, assignments: [assignment], availableAssignments: access.assignments, authority: { sessionId: access.sessionId, revision: selection.revision, assignmentVersion: assignment.version! }, workspaces: await ports.workspaces(scope, access.assignments) }; }; }
export function createAuthorizedWorkspaces(ports: {
    nodes(companyId: string): Promise<WorkspaceNodeFact[]>;
}) {
    return async (holdingSlug: string, assignments: WorkAssignment[]): Promise<AuthorizedWorkspace[]> => {
        const holdingGrant = assignments.some((assignment) => assignment.scopeKind === "holding");
        const result = { results: await ports.nodes(assignments[0]?.companyId ?? "") };
        const workspaces: AuthorizedWorkspace[] = [];
        if (holdingGrant)
            workspaces.push({
                key: `holding:${holdingSlug}`,
                href: `/holdings/${encodeURIComponent(holdingSlug)}/home`,
                label: "Holding",
                kind: "holding",
                inherited: false,
            });
        for (const node of result.results) {
            const exact = assignments.some((assignment) => assignment.scopeKind === node.kind && assignment.scopeNodeId === node.id);
            const branchGrant = node.kind === "department" &&
                assignments.some((assignment) => assignment.scopeKind === "branch" &&
                    assignment.scopeNodeId === node.branch_id);
            if (!holdingGrant && !exact && !branchGrant)
                continue;
            const prefix = `/holdings/${encodeURIComponent(holdingSlug)}`;
            const href = node.kind === "branch"
                ? `${prefix}/branches/${encodeURIComponent(node.slug)}/home`
                : node.branch_slug
                    ? `${prefix}/branches/${encodeURIComponent(node.branch_slug)}/departments/${encodeURIComponent(node.slug)}/home`
                    : `${prefix}/departments/${encodeURIComponent(node.slug)}/home`;
            workspaces.push({
                key: `${node.kind}:${node.id}`,
                href,
                label: node.display_name,
                kind: node.kind,
                inherited: !exact,
            });
        }
        return workspaces;
    };
}
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
    authority(userId: string, role: string, sessionId: string): Promise<NonNullable<PermissionActor['iam']>>;
    error: IdentityError;
}) { return async (): Promise<PlatformContext> => { const { session, requestHeaders } = await ports.session(); await ports.ready(); const now = ports.now(), rows = await ports.assignments(session.user.id, now), assignments: WorkAssignment[] = rows.map(assignment => ({ id: assignment.id, role: assignment.role, companyId: null, branchId: null, modules: JSON.parse(assignment.modules_json) as string[], scopeKind: 'platform' as const, scopeNodeId: null })).filter(assignment => isRoleValidAtScope(assignment.role, 'platform')); if (await ports.revoked(session.session.id, session.user.id))
    throw ports.error(401, 'SESSION_REVOKED', 'Sign in again to continue.'); const saved = await ports.preference(session.user.id), selectedId = cookieValue(requestHeaders.get('cookie'), 'erpx_assignment_platform') ?? saved?.platform_assignment_id, assignment = assignments.find(item => item.id === selectedId) ?? assignments[0]; if (!assignment)
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
export function createFirstAuthorizedWorkspace(ports: {
    nodes(holdingSlug: string): Promise<Pick<WorkspaceNodeFact, 'id' | 'kind' | 'slug' | 'branch_id' | 'branch_slug'>[]>;
    account(candidate: HoldingWorkspaceScope): Promise<AccountAccessContext>;
}) { return async (holdingSlug: string): Promise<HoldingWorkspaceScope | null> => { const nodes = await ports.nodes(holdingSlug), candidates: HoldingWorkspaceScope[] = [{ kind: 'holding', holdingSlug }, ...nodes.map((node): HoldingWorkspaceScope => node.kind === 'branch' ? { kind: 'branch', holdingSlug, branchSlug: node.slug } : { kind: 'department', holdingSlug, branchSlug: node.branch_slug ?? undefined, departmentSlug: node.slug })]; for (const candidate of candidates) {
    try {
        const account = await ports.account(candidate);
        if (account.assignments.some(assignment => assignmentApplies(assignment, account.scope)))
            return candidate;
    }
    catch { /* Try the next active workspace candidate. */ }
} return null; }; }
