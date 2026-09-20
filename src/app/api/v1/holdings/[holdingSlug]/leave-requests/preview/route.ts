import { requestPublicOrigin } from "@/server/public-origin";
import { requireWorkContext } from "@/server/authorization";
import { ApiError, errorResponse } from "@/server/http";
import { getLeave } from "@/server/leave";
import { previewLeave } from "@/server/leave-self-service";
import { leavePreviewSchema } from "@/lib/leave-self-service";

export async function POST(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/leave-requests/preview">) {
  try {
    const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const raw=await request.json();
    const input = leavePreviewSchema.safeParse(raw);
    if (!input.success) throw new ApiError(422, "VALIDATION_FAILED", input.error.issues[0]?.message ?? "Invalid leave request.");
    const actor = await requireWorkContext((await context.params).holdingSlug);
    return Response.json({ data: await previewLeave(actor, input.data,typeof raw.id === "string" ? (await getLeave(actor.holdingId,raw.id,actor)).id : null) });
  } catch (error) {
    return errorResponse(error);
  }
}
