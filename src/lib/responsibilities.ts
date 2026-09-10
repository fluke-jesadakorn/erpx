import type { WorkAssignment, WorkContext } from "@/server/authorization";
import { assignmentApplies } from "./workspace-access";
import { modulesForRole } from "./business-roles";

/** A responsibility is one complete grant. Never combine fragments of grants. */
export function applicableAssignments(
  actor: Pick<WorkContext, "assignments" | "scope">,
): WorkAssignment[] {
  return actor.assignments.filter((a) => assignmentApplies(a, actor.scope));
}
export function responsibilityRoles(actor: WorkContext) {
  return [
    ...new Set([
      actor.role,
      ...applicableAssignments(actor).map((a) => a.role),
    ]),
  ];
}
export function hasBusinessRole(actor: WorkContext, roles: readonly string[]) {
  return responsibilityRoles(actor).some((role) => roles.includes(role));
}
export function responsibilityContext(
  actor: WorkContext,
  roles: readonly string[],
): WorkContext | null {
  // Prefer the narrowest permitted business operation, in the caller's order.
  for (const role of roles) {
    const grant = applicableAssignments(actor).find((a) => a.role === role);
    if (grant)
      return {
        ...actor,
        assignmentId: grant.id,
        role: grant.role,
        assignmentScope: grant.scopeKind,
        inherited: grant.scopeKind !== actor.scope.kind,
        modules: actor.beta ? [] : modulesForRole(grant.role, grant.modules),
      };
    if (actor.role === role) return actor;
  }
  return null;
}
export function responsibilityContexts(actor: WorkContext) {
  const grants = applicableAssignments(actor);
  return grants.length
    ? grants.map((a) => ({
        ...actor,
        assignmentId: a.id,
        role: a.role,
        assignmentScope: a.scopeKind,
        inherited: a.scopeKind !== actor.scope.kind,
        modules: actor.beta ? [] : modulesForRole(a.role, a.modules),
      }))
    : [actor];
}
