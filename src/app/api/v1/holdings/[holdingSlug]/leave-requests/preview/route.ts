import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { getLeave } from "@/server/modules/people_payroll/public/leave";
import { previewLeave } from "@/server/modules/people_payroll/public/leave-self-service";
import { leavePreviewSchema } from "@/lib/people_payroll/leave-self-service";

export async function POST(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/leave-requests/preview">) {
  try {
    const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const raw=await request.json();
    const input = leavePreviewSchema.safeParse(raw);
    if (!input.success) throw new ApiError(422, "VALIDATION_FAILED", input.error.issues[0]?.message ?? "Invalid leave request.");
    const actor = await requireWorkContext((await context.params).holdingSlug);
    return Response.json(canonicalDestinationLinks({ data: await previewLeave(actor, input.data,typeof raw.id === "string" ? (await getLeave(actor.holdingId,raw.id,actor)).id : null) }));
  } catch (error) {
    return errorResponse(error);
  }
}
