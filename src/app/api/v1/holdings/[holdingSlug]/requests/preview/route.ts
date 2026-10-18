import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { employeeRequestSchema,previewEmployeeRequest } from "@/server/modules/records/public/employee-requests";
import { ApiError,errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
export async function POST(request:Request,context:{params:Promise<{holdingSlug:string}>}){try{
 const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
 const actor=await requireWorkContext((await context.params).holdingSlug),{write}=await holdingResources(actor.holdingId),raw=await request.json();
 return Response.json(canonicalDestinationLinks({data:await previewEmployeeRequest(write,actor,employeeRequestSchema.parse(raw),typeof raw.id==="string"?raw.id:undefined)}),{headers:{"Cache-Control":"private, no-store"}});
}catch(error){return errorResponse(error);}}
