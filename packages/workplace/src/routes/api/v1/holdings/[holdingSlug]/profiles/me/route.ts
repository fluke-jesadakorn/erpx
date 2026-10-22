import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { profileInput, saveMemberProfile } from "@erpx/server-organization/public/member-profiles";

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
