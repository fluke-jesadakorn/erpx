import { assistantAskSchema, assistantPageSchema } from "@/lib/ai/workspace-assistant";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources, runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { askAssistant, assistantView } from "@/server/modules/agent/public/workspace-assistant";
import {
  generateThaiHotelMockAi,
  isThaiHotelAiMockEnabled,
} from "@/server/modules/runtime/public/simulation-only-adapters";
type Context={params:Promise<{holdingSlug:string}>};
export async function GET(request:Request,context:Context) {
  try {
    const actor=await requireWorkContext((await context.params).holdingSlug),{write}=await holdingResources(actor.holdingId);
    const input=assistantPageSchema.parse(Object.fromEntries(new URL(request.url).searchParams));
    return Response.json({data:await assistantView(write,actor,await runtimeEnv(),input.path,input.mode)},{headers:{"Cache-Control":"private, no-store"}});
  }catch(e){return errorResponse(e);}
}
export async function POST(request:Request,context:Context) {
  try {
    if(request.headers.get("origin")!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const actor=await requireWorkContext((await context.params).holdingSlug),{write}=await holdingResources(actor.holdingId);
    const key=request.headers.get("Idempotency-Key");
    if(!key||!/^[a-zA-Z0-9_-]{8,100}$/.test(key))throw new ApiError(422,"COMMAND_KEY_REQUIRED","Use a valid Idempotency-Key.");
    const reader=request.body?.getReader();let body="",bytes=0;
    const decoder=new TextDecoder();
    if(!reader)throw new ApiError(422,"INVALID_JSON","A message is required.");
    while(true){const part=await reader.read();if(part.done)break;bytes+=part.value.byteLength;if(bytes>24000){await reader.cancel();throw new ApiError(413,"MESSAGE_TOO_LARGE","Shorten this message.");}body+=decoder.decode(part.value,{stream:true});}
    let raw:unknown;try{raw=JSON.parse(body+decoder.decode());}catch{throw new ApiError(422,"INVALID_JSON","Use valid JSON.");}
    const data=await askAssistant(write,actor,await runtimeEnv(),assistantAskSchema.parse(raw),key,isThaiHotelAiMockEnabled(actor.holdingSlug)?generateThaiHotelMockAi:undefined);
    return Response.json({data},{headers:{"Cache-Control":"private, no-store"}});
  }catch(e){return errorResponse(e);}
}
