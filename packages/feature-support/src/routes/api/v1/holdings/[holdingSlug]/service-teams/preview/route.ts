import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import { previewServiceTeam } from "@erpx/server-support/public/service-teams";
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
export async function POST(request:Request,context:{params:Promise<{holdingSlug:string}>}){try{
 const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
 const actor=await requireWorkContext((await context.params).holdingSlug),{write}=await holdingResources(actor.holdingId),raw=await request.json();
 const preview=await previewServiceTeam(write,actor,typeof raw.id==="string"?raw.id:null,raw);
 return Response.json(canonicalDestinationLinks({data:preview}),{headers:{"Cache-Control":"private, no-store"}});
}catch(error){return errorResponse(error);}}
