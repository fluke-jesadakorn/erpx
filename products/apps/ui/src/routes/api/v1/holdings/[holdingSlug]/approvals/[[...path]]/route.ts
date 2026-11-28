import { staffHandler } from "@erpx/work-projection-server/public/staff-api";
import { canonicalDestinationLinks } from "@erpx/work-projection-ui/lib/destination-links";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import {
  nativeStaffWorkAvailable,
  nativeStaffWorkCommand,
} from "@erpx/operations-server/public/native-staff-work-page";

export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
const headers = { "Cache-Control": "private, no-store" };
const retained = staffHandler("approvals");

/**
 * Native Workflow authority for `task.approval.reroute`. Task approvals are
 * rerouted by the native command (single serializable transaction, receipt,
 * audit, outbox and the staff_tasks version revision). Other approval kinds
 * stay with their own owner until their native ports are mounted.
 */
function nativeRerouteId(path: string[]): string | null {
  if (path.length === 2 && path[1] === "reroute" && path[0]) return path[0];
  return null;
}

export async function GET(request: Request, context: Context) {
  return retained(request, context);
}

export async function POST(request: Request, context: Context) {
  const { holdingSlug, path = [] } = await context.params;
  const instanceId = nativeRerouteId(path);
  if (instanceId !== null) {
    if (!nativeStaffWorkAvailable())
      return Response.json(
        { error: { code: "NATIVE_APPROVAL_REROUTE_UNAVAILABLE", message: "The native Approval owner is unavailable." } },
        { status: 503, headers },
      );
    try {
      const key = request.headers.get("Idempotency-Key") ?? "";
      const body = await request.clone().json();
      const data = await nativeStaffWorkCommand(
        request,
        holdingSlug,
        `/approval/${encodeURIComponent(instanceId)}/reroute`,
        body,
        key,
      );
      return Response.json(canonicalDestinationLinks({ data }), { headers });
    } catch (error) {
      // A non-task approval instance is not owned by the native Task command;
      // leave it to its own owner's retained path. Every other failure is the
      // native owner's answer and must fail closed.
      if (
        error instanceof ApiError &&
        error.code === "APPROVAL_INSTANCE_NOT_FOUND"
      )
        return retained(request, context);
      throw error;
    }
  }
  return retained(request, context);
}
