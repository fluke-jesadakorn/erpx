import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { setActiveSchema, setAppreciation } from "@erpx/server-collaboration/public/social";

export async function PUT(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/posts/[id]/reaction">) {
  try {
    const { holdingSlug, id } = await context.params;
    const commandId = request.headers.get("Idempotency-Key");
    if (!commandId) throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.");
    const parsed = setActiveSchema.safeParse(await request.json());
    if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid appreciation state.");
    const actor = await requireWorkContext(holdingSlug);
    return Response.json(canonicalDestinationLinks({ data: await setAppreciation(actor.holdingId, id, parsed.data.active, commandId, actor) }));
  } catch (error) { return errorResponse(error); }
}
