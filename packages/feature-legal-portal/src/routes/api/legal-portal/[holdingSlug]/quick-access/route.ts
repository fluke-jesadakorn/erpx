import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { z } from "zod";
import { boundedBody } from "@erpx/server-legal/public/legal/body";
import {legalPortalQuickAccessRequest} from '@erpx/server-legal/public/portal-quick-access-request';
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";

export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
const inputSchema = z.object({
  pinnedIds: z.array(z.string().min(1).max(240)).max(6).nullable(),
  expectedVersion: z.number().int().nonnegative(),
}).strict();

export async function GET(_request: Request, context: Context) {
  try {
    const { holdingSlug } = await context.params;
    const preference = await legalPortalQuickAccessRequest(holdingSlug);
    return Response.json(canonicalDestinationLinks({ data: preference }), { headers });
  } catch (error) {
    return errorResponse(error);
  }
}

export async function POST(request: Request, context: Context) {
  try {
    if (request.headers.get("origin") !== requestPublicOrigin(request))
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const requestKey = request.headers.get("Idempotency-Key");
    if (!requestKey) throw new ApiError(422, "COMMAND_KEY_REQUIRED", "A command key is required.");
    let raw: unknown;
    try {
      raw = JSON.parse(new TextDecoder().decode(await boundedBody(request, 4096)));
    } catch (error) {
      if (error instanceof SyntaxError) throw new ApiError(422, "INVALID_JSON", "Invalid JSON.");
      throw error;
    }
    const { pinnedIds, expectedVersion } = inputSchema.parse(raw);
    const { holdingSlug } = await context.params;
    const preference = await legalPortalQuickAccessRequest(holdingSlug,{pinnedIds,expectedVersion,requestKey});
    return Response.json(canonicalDestinationLinks({ data: preference }), { headers });
  } catch (error) {
    return errorResponse(error);
  }
}
