import { hasPermission } from "@/lib/permissions";
import type { PermissionActor } from "@/lib/iam";
import { ApiError } from "./http";
import { forbidden } from "next/navigation";
export function requirePermission<T extends PermissionActor>(
  actor: T,
  permission: string,
): T {
  if (!hasPermission(actor, permission)) {
    if ((actor as PermissionActor & { pageRequest?: true }).pageRequest) forbidden();
    throw new ApiError(
      403,
      "PERMISSION_REQUIRED",
      "Your selected role does not grant this permission.",
    );
  }
  return actor;
}
