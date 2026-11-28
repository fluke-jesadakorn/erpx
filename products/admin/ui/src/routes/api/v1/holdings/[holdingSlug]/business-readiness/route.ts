import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import {requireWorkContext} from "@erpx/identity-access-server/public/authorization";
import {businessReadiness} from "@erpx/platform-runtime-server/public/business-readiness";
import {errorResponse} from "@erpx/platform-runtime-server/public/http";
export async function GET(_request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
 try{return Response.json(canonicalDestinationLinks({data:await businessReadiness(await requireWorkContext((await params).holdingSlug))}),{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}
}
