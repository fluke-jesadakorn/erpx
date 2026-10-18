import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { getWorkSummary, getWorkPage } from "@/server/modules/work_projection/public/work-summary";
import { errorResponse } from "@/server/modules/runtime/public/http";
import { measureReadRequest } from "@/server/modules/runtime/public/runtime/read-request";
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
