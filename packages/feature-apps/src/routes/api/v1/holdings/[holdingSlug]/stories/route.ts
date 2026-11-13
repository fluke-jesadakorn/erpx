import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { createStorySchema, listStories } from "@erpx/server-collaboration/public/social";
import { nativeCreateStory, nativeSocialAvailable } from "@erpx/server-collaboration/public/native-social-page";

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
    if (!nativeSocialAvailable())
      throw new ApiError(
        503,
        "NATIVE_SOCIAL_UNAVAILABLE",
        "The native Social service is unavailable.",
      );
    return Response.json(canonicalDestinationLinks({ data: await nativeCreateStory(request, holdingSlug, parsed.data, commandId) }), { status: 201 });
  } catch (error) { return errorResponse(error); }
}
