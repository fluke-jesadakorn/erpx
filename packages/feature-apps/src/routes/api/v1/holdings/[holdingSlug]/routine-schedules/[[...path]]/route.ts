import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {staffHandler} from "@erpx/server-work_projection/public/staff-api";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {
  nativeRoutineCommand,
  nativeRoutinesAvailable,
} from "@erpx/server-operations/public/native-routines-page";

export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
const headers = {"Cache-Control": "private, no-store"};
const retained = staffHandler("routine-schedules");

/** Native Operations owns schedule pause/resume/end; the retained handler keeps
 * every other routine-schedule shape. A configured native owner that cannot
 * answer fails closed instead of writing through the removed TypeScript writers. */
function nativePath(p: string[]): string | null {
  if (p.length === 2 && ["pause", "resume", "end"].includes(p[1]))
    return `/${encodeURIComponent(p[0])}/${p[1]}`;
  return null;
}

function unavailable() {
  return errorResponse(
    new ApiError(503, "NATIVE_ROUTINES_UNAVAILABLE", "The native Operations routine command is unavailable."),
  );
}

export async function GET(request: Request, context: Context) {
  return retained(request, context);
}

export async function POST(request: Request, context: Context) {
  const { holdingSlug, path = [] } = await context.params;
  const target = nativePath(path);
  if (target === null) return retained(request, context);
  if (!nativeRoutinesAvailable()) return unavailable();
  try {
    const key = request.headers.get("Idempotency-Key") ?? "";
    const body = await request.json();
    const data = await nativeRoutineCommand(
      request,
      holdingSlug,
      "routine-schedules",
      target,
      body,
      key,
    );
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (error) {
    return errorResponse(error);
  }
}
