import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { errorResponse } from "@erpx/platform-runtime-server/public/http";
import { removeCurrentPushSession } from "@erpx/collaboration-server/public/web-push";

export const dynamic="force-dynamic";
type Context={params:Promise<{holdingSlug:string}>};
export async function DELETE(request:Request,context:Context) {
  try {
    return Response.json(canonicalDestinationLinks({data:await removeCurrentPushSession(request,(await context.params).holdingSlug)}),
      {headers:{"Cache-Control":"private, no-store"}});
  } catch(error) {return errorResponse(error);}
}
