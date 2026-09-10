// Database-only domain code shared by the Next.js server and jobs Worker.
import { responsibilityContext } from "@/lib/responsibilities";
import type { WorkContext } from "./authorization";
import { ApiError } from "./http";

export function requireBusinessRole(
  actor: WorkContext,
  roles: readonly string[],
) {
  const selected = responsibilityContext(actor, roles);
  if (!selected)
    throw new ApiError(
      403,
      "BUSINESS_ROLE_REQUIRED",
      "An active assignment for this action is required in the current workspace.",
    );
  return selected;
}
