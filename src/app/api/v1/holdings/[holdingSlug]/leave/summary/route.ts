import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { errorResponse } from "@/server/modules/runtime/public/http";
import { leaveSummary } from "@/server/modules/people_payroll/public/leave-self-service";

export const dynamic = "force-dynamic";

export async function GET(_request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/leave/summary">) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    return Response.json(canonicalDestinationLinks({ data: await leaveSummary(actor) }));
  } catch (error) {
    return errorResponse(error);
  }
}
