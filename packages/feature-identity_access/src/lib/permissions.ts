import metadata from "@erpx/contracts/permission-catalog";
import type { AccessDecision, PermissionActor, PermissionDefinition } from "@erpx/feature-identity_access/lib/iam";

/** Presentation/legacy projection of generated metadata. Native owner authorization repeats current policy. */
export const permissionCatalog: readonly PermissionDefinition[] = metadata as readonly PermissionDefinition[];
export function permissionDefinition(id: string) {
  return permissionCatalog.find((p) => p.id === id);
}
export function builtInPermissions(role: string): string[] {
  return permissionCatalog
    .filter((p) => p.roles.includes(role))
    .map((p) => p.id);
}
export function hasPermission(
  actor: PermissionActor,
  permission: string,
): boolean {
  const definition = permissionDefinition(permission);
  if (!definition || definition.availability !== "live") return false;
  const scope = actor.scope?.kind ?? actor.assignmentScope;
  if (
    scope &&
    definition.permittedScopes &&
    !definition.permittedScopes.includes(scope)
  )
    return false;
  if (definition.identityBased) return !!actor.userId;
  const permissions = actor.iam?.permissions ?? actor.permissions;
  return permissions
    ? permissions.includes(permission)
    : definition.roles.includes(actor.role);
}
export function evaluateAccess(
  actor: PermissionActor,
  permission: string,
  options: {
    inspect?: boolean;
    valid?: boolean;
    reason?: string;
    expiresAt?: number;
    source?: string;
  } = {},
): AccessDecision {
  const p = permissionDefinition(permission),
    allowed = hasPermission(actor, permission);
  const result =
    !p || p.availability !== "live"
      ? "unavailable"
      : options.valid === false ||
          (!allowed && (!p.identityBased || !options.inspect))
        ? "denied"
        : options.inspect && (p.conditions.length || p.identityBased)
          ? "conditional"
          : "allowed";
  return {
    result,
    permission,
    role: actor.role,
    revision: actor.iam?.roleRevision ?? actor.policyRevision ?? 1,
    assignmentId: actor.assignmentId,
    scope: actor.assignmentScope,
    source: options.source,
    expiresAt: options.expiresAt,
    reasons: [
      options.reason ??
        (p?.identityBased
          ? "Identity-based participation, evaluated independently of the selected role."
          : !p
            ? "This capability is unavailable."
            : options.valid === false
              ? "The assignment is not active in this scope."
              : allowed
                ? "Granted by the selected role."
                : p.permittedScopes &&
                    (actor.scope?.kind ?? actor.assignmentScope) &&
                    !p.permittedScopes.includes(
                      (actor.scope?.kind ?? actor.assignmentScope)!,
                    )
                  ? "This action is unavailable in the selected workspace scope."
                  : "The selected role does not grant this permission."),
    ],
    conditions: allowed || p?.identityBased ? [...(p?.conditions ?? [])] : [],
  };
}

// Temporary adapters for existing callers. The result is always a catalog key;
// an unknown policy denies access instead of falling back to role-name checks.
export function permissionForRoles(roles: readonly string[]): string {
  const key = [...new Set(roles)].sort().join(",");
  return (
    permissionCatalog.find(
      (p) =>
        p.namespace === "customer" &&
        [...new Set(p.roles)].sort().join(",") === key,
    )?.id ?? "unavailable"
  );
}
export function workflowPermission(role: string) {
  return `workflow.${role}`;
}
export function hasWorkflowPermission(actor: PermissionActor, role: string) {
  return hasPermission(actor, workflowPermission(role));
}
