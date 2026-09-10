import { requireWorkContext } from "@/server/authorization";
import { getWorkSummary } from "@/server/work-summary";
import { errorResponse } from "@/server/http";
export async function GET(
  _request: Request,
  context: { params: Promise<{ holdingSlug: string }> },
) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug),
      work = await getWorkSummary(actor);
    return Response.json(
      { data: work },
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
