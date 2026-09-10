import { requireWorkContext } from "@/server/authorization";
import { ApiError, errorResponse } from "@/server/http";
import { setActiveSchema, setFollow } from "@/server/social";

export async function PUT(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/posts/[id]/follow">) {
  try {
    const { holdingSlug, id } = await context.params;
    const commandId = request.headers.get("Idempotency-Key");
    if (!commandId) throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.");
    const parsed = setActiveSchema.safeParse(await request.json());
    if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "Invalid follow state.");
    const actor = await requireWorkContext(holdingSlug);
    return Response.json({ data: await setFollow(actor.holdingId, id, parsed.data.active, commandId, actor) });
  } catch (error) { return errorResponse(error); }
}
