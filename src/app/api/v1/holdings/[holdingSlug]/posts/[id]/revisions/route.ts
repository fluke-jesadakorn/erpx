import { holdingResources } from "@/server/runtime";
import { requireWorkContext } from "@/server/authorization";
import { errorResponse } from "@/server/http";
import { postRevisionHistory } from "@/server/social-home";

export async function GET(
  _request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/posts/[id]/revisions">,
) {
  try {
    const { holdingSlug, id } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const { write } = await holdingResources(actor.holdingId);
    return Response.json({ data: await postRevisionHistory(write, actor, id) },
      { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) {
    return errorResponse(error);
  }
}
