import { requestPublicOrigin } from "@/server/public-origin";
import { requireWorkContext } from "@/server/authorization";
import { newLeaveSchema } from "@/server/leave";
import { ApiError,errorResponse } from "@/server/http";
import { createSelfServiceLeave, listOwnLeave, normalizeLegacyLeave } from "@/server/leave-self-service";
import { leaveCreateSchema } from "@/lib/leave-self-service";
export const dynamic = "force-dynamic";
export async function GET(request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
  try {
    const actor=await requireWorkContext((await params).holdingSlug);
    return Response.json({data:await listOwnLeave(actor,new URL(request.url).searchParams.get("state") ?? undefined)});
  } catch(e) { return errorResponse(e); }
}
export async function POST(request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
  try {
    const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const key=request.headers.get("Idempotency-Key");if(!key)throw new ApiError(422,"COMMAND_KEY_REQUIRED","Idempotency-Key is required.");
    const raw:unknown=await request.json();
    const actor=await requireWorkContext((await params).holdingSlug);
    const modern=leaveCreateSchema.safeParse(raw);
    if(modern.success)return Response.json({data:await createSelfServiceLeave(actor,modern.data,key)},{status:201});
    const input=newLeaveSchema.safeParse(raw);if(!input.success)throw new ApiError(422,"VALIDATION_FAILED",modern.error.issues[0]?.message ?? input.error.issues[0].message);
    return Response.json({data:await createSelfServiceLeave(actor,{...await normalizeLegacyLeave(actor,input.data),saveOnly:true},key)},{status:201,headers:{Warning:'299 ERPX "Legacy leave request shape is deprecated"',Deprecation:"true"}});
  }catch(e){return errorResponse(e);}
}
