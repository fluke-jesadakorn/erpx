import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {businessReadiness} from "@erpx/server-runtime/public/business-readiness";
import {errorResponse} from "@erpx/server-runtime/public/http";
export async function GET(_request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
 try{return Response.json(canonicalDestinationLinks({data:await businessReadiness(await requireWorkContext((await params).holdingSlug))}),{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}
}
