import type {
  AssignmentScopeKind,
  ResolvedWorkspaceScope,
  WorkAssignment,
} from "@/server/authorization";

export function assignmentApplies(
  assignment: WorkAssignment,
  scope: Exclude<ResolvedWorkspaceScope, { kind: "platform" }>,
) {
  if (assignment.companyId !== scope.companyId) return false;
  if (assignment.scopeKind === "holding") return true;
  if (scope.kind === "holding") return false;
  if (assignment.scopeKind === "branch")
    return assignment.scopeNodeId === scope.branchId;
  return (
    scope.kind === "department" && assignment.scopeNodeId === scope.departmentId
  );
}

const businessRoleScopes: Record<string, readonly AssignmentScopeKind[]> = {
  it_admin: ["holding"],
  holding_owner: ["holding"],
  sales_manager: ["holding", "branch", "department"],
  sales_operator: ["holding", "branch", "department"],
  purchasing_manager: ["holding", "branch", "department"],
  purchasing_operator: ["holding", "branch", "department"],
  warehouse_manager: ["holding", "branch"],
  operations_manager: ["holding", "branch"],
  employee: ["holding", "branch", "department"],
  finance_preparer: ["holding", "branch", "department"],
  people_manager: ["holding", "branch", "department"],
  hr_admin: ["holding", "branch", "department"],
  payroll_preparer: ["holding", "branch", "department"],
  payroll_approver: ["holding", "branch", "department"],
  finance_approver: ["holding", "branch", "department"],
  treasury: ["holding", "branch", "department"],
  auditor: ["holding", "branch", "department"],
};

export function isRoleValidAtScope(
  role: string,
  scopeKind: AssignmentScopeKind,
) {
  if (scopeKind === "platform") return role.startsWith("platform_");
  return businessRoleScopes[role]?.includes(scopeKind) ?? false;
}
