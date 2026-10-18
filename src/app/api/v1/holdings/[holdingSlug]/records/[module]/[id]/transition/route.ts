import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { errorResponse, ApiError } from "@/server/modules/runtime/public/http";
import { transitionRecord, transitionSchema } from "@/server/modules/records/public/records";
export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]/[id]/transition">){
  try{if(request.headers.get("origin") && request.headers.get("origin")!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");const key=request.headers.get("Idempotency-Key");if(!key)throw new ApiError(422,"COMMAND_KEY_REQUIRED","Idempotency-Key is required.");const {holdingSlug,module,id}=await context.params;const parsed=transitionSchema.safeParse(await request.json());if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Invalid transition.");
    return Response.json(canonicalDestinationLinks({data:await transitionRecord(await requireWorkContext(holdingSlug),module,id,parsed.data,key)}));
  }catch(error){return errorResponse(error);}
}
