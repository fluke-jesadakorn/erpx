import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { transitionLeave,leaveLifecycleSchema } from "@erpx/server-people_payroll/public/leave";
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";
export async function POST(request:Request,{params}:{params:Promise<{holdingSlug:string;id:string}>}) {
  try {
    const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const key=request.headers.get("Idempotency-Key");if(!key)throw new ApiError(422,"COMMAND_KEY_REQUIRED","Idempotency-Key is required.");
    const input=leaveLifecycleSchema.safeParse(await request.json());if(!input.success)throw new ApiError(422,"VALIDATION_FAILED",input.error.issues[0].message);
    const {holdingSlug,id}=await params;return Response.json(canonicalDestinationLinks({data:await transitionLeave(await requireWorkContext(holdingSlug),id,input.data,key)}));
  }catch(e){return errorResponse(e);}
}
