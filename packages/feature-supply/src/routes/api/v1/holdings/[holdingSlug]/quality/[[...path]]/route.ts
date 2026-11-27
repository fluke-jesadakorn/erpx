import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {requestPublicOrigin} from "@erpx/server-manage_erpx/public/public-origin";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {nativeQualityAvailable,nativeQualityDetail,nativeQualitySaveSpecification,nativeQualityTransitionSpecification,nativeQualityInspect,nativeQualityHoldCreate,nativeQualityRelease,nativeQualityDisposition} from "@erpx/feature-quality/public/native-quality";
import {ownedNativeQualityCancel} from "@erpx/feature-quality/public/native-quality";
type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
function unavailable():never{throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');}
export async function GET(_request:Request,context:Context){
  try{
    const {holdingSlug,path=[]}=await context.params;
    await requireWorkContext(holdingSlug);
    // These named routes retain their previous errors until their own cutover.
    if(path.length!==1||!['alerts','read-plan','specifications'].includes(path[0]))
      throw new ApiError(404,'NOT_FOUND','Quality route not found.');
    if(!nativeQualityAvailable())unavailable();
    const native=await nativeQualityDetail(holdingSlug,path[0]);
    if(!native)unavailable();
    return Response.json(canonicalDestinationLinks({data:native}),{headers:{'Cache-Control':'private, no-store'}});
  }catch(error){return errorResponse(error);}
}
export async function POST(request:Request,context:Context){try{const origin=request.headers.get('origin');if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const {holdingSlug,path=[]}=await context.params,actor=await requireWorkContext(holdingSlug),key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const input=await request.json();let data;if(!path.length){if(!nativeQualityAvailable())throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');const native=await nativeQualityHoldCreate(holdingSlug,input,key);if(!native)throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');data=native;}else if(path.length===1&&path[0]==='specifications'){if(!nativeQualityAvailable())unavailable();const native=await nativeQualitySaveSpecification(holdingSlug,input,key);if(!native)unavailable();data=native;}else if(path.length===2&&path[0]==='specifications'){if(!nativeQualityAvailable())unavailable();const native=await nativeQualityTransitionSpecification(holdingSlug,path[1],input,key);if(!native)unavailable();data=native;}else if(path.length===2&&path[1]==='cancel'){if(!nativeQualityAvailable())unavailable();const native=await ownedNativeQualityCancel(actor,holdingSlug,path[0],input,key);if(!native)unavailable();data=native;}else if(path.length===2&&path[1]==='inspect'){if(!nativeQualityAvailable())unavailable();const native=await nativeQualityInspect(holdingSlug,path[0],input,key);if(!native)unavailable();data=native;}else if(path.length===2&&path[1]==='release'){if(!nativeQualityAvailable())throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');const native=await nativeQualityRelease(holdingSlug,path[0],input,key);if(!native)throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');data=native;}else if(path.length===2&&path[1]==='disposition'){if(!nativeQualityAvailable())throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');const native=await nativeQualityDisposition(holdingSlug,path[0],input,key);if(!native)throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');data=native;}else throw new ApiError(404,'NOT_FOUND','Quality route not found.');return Response.json(canonicalDestinationLinks({data}),{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}}
