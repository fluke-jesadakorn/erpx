import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { nativePostRevisions, nativeSocialAvailable } from "@erpx/server-collaboration/public/native-social-page";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";

export async function GET(
  request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/posts/[id]/revisions">,
) {
  try {
    const { holdingSlug, id } = await context.params;
    await requireWorkContext(holdingSlug);
    if (!nativeSocialAvailable())
      throw new ApiError(503, "NATIVE_SOCIAL_UNAVAILABLE", "The native Social service is unavailable.");
    return Response.json(canonicalDestinationLinks({ data: await nativePostRevisions(request, holdingSlug, id) }),
      { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) {
    return errorResponse(error);
  }
}
