import type { SqlDatabase } from "./runtime/types";
import { betaAccess } from "./beta-access";
import "server-only";
import { modulesForRole } from "@/lib/business-roles";

import { headers } from "next/headers";
import type { HoldingWorkspaceScope, WorkspaceScope } from "@/lib/workspace";
import {
  assignmentApplies,
  isRoleValidAtScope,
} from "@/lib/workspace-access";
export { assignmentApplies, isRoleValidAtScope } from "@/lib/workspace-access";
import { createAuth } from "./auth";
import {
  runtimeEnv,
  holdingBindingBySlug,
  holdingResources,
  type HoldingId,
} from "./runtime";

export type AssignmentScopeKind =
  | "platform"
  | "holding"
  | "branch"
  | "department";

export type ResolvedWorkspaceScope =
  | { kind: "platform" }
  | {
      kind: "holding";
      holdingId: HoldingId;
      holdingSlug: string;
      holdingName: string;
      companyId: string;
    }
  | {
      kind: "branch";
      holdingId: HoldingId;
      holdingSlug: string;
      holdingName: string;
      companyId: string;
      branchId: string;
      branchSlug: string;
      branchName: string;
    }
  | {
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
  scope: Exclude<ResolvedWorkspaceScope, { kind: "platform" }>;
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
  userId: string;
  name: string;
  email: string;
  assignmentId: string;
  role: string;
  scope: { kind: "platform" };
  modules: string[];
  assignments: WorkAssignment[];
};

export type WorkAssignment = {
  id: string;
  role: string;
  companyId: string | null;
  branchId: string | null;
  modules: string[];
  scopeKind: AssignmentScopeKind;
  scopeNodeId: string | null;
};

type OrganizationNode = {
  id: string;
  kind: "company" | "headquarters" | "branch" | "department" | "team";
  slug: string;
  display_name: string;
  parent_id: string | null;
  company_id: string;
  branch_id: string | null;
};

function cookieValue(cookieHeader: string | null, name: string) {
  const item = cookieHeader
    ?.split(";")
    .map((part) => part.trim())
    .find((part) => part.startsWith(`${name}=`));
  if (!item) return null;
  try {
    return decodeURIComponent(item.slice(name.length + 1));
  } catch {
    return null;
  }
}

async function currentSession() {
  const requestHeaders = await headers();
  const host = requestHeaders.get("host");
  const localHost =
    host?.startsWith("localhost") ||
    host?.startsWith("127.0.0.1") ||
    host?.startsWith("[::1]");
  const protocol =
    requestHeaders.get("x-forwarded-proto") ?? (localHost ? "http" : "https");
  const auth = await createAuth(host ? `${protocol}://${host}` : undefined);
  const session = await auth.api.getSession({ headers: requestHeaders });
  if (!session) throw new Error("UNAUTHENTICATED");
  return { session, requestHeaders };
}

async function findNode(
  database: SqlDatabase,
  kind: OrganizationNode["kind"],
  slug: string,
) {
  return database
    .prepare(
      `SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
       FROM organization_nodes WHERE kind=? AND slug=? AND status='active'`,
    )
    .bind(kind, slug)
    .first<OrganizationNode>();
}

async function validateNodeLineage(
  database: SqlDatabase,
  node: OrganizationNode,
) {
  const seen = new Set<string>();
  let current: OrganizationNode | null = node;
  for (let depth = 0; current && depth < 16; depth += 1) {
    if (seen.has(current.id) || current.company_id !== node.company_id)
      throw new Error("INVALID_ORGANIZATION_LINEAGE");
    seen.add(current.id);
    if (current.kind === "company") {
      if (current.id !== node.company_id || current.parent_id)
        throw new Error("INVALID_ORGANIZATION_LINEAGE");
      return;
    }
    if (!current.parent_id) throw new Error("INVALID_ORGANIZATION_LINEAGE");
    current = await database
      .prepare(
        `SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
         FROM organization_nodes WHERE id=? AND status='active'`,
      )
      .bind(current.parent_id)
      .first<OrganizationNode>();
  }
  throw new Error("INVALID_ORGANIZATION_LINEAGE");
}

export type WorkspaceCompany = { id: string; name: string };

export async function listWorkspaceCompanies(holdingId: HoldingId) {
  const { write } = await holdingResources(holdingId);
  const result = await write
    .prepare(
      `SELECT id,display_name FROM organization_nodes
       WHERE kind='company' AND status='active' ORDER BY display_name,id`,
    )
    .all<{ id: string; display_name: string }>();
  return result.results.map((company) => ({
    id: company.id,
    name: company.display_name,
  }));
}

export async function resolveWorkspaceScope(
  requested: HoldingWorkspaceScope,
): Promise<Exclude<ResolvedWorkspaceScope, { kind: "platform" }>> {
  const binding = await holdingBindingBySlug(requested.holdingSlug);
  const { write } = await holdingResources(binding.id);
  if (requested.kind === "holding") {
    const requestHeaders = await headers();
    const preferred = cookieValue(
      requestHeaders.get("cookie"),
      `erpx_company_${binding.id}`,
    );
    const company = preferred
      ? await write
          .prepare(
            `SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
             FROM organization_nodes WHERE kind='company' AND id=? AND status='active'`,
          )
          .bind(preferred)
          .first<OrganizationNode>()
      : await write
          .prepare(
            `SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
             FROM organization_nodes WHERE kind='company' AND status='active'
             ORDER BY display_name,id LIMIT 1`,
          )
          .first<OrganizationNode>();
    if (!company) throw new Error("COMPANY_REQUIRED");
    return {
      kind: "holding",
      holdingId: binding.id,
      holdingSlug: binding.slug,
      holdingName: binding.displayName,
      companyId: company.id,
    };
  }
  if (requested.kind === "branch") {
    const branch = await findNode(write, "branch", requested.branchSlug);
    if (!branch) throw new Error("WORKSPACE_NOT_FOUND");
    await validateNodeLineage(write, branch);
    return {
      kind: "branch",
      holdingId: binding.id,
      holdingSlug: binding.slug,
      holdingName: binding.displayName,
      companyId: branch.company_id,
      branchId: branch.id,
      branchSlug: branch.slug,
      branchName: branch.display_name,
    };
  }
  let branch: OrganizationNode | null = null;
  if (requested.branchSlug) {
    branch = await findNode(write, "branch", requested.branchSlug);
    if (!branch) throw new Error("WORKSPACE_NOT_FOUND");
  }
  const department = await write
    .prepare(
      branch
        ? `SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
           FROM organization_nodes WHERE kind='department' AND slug=?
           AND status='active' AND branch_id=? AND parent_id=?`
        : `SELECT id,kind,slug,display_name,parent_id,company_id,branch_id
           FROM organization_nodes WHERE kind='department' AND slug=?
           AND status='active' AND branch_id IS NULL`,
    )
    .bind(
      requested.departmentSlug,
      ...(branch ? [branch.id, branch.id] : []),
    )
    .first<OrganizationNode>();
  if (!department) throw new Error("WORKSPACE_NOT_FOUND");
  await validateNodeLineage(write, department);
  return {
    kind: "department",
    holdingId: binding.id,
    holdingSlug: binding.slug,
    holdingName: binding.displayName,
    companyId: department.company_id,
    branchId: department.branch_id,
    branchSlug: branch?.slug,
    branchName: branch?.display_name,
    departmentId: department.id,
    departmentSlug: department.slug,
    departmentName: department.display_name,
  };
}

export async function requestedHoldingScope(
  holdingSlug: string,
): Promise<HoldingWorkspaceScope> {
  const requestHeaders = await headers();
  const kind = requestHeaders.get("x-erpx-workspace-kind");
  const branchSlug = requestHeaders.get("x-erpx-branch-slug") ?? undefined;
  const departmentSlug =
    requestHeaders.get("x-erpx-department-slug") ?? undefined;
  if (kind === "department" && departmentSlug)
    return { kind, holdingSlug, branchSlug, departmentSlug };
  if (kind === "branch" && branchSlug)
    return { kind, holdingSlug, branchSlug };
  return { kind: "holding", holdingSlug };
}

function specificity(assignment: WorkAssignment) {
  if (assignment.scopeKind === "department") return 3;
  if (assignment.scopeKind === "branch") return 2;
  return 1;
}

async function listAuthorizedWorkspaces(
  database: SqlDatabase,
  holdingSlug: string,
  assignments: WorkAssignment[],
): Promise<AuthorizedWorkspace[]> {
  const holdingGrant = assignments.some(
    (assignment) => assignment.scopeKind === "holding",
  );
  const result = await database
    .prepare(
      `SELECT n.id,n.kind,n.slug,n.display_name,n.parent_id,n.branch_id,b.slug AS branch_slug
       FROM organization_nodes n
       LEFT JOIN organization_nodes b ON n.branch_id=b.id AND b.kind='branch'
       WHERE n.kind IN ('branch','department') AND n.status='active' AND n.company_id=?
       ORDER BY CASE n.kind WHEN 'branch' THEN 1 ELSE 2 END,n.display_name,n.id`,
    )
    .bind(assignments[0]?.companyId ?? "")
    .all<{
      id: string;
      kind: "branch" | "department";
      slug: string;
      display_name: string;
      parent_id: string | null;
      branch_id: string | null;
      branch_slug: string | null;
    }>();
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
    const exact = assignments.some(
      (assignment) =>
        assignment.scopeKind === node.kind && assignment.scopeNodeId === node.id,
    );
    const branchGrant =
      node.kind === "department" &&
      assignments.some(
        (assignment) =>
          assignment.scopeKind === "branch" &&
          assignment.scopeNodeId === node.branch_id,
      );
    if (!holdingGrant && !exact && !branchGrant) continue;
    const prefix = `/holdings/${encodeURIComponent(holdingSlug)}`;
    const href =
      node.kind === "branch"
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
}

export async function requireWorkContext(
  requested: HoldingWorkspaceScope | string,
): Promise<WorkContext> {
  const scope = await resolveWorkspaceScope(
    typeof requested === "string"
      ? await requestedHoldingScope(requested)
      : requested,
  );
  const { session, requestHeaders } = await currentSession();
  const { write } = await holdingResources(scope.holdingId);
  const now = Date.now();
  const result = await write
    .prepare(
      `SELECT id,role,company_id,branch_id,modules_json,scope_kind,scope_node_id
       FROM assignments
       WHERE user_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>?
       ORDER BY valid_until ASC,id ASC`,
    )
    .bind(session.user.id, now, now)
    .all<{
      id: string;
      role: string;
      company_id: string | null;
      branch_id: string | null;
      modules_json: string;
      scope_kind: "holding" | "branch" | "department";
      scope_node_id: string | null;
    }>();
  const assignments: WorkAssignment[] = result.results.map((assignment) => ({
    id: assignment.id,
    role: assignment.role,
    companyId: assignment.company_id,
    branchId: assignment.branch_id,
    modules: JSON.parse(assignment.modules_json) as string[],
    scopeKind: assignment.scope_kind,
    scopeNodeId: assignment.scope_node_id,
  })).filter((assignment) => isRoleValidAtScope(assignment.role, assignment.scopeKind));
  const eligible = assignments
    .filter((assignment) => assignmentApplies(assignment, scope))
    .sort((left, right) => specificity(right) - specificity(left));
  const selectedId = cookieValue(
    requestHeaders.get("cookie"),
    `erpx_assignment_${scope.holdingId}`,
  );
  const assignment =
    eligible.find((item) => item.id === selectedId) ?? eligible[0];
  if (!assignment) throw new Error("FORBIDDEN");
  const beta = await betaAccess(write,scope.companyId,requestHeaders);
  const workspaces = await listAuthorizedWorkspaces(
    write,
    scope.holdingSlug,
    assignments.filter((a) => a.companyId === scope.companyId),
  );
  return {
    userId: session.user.id,
    name: session.user.name,
    email: session.user.email,
    assignmentId: assignment.id,
    role: assignment.role,
    companyId: scope.companyId,
    branchId: scope.kind === "holding" ? null : scope.branchId,
    departmentId: scope.kind === "department" ? scope.departmentId : null,
    holdingId: scope.holdingId,
    holdingSlug: scope.holdingSlug,
    holdingName: scope.holdingName,
    beta,
    scope,
    assignmentScope: assignment.scopeKind,
    inherited: assignment.scopeKind !== scope.kind,
    modules: beta ? [] : [...new Set(eligible.flatMap((a) => modulesForRole(a.role, a.modules)))],
    assignments: eligible,
    workspaces,
  };
}

export async function requirePlatformContext(): Promise<PlatformContext> {
  const { session, requestHeaders } = await currentSession();
  const env = await runtimeEnv();
  const now = Date.now();
  const result = await env.CONTROL_DB.prepare(
    `SELECT id,role,modules_json FROM platform_assignments
     WHERE user_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>?
     ORDER BY valid_until ASC,id ASC`,
  )
    .bind(session.user.id, now, now)
    .all<{ id: string; role: string; modules_json: string }>();
  const assignments: WorkAssignment[] = result.results.map((assignment) => ({
    id: assignment.id,
    role: assignment.role,
    companyId: null,
    branchId: null,
    modules: JSON.parse(assignment.modules_json) as string[],
    scopeKind: "platform" as const,
    scopeNodeId: null,
  })).filter((assignment) => isRoleValidAtScope(assignment.role, "platform"));
  const selectedId = cookieValue(
    requestHeaders.get("cookie"),
    "erpx_assignment_platform",
  );
  const assignment =
    assignments.find((item) => item.id === selectedId) ?? assignments[0];
  if (!assignment) throw new Error("FORBIDDEN");
  return {
    userId: session.user.id,
    name: session.user.name,
    email: session.user.email,
    assignmentId: assignment.id,
    role: assignment.role,
    scope: { kind: "platform" },
    modules: modulesForRole(assignment.role,assignment.modules),
    assignments,
  };
}

export function publicScope(context: WorkContext | PlatformContext): WorkspaceScope {
  const scope = context.scope;
  if (scope.kind === "platform") return scope;
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

export async function firstAuthorizedWorkspace(
  holdingSlug: string,
): Promise<HoldingWorkspaceScope | null> {
  const binding = await holdingBindingBySlug(holdingSlug);
  const { write } = await holdingResources(binding.id);
  const nodes = await write
    .prepare(
      `SELECT n.id,n.kind,n.slug,n.branch_id,b.slug AS branch_slug
       FROM organization_nodes n
       LEFT JOIN organization_nodes b ON n.branch_id=b.id AND b.kind='branch'
       WHERE n.kind IN ('branch','department') AND n.status='active'
       ORDER BY CASE n.kind WHEN 'branch' THEN 1 ELSE 2 END,n.display_name,n.id`,
    )
    .all<{
      id: string;
      kind: "branch" | "department";
      slug: string;
      branch_id: string | null;
      branch_slug: string | null;
    }>();
  const candidates: HoldingWorkspaceScope[] = [
    { kind: "holding", holdingSlug },
    ...nodes.results.map((node): HoldingWorkspaceScope =>
      node.kind === "branch"
        ? { kind: "branch", holdingSlug, branchSlug: node.slug }
        : {
            kind: "department",
            holdingSlug,
            branchSlug: node.branch_slug ?? undefined,
            departmentSlug: node.slug,
          },
    ),
  ];
  for (const candidate of candidates) {
    try {
      await requireWorkContext(candidate);
      return candidate;
    } catch {
      // Continue to the next active workspace candidate.
    }
  }
  return null;
}
