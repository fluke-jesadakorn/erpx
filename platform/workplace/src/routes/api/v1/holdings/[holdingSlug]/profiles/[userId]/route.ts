import type { RouteContext } from "@erpx/platform-contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { errorResponse } from "@erpx/platform-runtime-server/public/http";
import { memberProfileView } from "@erpx/organization-server/public/member-profiles";

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
