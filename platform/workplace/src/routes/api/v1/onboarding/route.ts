import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { sessionFromRequest } from "@erpx/identity-access-server/public/account-session";
import { onboardingSchema, requireVerified } from "@erpx/manage-erpx-server/public/beta-onboarding";
import { betaBody } from "@erpx/manage-erpx-server/public/beta-http";
import { ApiError, errorResponse } from "@erpx/platform-runtime-server/public/http";
import {
  nativeManageErpxOnboardingCommand,
  nativeManageErpxOnboardingView,
} from "../../../../lib/native-manage-erpx";

export async function GET(request:Request){
 try{
  const user=(await sessionFromRequest(request))?.user;
  requireVerified(user);
  const data=await nativeManageErpxOnboardingView(request.headers,user.id);
  if(!data) throw new ApiError(503,'NATIVE_MANAGE_ERPX_UNAVAILABLE','The native Manage ERPX reader is unavailable.');
  return Response.json(canonicalDestinationLinks({data}),{headers:{'Cache-Control':'private, no-store'}});
 }catch(e){return errorResponse(e);}
}
export async function POST(request:Request){try{
  const input=onboardingSchema.parse(await betaBody(request));
  const data=await nativeManageErpxOnboardingCommand(request.headers,input);
  if(!data) throw new ApiError(503,'NATIVE_MANAGE_ERPX_UNAVAILABLE','The native Manage ERPX onboarding command is unavailable.');
  return Response.json(canonicalDestinationLinks({data}));
 }catch(e){return errorResponse(e);}}
