import { requireWorkContext } from "@/server/authorization";
import { errorResponse } from "@/server/http";
import { holdingPeopleDirectory } from "@/server/holding-people-directory";
import { measureReadRequest } from "@/server/runtime/read-request";

export async function GET(
  _request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/people-public-directory">,
) {
  return measureReadRequest("people", async () => {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    return Response.json({ data: await holdingPeopleDirectory(actor) }, {
      headers: { "Cache-Control": "private, no-store" },
    });
  } catch (error) {
    return errorResponse(error);
  }
  });
}
