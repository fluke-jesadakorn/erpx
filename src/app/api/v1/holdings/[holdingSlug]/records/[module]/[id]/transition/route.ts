import { requestPublicOrigin } from "@/server/public-origin";
import { requireWorkContext } from "@/server/authorization";
import { errorResponse, ApiError } from "@/server/http";
import { transitionRecord, transitionSchema } from "@/server/records";
export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]/[id]/transition">){
  try{if(request.headers.get("origin") && request.headers.get("origin")!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");const key=request.headers.get("Idempotency-Key");if(!key)throw new ApiError(422,"COMMAND_KEY_REQUIRED","Idempotency-Key is required.");const {holdingSlug,module,id}=await context.params;const parsed=transitionSchema.safeParse(await request.json());if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Invalid transition.");
    return Response.json({data:await transitionRecord(await requireWorkContext(holdingSlug),module,id,parsed.data,key)});
  }catch(error){return errorResponse(error);}
}
