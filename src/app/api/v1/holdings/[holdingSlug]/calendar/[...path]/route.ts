import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { calendarHierarchy, calendarData } from "@/server/modules/work_projection/public/calendar/index";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { publishedAnnouncements } from "@/server/modules/work_projection/public/calendar/announcements";
import { today } from "@/lib/work_projection/calendar";
import { POST as command } from "../route";
import { calendarAvailability } from "@/server/modules/work_projection/public/calendar/availability";
export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string; path: string[] }> };
export async function GET(request: Request, context: Context) {
  try {
    const { holdingSlug, path } = await context.params,
      actor = await requireWorkContext(holdingSlug),
      { write } = await holdingResources(actor.holdingId),
      q = new URL(request.url).searchParams;
    const data = path.length===1&&path[0]==="availability"
      ? await calendarAvailability(write,actor,q.getAll("userId"),Number(q.get("from")),Number(q.get("to")))
      :
      path.length === 1 &&
      path[0] === "announcements" &&
      q.get("published") === "true"
        ? {
            items: await publishedAnnouncements(write, actor, today()),
            sources: [{ source: "announcements", status: "ready" }],
          }
        : path.length === 1 && path[0] === "hierarchy"
          ? await calendarHierarchy(write, actor, {
              cursor: Number(q.get("cursor") ?? 0),
              search: q.get("search") ?? "",
              limit: Number(q.get("limit") ?? 200),
            })
          : path.length === 1 &&
              ["appointments", "announcements"].includes(path[0])
            ? await calendarData(write, actor, {
                ...Object.fromEntries(q),
                layers: path[0],
              })
            : null;
    if (!data)
      throw new ApiError(404, "NOT_FOUND", "Calendar resource not found.");
    return Response.json(
      canonicalDestinationLinks({ data }),
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
export async function POST(request: Request, context: Context) {
  const { path } = await context.params;
  if (
    path.length !== 1 ||
    !["appointments", "announcements", "tasks", "shifts"].includes(path[0])
  )
    return errorResponse(
      new ApiError(404, "NOT_FOUND", "Calendar resource not found."),
    );
  return command(request, context);
}
