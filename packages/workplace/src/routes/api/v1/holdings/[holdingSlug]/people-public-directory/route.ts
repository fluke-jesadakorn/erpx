import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { errorResponse } from "@erpx/server-runtime/public/http";
import { holdingPeopleDirectory } from "@erpx/server-organization/public/holding-people-directory";
import { measureReadRequest } from "@erpx/server-runtime/public/runtime/read-request";

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
