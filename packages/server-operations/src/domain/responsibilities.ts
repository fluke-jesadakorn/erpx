// Database-only domain code shared by the Next.js server and jobs Worker.
import {hasPermission,permissionForRoles} from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {BusinessRuleError} from "@erpx/server-shared/index";

export function requireBusinessRole(
  actor: WorkContext,
  roles: readonly string[],
) {
  const selected = hasPermission(actor,permissionForRoles(roles))?actor:null;
  if (!selected)
    throw new BusinessRuleError(
      403,
      "BUSINESS_ROLE_REQUIRED",
      "An active assignment for this action is required in the current workspace.",
    );
  return selected;
}
