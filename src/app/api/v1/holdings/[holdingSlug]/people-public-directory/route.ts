import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { errorResponse } from "@/server/modules/runtime/public/http";
import { holdingPeopleDirectory } from "@/server/modules/organization/public/holding-people-directory";
import { measureReadRequest } from "@/server/modules/runtime/public/runtime/read-request";

export async function GET(
  _request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/people-public-directory">,
) {
  return measureReadRequest("people", async () => {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    return Response.json(canonicalDestinationLinks({ data: await holdingPeopleDirectory(actor) }), {
      headers: { "Cache-Control": "private, no-store" },
    });
  } catch (error) {
    return errorResponse(error);
  }
  });
}
