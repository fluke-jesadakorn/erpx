import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {staffHandler} from "@erpx/server-work_projection/public/staff-api";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {
  nativeRoutineTemplateDetail,
  nativeRoutineTemplateList,
  nativeRoutineTemplatesAvailable,
} from "@erpx/feature-operations/public/native-routines-read";
import {
  nativeRoutineCommand,
  nativeRoutineRun,
  nativeRoutinesAvailable,
} from "@erpx/server-operations/public/native-routines-page";

export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
const headers = {"Cache-Control": "private, no-store"};
const retained = staffHandler("routine-templates");

/**
 * The native Operations owner is authoritative for routine template
 * create/update/clone/run, schedule create/update and both template reads.
 * A configured native owner that cannot answer must fail closed; there is no
 * retained command or read fallback. The retained handler answers only the
 * subpaths the native owner does not serve.
 */
function nativePath(p: string[]): string | null {
  if (p.length === 0) return "";
  if (p.length === 1) return `/${encodeURIComponent(p[0])}`;
  if (p.length === 2 && p[1] === "clone")
    return `/${encodeURIComponent(p[0])}/clone`;
  if (p.length === 2 && p[1] === "run")
    return `/${encodeURIComponent(p[0])}/run`;
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

function readUnavailable() {
  return errorResponse(
    new ApiError(503, "NATIVE_ROUTINES_UNAVAILABLE", "The native Operations routine read is unavailable."),
  );
}

export async function GET(request: Request, context: Context) {
  const { holdingSlug, path = [] } = await context.params;
  if (path.length > 1) return retained(request, context);
  if (!nativeRoutineTemplatesAvailable()) return readUnavailable();
  try {
    const actor = await requireWorkContext(holdingSlug);
    if (path.length === 1) {
      const data = await nativeRoutineTemplateDetail(request, holdingSlug, path[0], actor.userId);
      if (data === null)
        return errorResponse(new ApiError(404, "TEMPLATE_NOT_FOUND", "Routine template not found."));
      return Response.json(canonicalDestinationLinks({ data }), { headers });
    }
    const data = await nativeRoutineTemplateList(request, holdingSlug, actor.userId);
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (error) {
    return errorResponse(error);
  }
}

export async function POST(request: Request, context: Context) {
  const { holdingSlug, path = [] } = await context.params;
  const target = nativePath(path);
  if (target === null) return retained(request, context);
  if (!nativeRoutinesAvailable()) return unavailable();
  try {
    const key = request.headers.get("Idempotency-Key") ?? "";
    const body = await request.json();
    const data =
      path.length === 2 && path[1] === "run"
        ? await nativeRoutineRun(request, holdingSlug, path[0], body, key)
        : await nativeRoutineCommand(
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
