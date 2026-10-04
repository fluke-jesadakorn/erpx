import { requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { previewDepartmentFlow } from "@/server/department-authority";
import { ApiError,errorResponse } from "@/server/http";
import { requestPublicOrigin } from "@/server/public-origin";
export async function POST(request:Request,context:{params:Promise<{holdingSlug:string}>}){try{const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");const actor=await requireWorkContext((await context.params).holdingSlug),{write}=await holdingResources(actor.holdingId);return Response.json({data:await previewDepartmentFlow(write,actor,await request.json())},{headers:{"Cache-Control":"private, no-store"}});}catch(error){return errorResponse(error);}}
