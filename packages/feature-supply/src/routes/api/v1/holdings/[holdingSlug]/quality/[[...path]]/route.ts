import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {requestPublicOrigin} from "@erpx/server-manage_erpx/public/public-origin";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {cancelQualityHold,qualityWorkspace,qualityDetail,createQualityHold,inspectQualityHold,releaseQualityHold,createQualityScrapDisposition,saveQualitySpec,transitionQualitySpec,qualityHoldSchema,qualityInspectionSchema,qualityReleaseSchema,qualityDispositionSchema,qualitySpecSchema,qualitySpecActionSchema} from "@erpx/server-quality/public/quality";
import {acknowledgeQualityAlert} from "@erpx/server-quality/public/quality-alerts";
import {nativeQualityAvailable,nativeQualityDetail,nativeQualityWorkspace} from "@erpx/feature-quality/public/native-quality";
import {ownedNativeQualityCancel} from "@erpx/feature-quality/public/native-quality";
type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
export async function GET(_request:Request,context:Context){try{const {holdingSlug,path=[]}=await context.params,actor=await requireWorkContext(holdingSlug);if(path.length>1)throw new ApiError(404,'NOT_FOUND','Quality route not found.');
  if(path.length){
    const data=await qualityDetail(actor,path[0]);
    if(!nativeQualityAvailable())return Response.json(canonicalDestinationLinks({data}),{headers:{'Cache-Control':'private, no-store'}});
    const native=await nativeQualityDetail(holdingSlug,path[0]);
    if(!native)throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');
    return Response.json(canonicalDestinationLinks({data:{...data,canInspect:native.hold.decision.canInspect,canRelease:native.hold.decision.canRelease,canCancel:native.hold.decision.canCancel,canReviewCancellation:native.hold.decision.canReviewCancellation,canDisposition:native.hold.decision.canDisposition,hold:{...data.hold,version:native.hold.version,status:native.hold.status,last_result:native.hold.lastResult}}}),{headers:{'Cache-Control':'private, no-store'}});
  }
  const data=await qualityWorkspace(actor);
  if(!nativeQualityAvailable())return Response.json(canonicalDestinationLinks({data}),{headers:{'Cache-Control':'private, no-store'}});
  const native=await nativeQualityWorkspace(holdingSlug);
  if(!native)throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');
  return Response.json(canonicalDestinationLinks({data:{...data,canHold:native.canHold,canManageSpecs:native.canManageSpecs}}),{headers:{'Cache-Control':'private, no-store'}});
}catch(error){return errorResponse(error);}}
export async function POST(request:Request,context:Context){try{const origin=request.headers.get('origin');if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const {holdingSlug,path=[]}=await context.params,actor=await requireWorkContext(holdingSlug),key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const input=await request.json();let data;if(!path.length)data=await createQualityHold(actor,qualityHoldSchema.parse(input),key);else if(path.length===1&&path[0]==='specifications')data=await saveQualitySpec(actor,null,qualitySpecSchema.parse(input),key);else if(path.length===2&&path[0]==='specifications')data=await transitionQualitySpec(actor,path[1],qualitySpecActionSchema.parse(input),key);else if(path.length===2&&path[1]==='cancel'){if(!nativeQualityAvailable())data=await cancelQualityHold(actor,path[0],input,key);else{const native=await ownedNativeQualityCancel(actor,holdingSlug,path[0],input,key);if(!native)throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');data=native;}}else if(path.length===2&&path[1]==='inspect')data=await inspectQualityHold(actor,path[0],qualityInspectionSchema.parse(input),key);else if(path.length===2&&path[1]==='release')data=await releaseQualityHold(actor,path[0],qualityReleaseSchema.parse(input),key);else if(path.length===2&&path[1]==='disposition')data=await createQualityScrapDisposition(actor,path[0],qualityDispositionSchema.parse(input),key);else if(path.length===2&&path[1]==='acknowledge')data=await acknowledgeQualityAlert(actor,path[0],input,key);else throw new ApiError(404,'NOT_FOUND','Quality route not found.');return Response.json(canonicalDestinationLinks({data}),{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}}
