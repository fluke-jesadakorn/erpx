import type { RouteContext } from "@erpx/platform-contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { errorResponse } from "@erpx/platform-runtime-server/public/http";
import { leaveSummary } from "@erpx/people-payroll-server/public/leave-self-service";

export const dynamic = "force-dynamic";

export async function GET(_request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/leave/summary">) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    return Response.json(canonicalDestinationLinks({ data: await leaveSummary(actor) }));
  } catch (error) {
    return errorResponse(error);
  }
}
