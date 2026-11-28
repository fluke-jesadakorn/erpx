import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import {requireWorkContext} from "@erpx/identity-access-server/public/authorization";
import {routingAttention} from "@erpx/workflow-server/public/routing-attention";
import {errorResponse} from "@erpx/platform-runtime-server/public/http";
export async function GET(request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
 try {const actor=await requireWorkContext((await params).holdingSlug);return Response.json(canonicalDestinationLinks({data:await routingAttention(actor,Object.fromEntries(new URL(request.url).searchParams))}),{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}
}
