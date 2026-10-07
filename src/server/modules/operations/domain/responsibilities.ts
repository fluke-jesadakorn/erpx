// Database-only domain code shared by the Next.js server and jobs Worker.
import {hasPermission,permissionForRoles} from "@/lib/permissions";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import {BusinessRuleError} from "../../shared";

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
