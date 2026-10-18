import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { errorResponse } from "@/server/modules/runtime/public/http";
import { postRevisionHistory } from "@/server/modules/collaboration/public/social-home";

export async function GET(
  _request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/posts/[id]/revisions">,
) {
  try {
    const { holdingSlug, id } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const { write } = await holdingResources(actor.holdingId);
    return Response.json(canonicalDestinationLinks({ data: await postRevisionHistory(write, actor, id) }),
      { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) {
    return errorResponse(error);
  }
}
