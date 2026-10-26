import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { archiveStory } from "@erpx/server-collaboration/public/social";
import { z } from "zod";

const schema = z.object({ expectedVersion: z.number().int().positive() });

export async function POST(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/stories/[id]/archive">) {
  try {
    const { holdingSlug, id } = await context.params;
    const commandId = request.headers.get("Idempotency-Key");
    if (!commandId) throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.");
    const parsed = schema.safeParse(await request.json());
    if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", "The current story version is required.");
    const actor = await requireWorkContext(holdingSlug);
    return Response.json(canonicalDestinationLinks({ data: await archiveStory(actor.holdingId, id, parsed.data.expectedVersion, commandId, actor) }));
  } catch (error) { return errorResponse(error); }
}
