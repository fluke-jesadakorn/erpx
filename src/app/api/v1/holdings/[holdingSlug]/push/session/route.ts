import { errorResponse } from "@/server/modules/runtime/public/http";
import { removeCurrentPushSession } from "@/server/modules/collaboration/public/web-push";

export const dynamic="force-dynamic";
type Context={params:Promise<{holdingSlug:string}>};
export async function DELETE(request:Request,context:Context) {
  try {
    return Response.json({data:await removeCurrentPushSession(request,(await context.params).holdingSlug)},
      {headers:{"Cache-Control":"private, no-store"}});
  } catch(error) {return errorResponse(error);}
}
