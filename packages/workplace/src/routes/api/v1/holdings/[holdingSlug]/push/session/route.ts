import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { errorResponse } from "@erpx/server-runtime/public/http";
import { removeCurrentPushSession } from "@erpx/server-collaboration/public/web-push";

export const dynamic="force-dynamic";
type Context={params:Promise<{holdingSlug:string}>};
export async function DELETE(request:Request,context:Context) {
  try {
    return Response.json(canonicalDestinationLinks({data:await removeCurrentPushSession(request,(await context.params).holdingSlug)}),
      {headers:{"Cache-Control":"private, no-store"}});
  } catch(error) {return errorResponse(error);}
}
