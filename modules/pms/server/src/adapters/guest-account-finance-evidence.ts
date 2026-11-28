import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { nativeLane19GuestAccountEvidence } from "./native-lane-19/client";

/**
 * Financial evidence for one guest account. The two owner-local reads, the
 * Company/Branch scope and the legacy terminology projection are owned by the
 * mounted native lane 19 axum router; this adapter only asserts the caller's
 * permissions and transports the bounded identity. No SQL fallback remains.
 */
export async function guestAccountFinanceEvidence(actor: WorkContext, id: string) {
  requirePermission(actor, "finance.read");
  requirePermission(actor, "pms.reports");
  if (actor.departmentId) throw new ApiError(403, "PMS_BRANCH_REQUIRED", "Choose a Company or Branch.");
  return nativeLane19GuestAccountEvidence(actor, id);
}
