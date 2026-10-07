import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { acknowledgeHandover, handoverAcknowledgement } from "@/server/modules/operations/public/handover";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { holdingResources } from "@/server/modules/runtime/public/runtime";

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
