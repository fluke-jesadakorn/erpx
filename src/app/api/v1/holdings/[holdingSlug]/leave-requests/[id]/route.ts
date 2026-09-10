import { requireWorkContext } from "@/server/authorization";
import { getLeave, leaveDraftSchema, submitLeave } from "@/server/leave";
import { ApiError, errorResponse } from "@/server/http";

export const dynamic = "force-dynamic";

export async function GET(_request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/leave-requests/[id]">) {
  try {
    const { holdingSlug, id } = await context.params;
    const workContext = await requireWorkContext(holdingSlug);
    return Response.json({ data: await getLeave(workContext.holdingId, id, workContext) });
  } catch (error) {
    return errorResponse(error);
  }
}

export async function PATCH(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/leave-requests/[id]">) {
  try {
    const { holdingSlug, id } = await context.params;
    const commandId = request.headers.get("Idempotency-Key");
    if (!commandId) throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.");
    const parsed = leaveDraftSchema.safeParse(await request.json());
    if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid request.");
    const workContext = await requireWorkContext(holdingSlug);
    return Response.json({ data: await submitLeave(workContext.holdingId, id, parsed.data, commandId, workContext) });
  } catch (error) {
    return errorResponse(error);
  }
}
