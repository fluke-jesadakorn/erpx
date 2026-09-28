import { requireAccountAccess } from "@/server/authorization";
import { ApiError,errorResponse } from "@/server/http";
import { requestPublicOrigin } from "@/server/public-origin";
import { holdingResources } from "@/server/runtime";
import { removePushSession } from "@/server/web-push";

export const dynamic="force-dynamic";
type Context={params:Promise<{holdingSlug:string}>};
export async function DELETE(request:Request,context:Context) {
  try {
    if(request.headers.get("origin")!==requestPublicOrigin(request))
      throw new ApiError(403,"ORIGIN_FORBIDDEN","Use this workspace's own origin.");
    const actor=await requireAccountAccess((await context.params).holdingSlug);
    const {write}=await holdingResources(actor.scope.holdingId);
    return Response.json({data:await removePushSession(write,actor.userId,actor.sessionId)},
      {headers:{"Cache-Control":"private, no-store"}});
  } catch(error) {return errorResponse(error);}
}
