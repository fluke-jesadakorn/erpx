import type { RouteContext } from "@erpx/platform-contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { errorResponse } from "@erpx/platform-runtime-server/public/http";
import { holdingPeopleDirectory } from "@erpx/organization-server/public/holding-people-directory";
import { measureReadRequest } from "@erpx/platform-runtime-server/public/runtime/read-request";

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
