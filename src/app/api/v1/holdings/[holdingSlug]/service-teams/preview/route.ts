import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { previewServiceTeam } from "@/server/modules/support/public/service-teams";
import { ApiError,errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
export async function POST(request:Request,context:{params:Promise<{holdingSlug:string}>}){try{
 const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
 const actor=await requireWorkContext((await context.params).holdingSlug),{write}=await holdingResources(actor.holdingId),raw=await request.json();
 const preview=await previewServiceTeam(write,actor,typeof raw.id==="string"?raw.id:null,raw);
 return Response.json({data:preview},{headers:{"Cache-Control":"private, no-store"}});
}catch(error){return errorResponse(error);}}
