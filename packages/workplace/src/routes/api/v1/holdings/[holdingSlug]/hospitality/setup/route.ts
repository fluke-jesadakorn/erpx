import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { hospitalitySetup } from "@erpx/server-pms/public/hospitality-setup";
import { errorResponse } from "@erpx/server-runtime/public/http";
export const dynamic='force-dynamic';
export async function GET(request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
  try {
    const actor=await requireWorkContext((await params).holdingSlug),query=new URL(request.url).searchParams;
    return Response.json(canonicalDestinationLinks({data:await hospitalitySetup(actor,query.get('branch')??undefined,query.get('property')??undefined)}),{headers:{'Cache-Control':'private, no-store'}});
  } catch(error) {return errorResponse(error);}
}
