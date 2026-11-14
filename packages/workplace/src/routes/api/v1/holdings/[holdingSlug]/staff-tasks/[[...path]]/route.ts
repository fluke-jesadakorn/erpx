import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {staffHandler} from "@erpx/server-work_projection/public/staff-api";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {
  nativeStaffWorkAvailable,
  nativeStaffWorkCommand,
  nativeStaffWorkRead,
} from "@erpx/server-operations/public/native-staff-work-page";

export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
const headers = {"Cache-Control": "private, no-store"};
const retained = staffHandler("staff-tasks");

/**
 * The native Operations owner is authoritative for the task workspace, task
 * detail and the revise/transition/delegate/revoke commands. A configured
 * native owner that cannot answer must fail closed; there is no retained task
 * command fallback. The retained handler answers only the path shapes the
 * native owner does not serve.
 */
function nativePath(p: string[]): string | null {
  if (p.length === 0) return "";
  if (p.length === 1) return `/${encodeURIComponent(p[0])}`;
  if (p.length === 2 && ["revise", "transition", "delegate"].includes(p[1]))
    return `/${encodeURIComponent(p[0])}/${p[1]}`;
  if (p.length === 4 && p[1] === "delegate" && p[2] === "revoke")
    return `/${encodeURIComponent(p[0])}/delegate/revoke/${encodeURIComponent(p[3])}`;
  return null;
}

function unavailable() {
  return errorResponse(
    new ApiError(503, "NATIVE_STAFF_WORK_UNAVAILABLE", "The native Task owner is unavailable."),
  );
}

export async function GET(request: Request, context: Context) {
  const { holdingSlug, path = [] } = await context.params;
  const target = nativePath(path);
  if (target !== null) {
    if (!nativeStaffWorkAvailable()) return unavailable();
    const projectId = new URL(request.url).searchParams.get("projectId");
    const data = await nativeStaffWorkRead(request, holdingSlug, target, projectId);
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  }
  return retained(request, context);
}

export async function POST(request: Request, context: Context) {
  const { holdingSlug, path = [] } = await context.params;
  const target = nativePath(path);
  if (target !== null) {
    if (!nativeStaffWorkAvailable()) return unavailable();
    const key = request.headers.get("Idempotency-Key") ?? "";
    const body = await request.json();
    const data = await nativeStaffWorkCommand(request, holdingSlug, target, body, key);
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  }
  return retained(request, context);
}
