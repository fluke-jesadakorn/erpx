import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { holdingResources } from "../../../../../../../../../../../../platform/runtime/server/src/public/runtime";
import {previewDepartmentFlow} from '@erpx/organization-server/public/department-authority-reads';
import { ApiError,errorResponse } from "@erpx/platform-runtime-server/public/http";
import { requestPublicOrigin } from "@erpx/manage-erpx-server/public/public-origin";
export async function POST(request:Request,context:{params:Promise<{holdingSlug:string}>}){try{const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");const actor=await requireWorkContext((await context.params).holdingSlug),{write}=await holdingResources(actor.holdingId);return Response.json(canonicalDestinationLinks({data:await previewDepartmentFlow(write,actor,await request.json())}),{headers:{"Cache-Control":"private, no-store"}});}catch(error){return errorResponse(error);}}
