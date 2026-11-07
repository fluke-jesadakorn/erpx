import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { holdingResources } from "../../../../../../../../../../../packages/server-runtime/src/public/runtime";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { errorResponse } from "@erpx/server-runtime/public/http";
import {postRevisionHistory} from '@erpx/server-collaboration/public/social-home';

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
