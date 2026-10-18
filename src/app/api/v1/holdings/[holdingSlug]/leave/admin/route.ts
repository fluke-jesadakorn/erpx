import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { administerLeave, leaveAdminSchema, leaveAdministration } from "@/server/modules/people_payroll/public/leave-self-service";

export async function POST(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/leave/admin">) {
  try {
    const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const key = request.headers.get("Idempotency-Key");
    if (!key) throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.");
    const input = leaveAdminSchema.safeParse(await request.json());
    if (!input.success) throw new ApiError(422, "VALIDATION_FAILED", input.error.issues[0]?.message ?? "Invalid leave administration request.");
    const actor = await requireWorkContext((await context.params).holdingSlug);
    return Response.json(canonicalDestinationLinks({ data: await administerLeave(actor, input.data, key) }), { status: 201 });
  } catch (error) {
    return errorResponse(error);
  }
}

export async function GET(_request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
 try{return Response.json(canonicalDestinationLinks({data:await leaveAdministration(await requireWorkContext((await params).holdingSlug))}),{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}
}
