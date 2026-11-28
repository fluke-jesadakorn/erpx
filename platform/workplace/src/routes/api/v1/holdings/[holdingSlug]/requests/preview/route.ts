import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { holdingResources } from "../../../../../../../../../runtime/server/src/public/runtime";
import {employeeRequestSchema,previewEmployeeRequest} from '@erpx/records-server/public/employee-requests';
import { ApiError,errorResponse } from "@erpx/platform-runtime-server/public/http";
import { requestPublicOrigin } from "@erpx/manage-erpx-server/public/public-origin";
export async function POST(request:Request,context:{params:Promise<{holdingSlug:string}>}){try{
 const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
 const actor=await requireWorkContext((await context.params).holdingSlug),{write}=await holdingResources(actor.holdingId),raw=await request.json();
 return Response.json(canonicalDestinationLinks({data:await previewEmployeeRequest(write,actor,employeeRequestSchema.parse(raw),typeof raw.id==="string"?raw.id:undefined)}),{headers:{"Cache-Control":"private, no-store"}});
}catch(error){return errorResponse(error);}}
