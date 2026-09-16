import { requestPublicOrigin } from "@/server/public-origin";
import { z } from "zod";
import { requireWorkContext } from "@/server/authorization";
import { ApiError,errorResponse } from "@/server/http";
import { listCounts,countOptions,countDetail,saveCount,transitionCount,countSchema,countActionSchema } from "@/server/inventory-counts";
type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
export async function GET(_request:Request,context:Context){try{const {holdingSlug,path:p=[]}=await context.params,actor=await requireWorkContext(holdingSlug);if(p.length>1)throw new ApiError(404,"NOT_FOUND","Stock count route not found.");return Response.json({data:p.length?await countDetail(actor,p[0]):{counts:await listCounts(actor),options:await countOptions(actor)}},{headers:{"Cache-Control":"private, no-store"}});}catch(e){return errorResponse(e);}}
export async function POST(request:Request,context:Context){try{
 const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
 const {holdingSlug,path:p=[]}=await context.params,actor=await requireWorkContext(holdingSlug),key=request.headers.get("Idempotency-Key");if(!key)throw new ApiError(422,"COMMAND_KEY_REQUIRED","Idempotency-Key is required.");const input=await request.json();let data;
 if(p.length<=1)data=await saveCount(actor,p[0]??null,countSchema.parse(input),key);
 else if(p.length===2&&["submit","reject","cancel","approve","post"].includes(p[1]))data=await transitionCount(actor,p[0],p[1] as "submit"|"reject"|"cancel"|"approve"|"post",countActionSchema.parse(input),key);
 else throw new ApiError(404,"NOT_FOUND","Stock count action not found.");return Response.json({data},{status:p.length===0?201:200});
}catch(e){if(e instanceof z.ZodError)return errorResponse(new ApiError(422,"VALIDATION_FAILED",e.issues[0]?.message??"Invalid input."));if(e instanceof SyntaxError)return errorResponse(new ApiError(422,"INVALID_JSON","Use valid JSON."));return errorResponse(e);}}
