import { requireWorkContext } from "@/server/authorization";
import { getWorkSummary, getWorkPage } from "@/server/work-summary";
import { errorResponse } from "@/server/http";
export async function GET(
  request: Request,
  context: { params: Promise<{ holdingSlug: string }> },
) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug),
      params = new URL(request.url).searchParams,
      work = params.get("paged") === "1" ? await getWorkPage(actor,{
        view:params.get("view")??"actions",queue:params.get("queue")??"todo",type:params.get("type")??"all",
        q:params.get("q")??"",responsibility:params.get("responsibility")??"all",offset:Number(params.get("offset")??0),
      }) : await getWorkSummary(actor);
    return Response.json(
      { data: work },
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
