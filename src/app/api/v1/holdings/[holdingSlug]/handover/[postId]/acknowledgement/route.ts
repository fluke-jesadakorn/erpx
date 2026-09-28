import { requireWorkContext } from "@/server/authorization";
import { acknowledgeHandover, handoverAcknowledgement } from "@/server/handover";
import { ApiError, errorResponse } from "@/server/http";
import { requestPublicOrigin } from "@/server/public-origin";
import { holdingResources } from "@/server/runtime";

export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string; postId: string }> };
const headers = { "Cache-Control": "private, no-store" };

export async function GET(_request: Request, context: Context) {
  try {
    const params = await context.params;
    const actor = await requireWorkContext(params.holdingSlug);
    const { write } = await holdingResources(actor.holdingId);
    return Response.json({ data: await handoverAcknowledgement(write, actor, params.postId) }, { headers });
  } catch (error) { return errorResponse(error); }
}

export async function POST(request: Request, context: Context) {
  try {
    if (request.headers.get("origin") !== requestPublicOrigin(request))
      throw new ApiError(403,"ORIGIN_FORBIDDEN","Confirm from this workspace only.");
    const key=request.headers.get("Idempotency-Key");
    if(!key||!/^[a-f0-9-]{36}$/i.test(key))
      throw new ApiError(422,"COMMAND_KEY_REQUIRED","Provide a valid confirmation key.");
    const params=await context.params;
    const actor=await requireWorkContext(params.holdingSlug);
    const {write}=await holdingResources(actor.holdingId);
    return Response.json({data:await acknowledgeHandover(write,actor,params.postId,key)},{headers});
  } catch(error) { return errorResponse(error); }
}
