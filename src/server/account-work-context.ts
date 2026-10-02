import "server-only";

import { safeReturnPath } from "@/lib/access-problem";
import type { AccountWorkContext } from "@/lib/account-work-context";
import { parseHoldingWorkspace } from "@/lib/workspace";
import type { CurrentWorkspaceChoice } from "@/lib/workspace-choice";
import {
  assignmentApplies,
  canEnterManageErpx,
  currentSession,
  requireAccountAccess,
  requirePlatformContext,
  type ResolvedWorkspaceScope,
} from "./authorization";
import { ApiError } from "./http";
import { holdingResources } from "./runtime";
import { initializeWorkSelection } from "./work-role-store";
import { workspaceChoices, workspaceChoicesForAssignment } from "./workspace-choices";

function currentChoice(scope: ResolvedWorkspaceScope): CurrentWorkspaceChoice {
  if (scope.kind === "platform") return scope;
  const common = { holdingSlug: scope.holdingSlug, companyId: scope.companyId };
  if (scope.kind === "holding") return { kind: scope.kind, ...common };
  if (scope.kind === "branch") return { kind: scope.kind, ...common, branchId: scope.branchId };
  return { kind: scope.kind, ...common, branchId: scope.branchId, departmentId: scope.departmentId };
}

async function manageErpxAvailable() {
  try { return await canEnterManageErpx(); }
  catch (error) {
    if (error instanceof ApiError && [401, 403].includes(error.status)) return false;
    if (error instanceof Error && error.message === "FORBIDDEN") return false;
    throw error;
  }
}

/** Inspect this page in the new account without choosing a company or destination. */
export async function accountWorkContext(input: { from: string; companyId?: string }): Promise<AccountWorkContext> {
  const from = safeReturnPath(input.from);
  if (!from) throw new ApiError(422, "INVALID_WORKSPACE", "Use the current workspace page.");
  const [{ session }, choices, canManageErpx] = await Promise.all([
    currentSession(), workspaceChoices(), manageErpxAvailable(),
  ]);
  const result: AccountWorkContext = {
    user: { id: session.user.id, name: session.user.name, email: session.user.email },
    from, status: "forbidden", sameWorkspace: false, canUseWorkspace: false,
    requiresWorkspaceSelection: choices.length > 0 || canManageErpx,
    choices, canManageErpx, assignments: [], assignmentId: null, revision: 0, context: null,
  };
  let parts: string[];
  try { parts = new URL(from, "https://erpx.invalid").pathname.split("/").filter(Boolean).map(decodeURIComponent); }
  catch { throw new ApiError(422, "INVALID_WORKSPACE", "Use the current workspace page."); }
  if (parts[0] === "platform") {
    result.current = { kind: "platform" };
    try {
      const actor = await requirePlatformContext();
      return { ...result, status: "ready", sameWorkspace: true, canUseWorkspace: true,
        requiresWorkspaceSelection: false, assignments: actor.assignments, assignmentId: actor.assignmentId,
        context: { userId: actor.userId, companyId: "platform", assignmentId: actor.assignmentId, revision: 0 } };
    } catch (error) {
      if (!(error instanceof Error) || error.message !== "FORBIDDEN") throw error;
      return { ...result, problem: { code: "FORBIDDEN", message: "Your account cannot open this workspace." } };
    }
  }
  const parsed = parseHoldingWorkspace(parts[1], parts.slice(2));
  if (!parsed) throw new ApiError(422, "INVALID_WORKSPACE", "Use the current workspace page.");
  if (input.companyId) result.current = { kind: "holding", holdingSlug: parsed.scope.holdingSlug, companyId: input.companyId };
  try {
    // The explicit company is deliberate inspection evidence. Old revision
    // headers are irrelevant to this GET; account access still verifies the
    // session, assignments and company without asserting an old form's role.
    const access = await requireAccountAccess(parsed.scope, { companyId: input.companyId });
    result.current = currentChoice(access.scope);
    result.assignments = access.assignments;
    const { write } = await holdingResources(access.scope.holdingId);
    const eligible = access.assignments.filter(assignment => assignmentApplies(assignment, access.scope));
    const selection = await initializeWorkSelection(write, {
      sessionId: access.sessionId, userId: access.userId, companyId: access.scope.companyId, expiresAt: access.expiresAt,
    }, access.assignments, eligible);
    result.assignmentId = selection?.assignment_id ?? null;
    result.revision = selection?.revision ?? 0;
    const selected = selection && selection.expires_at > Date.now()
      ? access.assignments.find(assignment => assignment.id === selection.assignment_id && assignment.version === selection.assignment_version)
      : undefined;
    if (selected && eligible.some(assignment => assignment.id === selected.id)) {
      return { ...result, status: "ready", sameWorkspace: true, canUseWorkspace: true, requiresWorkspaceSelection: false,
        context: { userId: access.userId, companyId: access.scope.companyId, assignmentId: selected.id, revision: result.revision } };
    }
    if (!selected && eligible.length) {
      return { ...result, status: "role-required", requiresWorkspaceSelection: false,
        problem: { code: "WORK_ROLE_REQUIRED", message: "Choose an available role for this workspace." } };
    }
    if (selected) {
      const scoped = await workspaceChoicesForAssignment(access.scope.holdingId, selected.id);
      const companyChoices = scoped.flatMap(choice => choice.kind === "holding" ? choice.companies : []);
      result.choices = choices.map(choice => choice.kind === "holding" && choice.holdingSlug === access.scope.holdingSlug
        ? { ...choice, companies: choice.companies.flatMap(company => company.id === access.scope.companyId
          ? companyChoices.filter(item => item.id === company.id) : [company]) }
        : choice).filter(choice => choice.kind === "platform" || choice.companies.length > 0);
      result.requiresWorkspaceSelection = result.choices.length > 0 || canManageErpx;
    }
    return { ...result, status: result.requiresWorkspaceSelection ? "workspace-required" : "forbidden",
      problem: { code: "WORK_ROLE_UNAVAILABLE", message: "Your selected role cannot open this workspace. Choose an available workspace or role." } };
  } catch (error) {
    if (error instanceof ApiError && error.status === 401) throw error;
    const code = error instanceof ApiError ? error.code : error instanceof Error ? error.message : "";
    if (!["FORBIDDEN", "COMPANY_REQUIRED", "HOLDING_NOT_FOUND", "WORKSPACE_NOT_FOUND", "INVALID_ORGANIZATION_LINEAGE"].includes(code)) throw error;
    return { ...result, status: result.requiresWorkspaceSelection ? "workspace-required" : "forbidden",
      problem: { code, message: "Your account cannot open the current workspace. Choose an available workspace or account." } };
  }
}
