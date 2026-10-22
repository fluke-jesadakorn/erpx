import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { errorResponse } from "@erpx/server-runtime/public/http";
import { memberProfileView } from "@erpx/server-organization/public/member-profiles";

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
