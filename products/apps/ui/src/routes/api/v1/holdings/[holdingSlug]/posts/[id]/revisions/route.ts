import type { RouteContext } from "@erpx/platform-contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { nativePostRevisions, nativeSocialAvailable } from "@erpx/collaboration-server/public/native-social-page";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { ApiError, errorResponse } from "@erpx/platform-runtime-server/public/http";

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
