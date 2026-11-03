import type {RouteContext} from '@erpx/contracts/next-routes';
import {canonicalDestinationLinks} from '@erpx/feature-work_projection/lib/destination-links';
import {selectNativeCompanyPreference} from '@erpx/server-work_projection/public/workspace-preferences';
import {currentSession} from '@erpx/server-identity_access/public/authorization';
import {ApiError,errorResponse,boundedCommandBody} from '@erpx/server-runtime/public/http';
import {NextResponse} from 'next/server';
export async function POST(request:Request,context:RouteContext<'/api/v1/holdings/[holdingSlug]/companies/active'>){
 try{
  if(!process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG)throw new ApiError(503,'NATIVE_ACCOUNT_PREFERENCE_TARGET_UNAVAILABLE','The native Company selection is not configured.');
  const {holdingSlug}=await context.params,{session}=await currentSession();
  const text=await boundedCommandBody(request);if(new TextEncoder().encode(text).length>16384)throw new ApiError(422,'VALIDATION_FAILED','Choose a valid Company.');
  const state=await selectNativeCompanyPreference(request.headers,session.user.id,text,holdingSlug);
  if(!state.company)throw new ApiError(502,'NATIVE_ACCOUNT_PREFERENCE_RESPONSE_INVALID','The native Company selection is unavailable.');
  const response=NextResponse.json(canonicalDestinationLinks({data:{companyId:state.company.companyId}}),{headers:{'Cache-Control':'private, no-store'}});
  response.cookies.set(`erpx_company_${state.holdingId}`,state.company.companyId,{httpOnly:true,sameSite:'lax',secure:true,path:'/',maxAge:60*60*24*365,priority:'high'});
  return response;
 }catch(error){return errorResponse(error);}
}
