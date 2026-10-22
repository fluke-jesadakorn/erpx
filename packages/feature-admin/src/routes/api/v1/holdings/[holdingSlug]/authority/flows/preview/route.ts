import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../../../../../../../../packages/server-runtime/src/public/runtime";
import { previewDepartmentFlow } from "@erpx/server-organization/public/department-authority";
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
export async function POST(request:Request,context:{params:Promise<{holdingSlug:string}>}){try{const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");const actor=await requireWorkContext((await context.params).holdingSlug),{write}=await holdingResources(actor.holdingId);return Response.json(canonicalDestinationLinks({data:await previewDepartmentFlow(write,actor,await request.json())}),{headers:{"Cache-Control":"private, no-store"}});}catch(error){return errorResponse(error);}}
