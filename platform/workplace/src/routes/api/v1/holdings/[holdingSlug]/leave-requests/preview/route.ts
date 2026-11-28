import type { RouteContext } from "@erpx/platform-contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requestPublicOrigin } from "@erpx/manage-erpx-server/public/public-origin";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { ApiError, errorResponse } from "@erpx/platform-runtime-server/public/http";
import {getLeave} from '@erpx/people-payroll-server/public/leave-read';
import { previewLeave } from "@erpx/people-payroll-server/public/leave-self-service";
import { leavePreviewSchema } from "@erpx/people-payroll-ui/lib/leave-self-service";

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
