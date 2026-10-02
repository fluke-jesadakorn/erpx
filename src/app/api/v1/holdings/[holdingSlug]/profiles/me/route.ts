import { requireWorkContext } from "@/server/authorization";
import { requestPublicOrigin } from "@/server/public-origin";
import { ApiError, errorResponse } from "@/server/http";
import { profileInput, saveMemberProfile } from "@/server/member-profiles";

export async function PATCH(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/profiles/me">) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request)) throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const { holdingSlug } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const input = profileInput.parse(await request.json());
    const commandId = request.headers.get("Idempotency-Key") ?? "";
    const data = await saveMemberProfile(actor,input,commandId);
    return Response.json({ data }, { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
