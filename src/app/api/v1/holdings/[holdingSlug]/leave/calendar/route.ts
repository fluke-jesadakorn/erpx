import { requireWorkContext } from "@/server/authorization";
import { ApiError, errorResponse } from "@/server/http";
import { leaveCalendarMonth } from "@/server/leave-self-service";

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
      { data: await leaveCalendarMonth(actor, month) },
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (error) {
    return errorResponse(error);
  }
}
