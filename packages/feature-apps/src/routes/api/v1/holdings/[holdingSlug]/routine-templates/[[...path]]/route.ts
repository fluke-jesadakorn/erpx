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
const retained = staffHandler("routine-templates");

/**
 * The native Operations owner is authoritative for routine template
 * create/update/clone and schedule create/update. A configured native owner
 * that cannot answer must fail closed; there is no retained command fallback.
 * The retained handler answers only the shapes the native owner does not serve:
 * template reads and `/{id}/run`.
 */
function nativePath(p: string[]): string | null {
  if (p.length === 0) return "";
  if (p.length === 1) return `/${encodeURIComponent(p[0])}`;
  if (p.length === 2 && p[1] === "clone")
    return `/${encodeURIComponent(p[0])}/clone`;
  if (p.length === 2 && p[1] === "schedules")
    return `/${encodeURIComponent(p[0])}/schedules`;
  if (p.length === 3 && p[1] === "schedules")
    return `/${encodeURIComponent(p[0])}/schedules/${encodeURIComponent(p[2])}`;
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
      "routine-templates",
      target,
      body,
      key,
    );
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (error) {
    return errorResponse(error);
  }
}
