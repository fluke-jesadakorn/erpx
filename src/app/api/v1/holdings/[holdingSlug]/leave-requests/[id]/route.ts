import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { getLeave, leaveDraftSchema } from "@/server/modules/people_payroll/public/leave";
import { editSelfServiceLeave, leaveEditSchema, normalizeLegacyLeave } from "@/server/modules/people_payroll/public/leave-self-service";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";

export const dynamic = "force-dynamic";

export async function GET(_request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/leave-requests/[id]">) {
  try {
    const { holdingSlug, id } = await context.params;
    const workContext = await requireWorkContext(holdingSlug);
    return Response.json({ data: await getLeave(workContext.holdingId, id, workContext) });
  } catch (error) {
    return errorResponse(error);
  }
}

export async function PATCH(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/leave-requests/[id]">) {
  try {
    const { holdingSlug, id } = await context.params;
    const commandId = request.headers.get("Idempotency-Key");
    if (!commandId) throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.");
    const origin=request.headers.get("origin"); if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const raw=await request.json(), modern=leaveEditSchema.safeParse(raw);
    const workContext = await requireWorkContext(holdingSlug);
    if(modern.success)return Response.json({data:await editSelfServiceLeave(workContext,id,modern.data,commandId)});
    const parsed = leaveDraftSchema.safeParse(raw);
    if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid request.");
    return Response.json({ data: await editSelfServiceLeave(workContext,id,{...await normalizeLegacyLeave(workContext,parsed.data,id),expectedVersion:parsed.data.expectedVersion,saveOnly:parsed.data.saveOnly},commandId) });
  } catch (error) {
    return errorResponse(error);
  }
}
