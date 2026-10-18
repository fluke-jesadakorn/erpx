import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { profileInput, saveMemberProfile } from "@/server/modules/organization/public/member-profiles";

export async function PATCH(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/profiles/me">) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request)) throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const { holdingSlug } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const input = profileInput.parse(await request.json());
    const commandId = request.headers.get("Idempotency-Key") ?? "";
    const data = await saveMemberProfile(actor,input,commandId);
    return Response.json(canonicalDestinationLinks({ data }), { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
