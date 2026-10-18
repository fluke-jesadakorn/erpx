import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { errorResponse } from "@/server/modules/runtime/public/http";
import { searchReferences } from "@/server/modules/records/public/records";
export async function GET(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/search">){try{const {holdingSlug}=await context.params;const q=new URL(request.url).searchParams.get("q")??"";return Response.json(canonicalDestinationLinks({data:q.length<1?[]:await searchReferences(await requireWorkContext(holdingSlug),q)}));}catch(error){return errorResponse(error);}}
