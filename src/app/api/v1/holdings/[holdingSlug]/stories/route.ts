import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { createStory, createStorySchema, listStories } from "@/server/modules/collaboration/public/social";

export const dynamic = "force-dynamic";

export async function GET(_request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/stories">) {
  try {
    const { holdingSlug } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    return Response.json(canonicalDestinationLinks({ data: await listStories(actor.holdingId, actor) }));
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
    return Response.json(canonicalDestinationLinks({ data: await createStory(actor.holdingId, parsed.data, commandId, actor) }), { status: 201 });
  } catch (error) { return errorResponse(error); }
}
