import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { hospitalitySetup } from "@erpx/pms-server/public/hospitality-setup";
import { errorResponse } from "@erpx/platform-runtime-server/public/http";
export const dynamic='force-dynamic';
export async function GET(request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
  try {
    const actor=await requireWorkContext((await params).holdingSlug),query=new URL(request.url).searchParams;
    return Response.json(canonicalDestinationLinks({data:await hospitalitySetup(actor,query.get('branch')??undefined,query.get('property')??undefined)}),{headers:{'Cache-Control':'private, no-store'}});
  } catch(error) {return errorResponse(error);}
}
