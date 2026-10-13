import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { newLeaveSchema } from "@/server/modules/people_payroll/public/leave";
import { ApiError,errorResponse } from "@/server/modules/runtime/public/http";
import { createSelfServiceLeave, listOwnLeave, listOwnLeavePage, normalizeLegacyLeave } from "@/server/modules/people_payroll/public/leave-self-service";
import { leaveCreateSchema } from "@/lib/people_payroll/leave-self-service";
export const dynamic = "force-dynamic";
export async function GET(request:Request,{params}:{params:Promise<{holdingSlug:string}>}) {
  try {
    const actor=await requireWorkContext((await params).holdingSlug);
    const search=new URL(request.url).searchParams;
    if(search.has("limit")||search.has("cursor")||search.has("filter")) {
      const page=await listOwnLeavePage(actor,{filter:search.get("filter")??undefined,limit:search.get("limit")??undefined,cursor:search.get("cursor")??undefined});
      return Response.json({data:page.items,hasMore:page.hasMore,nextCursor:page.nextCursor},{headers:{"Cache-Control":"private, no-store"}});
    }
    return Response.json({data:await listOwnLeave(actor,search.get("state") ?? undefined)},{headers:{"Cache-Control":"private, no-store"}});
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
