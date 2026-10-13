import type { WorkContext, PlatformContext, IdentitySession } from "../domain/authorization";
import type { IamActor } from "../domain/iam-actor";
import type { PermissionActor } from "@/lib/identity_access/iam";
import type { IdentityError } from "./authorization";
export function createIamContext(ports: {
    session(): Promise<{
        session: IdentitySession;
    }>;
    work(holdingSlug: string): Promise<WorkContext>;
    platform(): Promise<PlatformContext>;
    prepareCompany(holdingId: WorkContext['holdingId']): Promise<void>;
    preparePlatform(): Promise<void>;
    platformAuthority(userId: string, role: string, sessionId: string): Promise<NonNullable<PermissionActor['iam']>>;
}) { return async (holdingSlug: string | null): Promise<IamActor> => { const { session } = await ports.session(); if (holdingSlug) {
    const a = await ports.work(holdingSlug);
    await ports.prepareCompany(a.holdingId);
    return { ...a, sessionId: session.session.id, holdingId: a.holdingId, scopeKind: a.scope.kind, scopeNodeId: a.departmentId ?? a.branchId, platform: false };
} const a = await ports.platform(); await ports.preparePlatform(); return { ...a, iam: await ports.platformAuthority(a.userId, a.role, session.session.id), sessionId: session.session.id, holdingId: null, companyId: 'platform', scopeKind: 'platform', scopeNodeId: null, branchId: null, departmentId: null, platform: true }; }; }
export function createAssertCompanyScope(ports: {
    error: IdentityError;
}) { return (actor: IamActor) => { if (!['holding', 'platform'].includes(actor.scopeKind))
    throw ports.error(403, 'COMPANY_SCOPE_REQUIRED', 'Choose an authorized company scope for this operation.'); }; }
export function createIamActorFromWork(ports: {
    error: IdentityError;
}) { return (a: WorkContext): IamActor => { if (!a.authority)
    throw ports.error(403, 'FRESH_MFA_REQUIRED', 'Sign in and verify MFA before changing access.'); return { ...a, sessionId: a.authority.sessionId, holdingId: a.holdingId, scopeKind: a.scope.kind, scopeNodeId: a.departmentId ?? a.branchId, platform: false }; }; }
