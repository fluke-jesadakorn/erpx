import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { decideLeaveCancellation, leaveDecisionSchema } from "@erpx/server-people_payroll/public/leave";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";

export async function POST(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/leave-requests/[id]/cancellation-review">) {
  try {
    const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const key = request.headers.get("Idempotency-Key");
    if (!key) throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.");
    const body = await request.json() as { decision?: unknown; expectedVersion?: unknown; note?: unknown };
    const input = leaveDecisionSchema.safeParse(body);
    if (!input.success) throw new ApiError(422, "VALIDATION_FAILED", input.error.issues[0]?.message ?? "Invalid decision.");
    if (body.decision !== "approve" && body.decision !== "reject") throw new ApiError(422, "VALIDATION_FAILED", "Decision must be approve or reject.");
    const { holdingSlug, id } = await context.params, actor = await requireWorkContext(holdingSlug);
    return Response.json(canonicalDestinationLinks({ data: await decideLeaveCancellation(actor, id, input.data, key, body.decision === "approve") }));
  } catch (error) { return errorResponse(error); }
}
