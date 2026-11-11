import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {staffHandler} from "@erpx/server-work_projection/public/staff-api";
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
 * The retained handler still owns levels, templates, schedules, milestones,
 * notifications, approvals reroute and appointments. Only the task workspace,
 * task detail and task commands switch to the native owner.
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

export async function GET(request: Request, context: Context) {
  const { holdingSlug, path = [] } = await context.params;
  if (nativeStaffWorkAvailable()) {
    const target = nativePath(path);
    if (target !== null) {
      const projectId = new URL(request.url).searchParams.get("projectId");
      const data = await nativeStaffWorkRead(request, holdingSlug, target, projectId);
      return Response.json(canonicalDestinationLinks({ data }), { headers });
    }
  }
  return retained(request, context);
}

export async function POST(request: Request, context: Context) {
  const { holdingSlug, path = [] } = await context.params;
  if (nativeStaffWorkAvailable()) {
    const target = nativePath(path);
    if (target !== null) {
      const key = request.headers.get("Idempotency-Key") ?? "";
      const body = await request.json();
      const data = await nativeStaffWorkCommand(request, holdingSlug, target, body, key);
      return Response.json(canonicalDestinationLinks({ data }), { headers });
    }
  }
  return retained(request, context);
}
