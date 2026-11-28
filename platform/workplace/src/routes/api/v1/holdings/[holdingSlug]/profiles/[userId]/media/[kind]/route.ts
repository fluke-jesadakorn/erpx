import type { RouteContext } from "@erpx/platform-contracts/next-routes";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { holdingResources } from "../../../../../../../../../../../runtime/server/src/public/runtime";
import { profileMediaRow } from "@erpx/organization-server/public/member-profiles";
import { ApiError, errorResponse } from "@erpx/platform-runtime-server/public/http";

export async function GET(_request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/profiles/[userId]/media/[kind]">) {
  try {
    const { holdingSlug,userId,kind } = await context.params;
    if (kind !== "avatar" && kind !== "cover") throw new ApiError(404,"PROFILE_MEDIA_NOT_FOUND","Unknown profile image.");
    const actor = await requireWorkContext(holdingSlug);
    const row = await profileMediaRow(actor,userId,kind);
    const { files } = await holdingResources(actor.holdingId);
    const stored = await files.get(row.key!);
    if (!stored) throw new ApiError(503,"PROFILE_MEDIA_UNAVAILABLE","Profile image temporarily unavailable.");
    return new Response(stored.body, { headers: {
      "Content-Type": row.media_type!, "Cache-Control": "private, no-store", "X-Content-Type-Options": "nosniff",
      "Content-Security-Policy": "default-src 'none'; sandbox",
    } });
  } catch (error) { return errorResponse(error); }
}
