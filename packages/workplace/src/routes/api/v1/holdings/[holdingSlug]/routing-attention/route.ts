import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {routingAttention} from "@erpx/server-workflow/public/routing-attention";
import {errorResponse} from "@erpx/server-runtime/public/http";
export async function GET(request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
 try {const actor=await requireWorkContext((await params).holdingSlug);return Response.json(canonicalDestinationLinks({data:await routingAttention(actor,Object.fromEntries(new URL(request.url).searchParams))}),{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}
}
