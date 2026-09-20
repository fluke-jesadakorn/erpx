import { requireWorkContext } from "@/server/authorization";
import { errorResponse } from "@/server/http";
import { leaveSummary } from "@/server/leave-self-service";

export const dynamic = "force-dynamic";

export async function GET(_request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/leave/summary">) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    return Response.json({ data: await leaveSummary(actor) });
  } catch (error) {
    return errorResponse(error);
  }
}
