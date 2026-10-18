import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import {requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {businessReadiness} from "@/server/modules/runtime/public/business-readiness";
import {errorResponse} from "@/server/modules/runtime/public/http";
export async function GET(_request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
 try{return Response.json(canonicalDestinationLinks({data:await businessReadiness(await requireWorkContext((await params).holdingSlug))}),{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}
}
