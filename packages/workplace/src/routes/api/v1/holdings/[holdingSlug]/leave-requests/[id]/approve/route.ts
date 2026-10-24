import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import {approveLeave,leaveDecisionSchema} from '@erpx/server-people_payroll/public/leave';
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";

export async function POST(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/leave-requests/[id]/approve">) {
  try {
    const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const { holdingSlug, id } = await context.params;
    const commandId = request.headers.get("Idempotency-Key");
    if (!commandId) throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.");
    const parsed = leaveDecisionSchema.safeParse(await request.json());
    if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid request.");
    const workContext = await requireWorkContext(holdingSlug);
    return Response.json(canonicalDestinationLinks({ data: await approveLeave(workContext.holdingId, id, parsed.data, commandId, workContext) }));
  } catch (error) {
    return errorResponse(error);
  }
}
