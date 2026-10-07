import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { calendarData, calendarCommand } from "@/server/modules/work_projection/public/calendar/index";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
export async function GET(request: Request, context: Context) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug),
      { write } = await holdingResources(actor.holdingId);
    return Response.json(
      {
        data: await calendarData(
          write,
          actor,
          Object.fromEntries(new URL(request.url).searchParams),
        ),
      },
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
export async function POST(request: Request, context: Context) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
      throw new ApiError(
        403,
        "ORIGIN_FORBIDDEN",
        "Use the application origin for this action.",
      );
    const key = request.headers.get("Idempotency-Key");
    if (!key)
      throw new ApiError(
        422,
        "IDEMPOTENCY_KEY_REQUIRED",
        "Idempotency-Key is required.",
      );
    const actor = await requireWorkContext((await context.params).holdingSlug),
      { write } = await holdingResources(actor.holdingId);
    let input: unknown;
    try {
      input = await request.json();
    } catch {
      throw new ApiError(
        422,
        "INVALID_JSON",
        "Request body must be valid JSON.",
      );
    }
    const resource = (await context.params).path?.[0];
    const prefix =
      resource === "appointments"
        ? "appointment."
        : resource === "announcements"
          ? "announcement."
          : resource === "tasks"
            ? "task."
            : resource === "shifts"
              ? "shift."
            : null;
    if (
      prefix &&
      (!input ||
        typeof input !== "object" ||
        !("action" in input) ||
        typeof input.action !== "string" ||
        !input.action.startsWith(prefix))
    )
      throw new ApiError(
        422,
        "INVALID_CALENDAR_ACTION",
        "Use an action for this calendar resource.",
      );
    return Response.json({
      data: await calendarCommand(write as PostgresDatabase, actor, input, key),
    });
  } catch (e) {
    return errorResponse(e);
  }
}
