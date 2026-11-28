import type { RouteContext } from "@erpx/platform-contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { ApiError, errorResponse } from "@erpx/platform-runtime-server/public/http";
import { createStorySchema, listStories } from "@erpx/collaboration-server/public/social";
import { nativeCreateStory } from "@erpx/collaboration-server/public/native-social-page";

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
    await requireWorkContext(holdingSlug);
    return Response.json(canonicalDestinationLinks({ data: await nativeCreateStory(request, holdingSlug, parsed.data, commandId) }), { status: 201 });
  } catch (error) { return errorResponse(error); }
}
