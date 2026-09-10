import { requireWorkContext } from "@/server/authorization";
import { ApiError, errorResponse } from "@/server/http";
import { createStory, createStorySchema, listStories } from "@/server/social";

export const dynamic = "force-dynamic";

export async function GET(_request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/stories">) {
  try {
    const { holdingSlug } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    return Response.json({ data: await listStories(actor.holdingId, actor) });
  } catch (error) { return errorResponse(error); }
}

export async function POST(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/stories">) {
  try {
    const { holdingSlug } = await context.params;
    const commandId = request.headers.get("Idempotency-Key");
    if (!commandId) throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.");
    const parsed = createStorySchema.safeParse(await request.json());
    if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid story.");
    const actor = await requireWorkContext(holdingSlug);
    return Response.json({ data: await createStory(actor.holdingId, parsed.data, commandId, actor) }, { status: 201 });
  } catch (error) { return errorResponse(error); }
}
