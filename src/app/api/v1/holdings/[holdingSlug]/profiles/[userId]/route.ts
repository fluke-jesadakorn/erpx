import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { errorResponse } from "@/server/modules/runtime/public/http";
import { memberProfileView } from "@/server/modules/organization/public/member-profiles";

export async function GET(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/profiles/[userId]">) {
  try {
    const { holdingSlug, userId } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const beforeRaw = new URL(request.url).searchParams.get("before");
    const before = beforeRaw ? Number(beforeRaw) : undefined;
    const beforeId = new URL(request.url).searchParams.get("beforeId") ?? undefined;
    return Response.json(canonicalDestinationLinks({ data: await memberProfileView(actor,userId,Number.isSafeInteger(before) && before! > 0 ? before : undefined,beforeId) }),
      { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
