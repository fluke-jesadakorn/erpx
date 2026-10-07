import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { pushSettings, registerPush, removePushDevice, savePushPreferences } from "@/server/modules/collaboration/public/web-push";
import { workspaceBase } from "@/lib/workspace";
import { frontAppForHost, frontPushLandingPath } from "@/lib/front-host";
import { frontCompanyBrand } from "@/server/modules/work_projection/public/front-app";

export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug:string }> };
const response = (data:unknown) => Response.json({data},{headers:{"Cache-Control":"private, no-store"}});
function assertOrigin(request:Request) {
  if(request.headers.get("origin")!==requestPublicOrigin(request))
    throw new ApiError(403,"ORIGIN_FORBIDDEN","Use this workspace's own origin.");
}
export async function GET(_request:Request,context:Context) {
  try {
    const actor=await requireWorkContext((await context.params).holdingSlug);
    const {write}=await holdingResources(actor.holdingId);
    return response(await pushSettings(write,actor));
  } catch(error) { return errorResponse(error); }
}
export async function POST(request:Request,context:Context) {
  try {
    assertOrigin(request);
    const actor=await requireWorkContext((await context.params).holdingSlug),input=await request.json();
    const {write}=await holdingResources(actor.holdingId);
    if(input?.action==="subscribe"){
      let landingPath=`${workspaceBase(publicScope(actor))}/inbox?tab=updates`;
      if(frontAppForHost(request.headers.get("host"))){
        if(typeof input.frontCompanySlug!=="string" || !/^[A-Za-z0-9_-]+$/.test(input.frontCompanySlug))
          throw new ApiError(422,"INVALID_FRONT_COMPANY","Choose a valid Front company.");
        const company=await frontCompanyBrand(actor.holdingSlug,input.frontCompanySlug);
        if(!company || company.companyId!==actor.companyId)
          throw new ApiError(403,"FRONT_COMPANY_FORBIDDEN","This Front company is not your selected work company.");
        landingPath=frontPushLandingPath(actor.scope,company.companySlug);
      }
      return response(await registerPush(write,actor,requestPublicOrigin(request),landingPath,input.subscription));
    }
    if(input?.action==="preferences")return response(await savePushPreferences(write,actor,input.preferences));
    throw new ApiError(422,"INVALID_PUSH_ACTION","Choose a push settings action.");
  } catch(error) { return errorResponse(error); }
}
export async function DELETE(request:Request,context:Context) {
  try {
    assertOrigin(request);
    const actor=await requireWorkContext((await context.params).holdingSlug),input=await request.json().catch(()=>({}));
    const {write}=await holdingResources(actor.holdingId);
    return response(await removePushDevice(write,actor,typeof input?.endpoint==="string"?input.endpoint:undefined));
  } catch(error) { return errorResponse(error); }
}
