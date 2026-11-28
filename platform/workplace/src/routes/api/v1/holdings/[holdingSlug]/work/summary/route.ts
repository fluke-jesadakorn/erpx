import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { getWorkSummary, getWorkPage } from "@erpx/work-projection-server/public/work-summary";
import { errorResponse } from "@erpx/platform-runtime-server/public/http";
import { measureReadRequest } from "@erpx/platform-runtime-server/public/runtime/read-request";
export async function GET(
  request: Request,
  context: { params: Promise<{ holdingSlug: string }> },
) {
  return measureReadRequest("work", async () => {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug),
      params = new URL(request.url).searchParams,
      work = params.get("paged") === "1" ? await getWorkPage(actor,{
        view:params.get("view")??"actions",queue:params.get("queue")??"todo",type:params.get("type")??"all",
        q:params.get("q")??"",responsibility:params.get("responsibility")??"all",offset:Number(params.get("offset")??0),
        tracking:params.get("tracking")??"all",
      }) : await getWorkSummary(actor);
    return Response.json(
      canonicalDestinationLinks({ data: work }),
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
  });
}
