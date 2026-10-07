import { parseHoldingWorkspace, type HoldingWorkspaceScope } from '@/lib/workspace';
import { assignmentProblem, safeReturnPath } from '../domain/access-problem';
import type { AccessProblem } from '@/lib/access-problem';
import { assignmentApplies, isRoleValidAtScope } from '../domain/workspace-access';
import type { ResolvedWorkspaceScope, WorkAssignment } from '../domain/authorization';
type AssignmentRow = {
    role: string;
    company_id: string;
    branch_id: string | null;
    scope_kind: "holding" | "branch" | "department";
    scope_node_id: string | null;
    valid_from: number;
    valid_until: number;
    revoked_at: number | null;
};
type Node = {
    id: string;
    parent_id: string | null;
    company_id: string;
    status: string;
};
export interface AccessDiagnosisPorts {
    session(): Promise<{
        session: {
            user: {
                id: string;
                email: string;
            };
        };
    }>;
    platform(): Promise<unknown>;
    work(scope: HoldingWorkspaceScope): Promise<unknown>;
    scope(scope: HoldingWorkspaceScope): Promise<Exclude<ResolvedWorkspaceScope, {
        kind: 'platform';
    }>>;
    now(): number;
    apiError(error: unknown): error is Error & {
        code: string;
        status: number;
    };
    redirect(url: string): never;
    platformAssignments(userId: string): Promise<{
        role: string;
        valid_from: number;
        valid_until: number;
        revoked_at: number | null;
    }[]>;
    binding(slug: string): Promise<{
        holding_id: string;
        status: string;
    } | null>;
    member(holdingId: string, userId: string): Promise<boolean>;
    scopedNode(holdingId: string, kind: string, slug: string | null, branchSlug: string | null, userId: string): Promise<Node | null>;
    parent(holdingId: string, id: string, companyId: string): Promise<Node | null>;
    assignments(scope: Exclude<ResolvedWorkspaceScope, {
        kind: 'platform';
    }>, userId: string): Promise<AssignmentRow[]>;
}
const asAssignment = (r: AssignmentRow): WorkAssignment => ({ id: "diagnostic", role: r.role, companyId: r.company_id, branchId: r.branch_id, scopeKind: r.scope_kind, scopeNodeId: r.scope_node_id, modules: [] });
export function createAccessDiagnosis(ports: AccessDiagnosisPorts) {
    return async (from: string | null, hint?: AccessProblem): Promise<{
        problem: AccessProblem;
        email?: string;
    }> => {
        let identity;
        try {
            identity = await ports.session();
        }
        catch (error) {
            if (error instanceof Error && error.message === "UNAUTHENTICATED")
                return { problem: "sign_in" };
            throw error;
        }
        const email = identity.session.user.email, userId = identity.session.user.id;
        if (!safeReturnPath(from))
            return { problem: "invalid", email };
        if (from!.startsWith("/platform/")) {
            try {
                await ports.platform();
                return { problem: hint ?? "available", email };
            }
            catch (error) {
                if (!(error instanceof Error) || error.message !== "FORBIDDEN")
                    throw error;
            }
            const rows = await ports.platformAssignments(userId);
            const reason = assignmentProblem(rows.map(r => ({ ...r, applies: true, validRole: isRoleValidAtScope(r.role, "platform") })), ports.now());
            return { problem: reason === "no_access" ? "platform" : reason, email };
        }
        let parts: string[];
        try {
            parts = new URL(from!, "https://erpx.invalid").pathname.split("/").filter(Boolean).map(decodeURIComponent);
        }
        catch {
            return { problem: "invalid", email };
        }
        const parsed = parseHoldingWorkspace(parts[1], parts.slice(2));
        if (!parsed)
            return { problem: "invalid", email };
        try {
            await ports.work(parsed.scope);
            return { problem: hint ?? "available", email };
        }
        catch (error) {
            if (ports.apiError(error) && ["WORK_ROLE_REQUIRED", "WORK_ROLE_UNAVAILABLE", "WORK_CONTEXT_CHANGED"].includes(error.code))
                ports.redirect(`/work-role?${new URLSearchParams({ from: from! })}`);
            const code = error instanceof Error ? error.message : "";
            if (code === "COMPANY_REQUIRED")
                return { problem: "company", email };
            if (["HOLDING_NOT_FOUND", "WORKSPACE_NOT_FOUND", "INVALID_ORGANIZATION_LINEAGE"].includes(code)) {
                // Inactive status is disclosed only to an existing member, never to outsiders.
                const binding = await ports.binding(parts[1]);
                if (binding) {
                    const member = await ports.member(binding.holding_id, userId);
                    if (binding.status !== "active" && member)
                        return { problem: "suspended", email };
                    if (parsed.scope.kind !== "holding") {
                        const branchSlug = parsed.scope.branchSlug ?? null;
                        const kind = parsed.scope.kind;
                        const slug = kind === "branch" ? branchSlug : parsed.scope.kind === "department" ? parsed.scope.departmentSlug : null;
                        let node = await ports.scopedNode(binding.holding_id, kind, slug, branchSlug, userId);
                        const seen = new Set<string>();
                        while (node && !seen.has(node.id) && seen.size < 16) {
                            if (node.status !== "active")
                                return { problem: "suspended", email };
                            seen.add(node.id);
                            node = node.parent_id ? await ports.parent(binding.holding_id, node.parent_id, node.company_id) : null;
                        }
                    }
                }
                return { problem: code === "INVALID_ORGANIZATION_LINEAGE" ? "setup" : "workspace", email };
            }
            if (code !== "FORBIDDEN")
                throw error;
            const scope = await ports.scope(parsed.scope);
            const rows = await ports.assignments(scope, userId);
            return { problem: assignmentProblem(rows.map(r => ({ ...r, applies: assignmentApplies(asAssignment(r), scope), validRole: isRoleValidAtScope(r.role, r.scope_kind) })), ports.now()), email };
        }
    };
}
