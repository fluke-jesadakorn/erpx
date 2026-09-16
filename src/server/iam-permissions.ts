import { hasPermission } from "@/lib/permissions";
import type { PermissionActor } from "@/lib/iam";
import { ApiError } from "./http";
export function requirePermission<T extends PermissionActor>(
  actor: T,
  permission: string,
): T {
  if (!hasPermission(actor, permission))
    throw new ApiError(
      403,
      "PERMISSION_REQUIRED",
      "Your selected role does not grant this permission.",
    );
  return actor;
}
