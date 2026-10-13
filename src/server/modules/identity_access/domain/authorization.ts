import type { PermissionActor } from "@/lib/identity_access/iam";
import type { WorkAuthority } from "@/server/modules/work_projection/public/work-role-store";
import type { HoldingId } from "@/server/modules/runtime/public/runtime";
import type { WorkspaceScope } from "@/lib/work_projection/workspace";
export type AssignmentScopeKind = "platform" | "holding" | "branch" | "department";
export type ResolvedWorkspaceScope = {
    kind: "platform";
} | {
    kind: "holding";
    holdingId: HoldingId;
    holdingSlug: string;
    holdingName: string;
    companyId: string;
} | {
    kind: "branch";
    holdingId: HoldingId;
    holdingSlug: string;
    holdingName: string;
    companyId: string;
    branchId: string;
    branchSlug: string;
    branchName: string;
} | {
    kind: "department";
    holdingId: HoldingId;
    holdingSlug: string;
    holdingName: string;
    companyId: string;
    branchId: string | null;
    branchSlug?: string;
    branchName?: string;
    departmentId: string;
    departmentSlug: string;
    departmentName: string;
};
export type WorkContext = {
    /** Trusted proxy evidence for permission recovery in the page render. */
    pageRequest?: true;
    inspection?: true;
    iam?: PermissionActor["iam"];
    authority?: WorkAuthority;
    availableAssignments?: WorkAssignment[];
    beta?: boolean;
    userId: string;
    name: string;
    email: string;
    assignmentId: string;
    role: string;
    companyId: string;
    branchId: string | null;
    departmentId: string | null;
    holdingId: HoldingId;
    holdingSlug: string;
    holdingName: string;
    scope: Exclude<ResolvedWorkspaceScope, {
        kind: "platform";
    }>;
    assignmentScope: AssignmentScopeKind;
    inherited: boolean;
    modules: string[];
    assignments: WorkAssignment[];
    workspaces: AuthorizedWorkspace[];
};
export type AuthorizedWorkspace = {
    key: string;
    href: string;
    label: string;
    kind: "holding" | "branch" | "department";
    inherited: boolean;
};
export type PlatformContext = {
    iam?: PermissionActor["iam"];
    userId: string;
    name: string;
    email: string;
    assignmentId: string;
    role: string;
    scope: {
        kind: "platform";
    };
    modules: string[];
    assignments: WorkAssignment[];
};
export type WorkAssignment = {
    version?: number;
    label?: string;
    validUntil?: number;
    id: string;
    role: string;
    companyId: string | null;
    branchId: string | null;
    modules: string[];
    scopeKind: AssignmentScopeKind;
    scopeNodeId: string | null;
};
export type OrganizationNode = {
    id: string;
    kind: "company" | "headquarters" | "branch" | "department" | "team";
    slug: string;
    display_name: string;
    parent_id: string | null;
    company_id: string;
    branch_id: string | null;
};
export type WorkspaceCompany = {
    id: string;
    name: string;
};
export type WorkspaceResolutionOptions = {
    companyId?: string;
};
export type AccountAccessContext = {
    sessionId: string;
    expiresAt: number;
    userId: string;
    name: string;
    email: string;
    scope: Exclude<ResolvedWorkspaceScope, {
        kind: "platform";
    }>;
    assignments: WorkAssignment[];
    requestHeaders: Headers;
};
export function cookieValue(cookieHeader: string | null, name: string) {
    const item = cookieHeader
        ?.split(";")
        .map((part) => part.trim())
        .find((part) => part.startsWith(`${name}=`));
    if (!item)
        return null;
    try {
        return decodeURIComponent(item.slice(name.length + 1));
    }
    catch {
        return null;
    }
}
export function publicScope(context: WorkContext | PlatformContext): WorkspaceScope {
    const scope = context.scope;
    if (scope.kind === "platform")
        return scope;
    if (scope.kind === "holding")
        return { kind: "holding", holdingSlug: scope.holdingSlug };
    if (scope.kind === "branch")
        return {
            kind: "branch",
            holdingSlug: scope.holdingSlug,
            branchSlug: scope.branchSlug,
        };
    return {
        kind: "department",
        holdingSlug: scope.holdingSlug,
        branchSlug: scope.branchSlug,
        departmentSlug: scope.departmentSlug,
    };
}
export type IdentitySession = {
    session: {
        id: string;
        expiresAt: Date | string | number;
    };
    user: {
        id: string;
        name: string;
        email: string;
    };
};
export type WorkspaceBinding = {
    id: HoldingId;
    slug: string;
    displayName: string;
};
export type WorkspaceGrantFact = {
    id: string;
    role: string;
    company_id: string;
    branch_id: string | null;
    modules_json: string;
    scope_kind: "holding" | "branch" | "department";
    scope_node_id: string | null;
    version: number;
    valid_until: number;
    company_name: string;
    scope_name: string | null;
    branch_name: string | null;
};
export type WorkspaceNodeFact = {
    id: string;
    kind: "branch" | "department";
    slug: string;
    display_name: string;
    parent_id: string | null;
    branch_id: string | null;
    branch_slug: string | null;
};
