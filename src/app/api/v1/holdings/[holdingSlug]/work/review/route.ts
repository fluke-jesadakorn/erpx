import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { getWorkReview } from "@/server/modules/work_projection/public/work-review";
import { workReviewKinds, type WorkReviewKind } from "@/lib/work_projection/work-review";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";

export async function GET(request: Request, context: { params: Promise<{ holdingSlug: string }> }) {
  try {
    const query = new URL(request.url).searchParams, kind = query.get("kind"), id = query.get("id");
    if (!id || id.length > 180 || !workReviewKinds.includes(kind as WorkReviewKind)) throw new ApiError(422, "REVIEW_SOURCE_INVALID", "Choose a supported source record.");
    const actor = await requireWorkContext((await context.params).holdingSlug);
    return Response.json({ data: await getWorkReview(actor, kind as WorkReviewKind, id) }, { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
