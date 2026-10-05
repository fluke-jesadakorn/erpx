import { requireWorkContext } from "@/server/authorization";
import { holdingResources, runtimeEnv } from "@/server/runtime";
import { ApiError, errorResponse } from "@/server/http";
import { requestPublicOrigin } from "@/server/public-origin";
import { createExternalConnection, listExternalConnections, revokeExternalConnection } from "@/server/executive/connections";
import { z } from "zod";

type Context={params:Promise<{holdingSlug:string}>};
const headers={"Cache-Control":"private, no-store"};
async function resources(context:Context) {
  const {holdingSlug}=await context.params,actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId),{CONTROL_DB}=await runtimeEnv();
  return {actor,write,control:CONTROL_DB};
}
export async function GET(_request:Request,context:Context) {
  try{const {actor,write,control}=await resources(context);return Response.json({data:await listExternalConnections(control,write,actor)},{headers});}catch(e){return errorResponse(e);}
}
export async function POST(request:Request,context:Context) {
  try {
    if(request.headers.get("origin")!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const {actor,write,control}=await resources(context);
    const commandKey=z.string().uuid().parse(request.headers.get("Idempotency-Key"));
    const reader=request.body?.getReader();if(!reader)throw new ApiError(422,"INVALID_JSON","A request body is required.");
    let size=0,body="";const decoder=new TextDecoder();
    while(true){const part=await reader.read();if(part.done)break;size+=part.value.byteLength;if(size>4096){await reader.cancel();throw new ApiError(413,"BODY_TOO_LARGE","This connection request is too large.");}body+=decoder.decode(part.value,{stream:true});}
    let raw:unknown;try{raw=JSON.parse(body+decoder.decode());}catch{throw new ApiError(422,"INVALID_JSON","Use valid JSON.");}
    const data=raw&&typeof raw==="object"&&"revokeId" in raw
      ? await revokeExternalConnection(control,write,actor,z.object({revokeId:z.string().uuid()}).strict().parse(raw).revokeId)
      : await createExternalConnection(control,write,actor,raw,commandKey);
    return Response.json({data},{headers});
  }catch(e){return errorResponse(e);}
}
