import type { RouteContext } from "@erpx/platform-contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { ApiError, errorResponse } from "@erpx/platform-runtime-server/public/http";
import { leaveCalendarMonth } from "@erpx/people-payroll-server/public/leave-self-service";

export const dynamic = "force-dynamic";

export async function GET(
  request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/leave/calendar">,
) {
  try {
    const month = new URL(request.url).searchParams.get("month");
    if (!month)
      throw new ApiError(422, "LEAVE_MONTH_REQUIRED", "Choose a calendar month.");
    const { holdingSlug } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    return Response.json(
      canonicalDestinationLinks({ data: await leaveCalendarMonth(actor, month) }),
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (error) {
    return errorResponse(error);
  }
}
