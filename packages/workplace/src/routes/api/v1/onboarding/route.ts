import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { sessionFromRequest } from "@erpx/server-identity_access/public/account-session";
import { onboardingSchema, requireVerified } from "@erpx/server-manage_erpx/public/beta-onboarding";
import { betaBody } from "@erpx/server-manage_erpx/public/beta-http";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
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
