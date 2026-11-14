import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { createNativeExecutiveConnections } from "@erpx/server-runtime/public/executive/native-connections";

type Context={params:Promise<{holdingSlug:string}>};
const headers={"Cache-Control":"private, no-store"};
const client=()=>createNativeExecutiveConnections(process.env.ERPX_NATIVE_EXECUTIVE_CONNECTIONS_CONFIG??null);
export async function GET(request:Request,context:Context) {
  try{
    const {holdingSlug}=await context.params;
    const data=await client().list(request.headers,holdingSlug);
    return Response.json(canonicalDestinationLinks({data}),{headers});
  }catch(e){return errorResponse(e);}
}
export async function POST(request:Request,context:Context) {
  try {
    const {holdingSlug}=await context.params;
    const key=request.headers.get("Idempotency-Key");
    if(!key)throw new ApiError(422,"VALIDATION_FAILED","Invalid uuid");
    const body=await request.text();
    if(Buffer.byteLength(body)>4096)throw new ApiError(413,"BODY_TOO_LARGE","This connection request is too large.");
    const data=await client().command(request.headers,holdingSlug,body,key);
    return Response.json(canonicalDestinationLinks({data}),{headers});
  }catch(e){return errorResponse(e);}
}
