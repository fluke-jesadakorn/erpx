import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { createPost, createPostSchema, feedFilterSchema, listFeed } from "@erpx/server-collaboration/public/social";

export const dynamic = "force-dynamic";

export async function GET(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/feed">) {
  try {
    const { holdingSlug } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const requested = new URL(request.url).searchParams.get("filter") ?? "all";
    const filter = feedFilterSchema.safeParse(requested);
    if (!filter.success) throw new ApiError(422, "INVALID_FILTER", "Choose a valid feed filter.");
    return Response.json(canonicalDestinationLinks({ data: await listFeed(actor.holdingId, actor, filter.data) }));
  } catch (error) { return errorResponse(error); }
}

export async function POST(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/feed">) {
  try {
    const { holdingSlug } = await context.params;
    const commandId = request.headers.get("Idempotency-Key");
    if (!commandId) throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.");
    const parsed = createPostSchema.safeParse(await request.json());
    if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid update.");
    const actor = await requireWorkContext(holdingSlug);
    return Response.json(canonicalDestinationLinks({ data: await createPost(actor.holdingId, parsed.data, commandId, actor) }), { status: 201 });
  } catch (error) { return errorResponse(error); }
}
