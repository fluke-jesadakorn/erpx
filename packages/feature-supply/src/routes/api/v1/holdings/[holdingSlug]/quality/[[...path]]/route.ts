import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {requestPublicOrigin} from "@erpx/server-manage_erpx/public/public-origin";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {cancelQualityHold,qualityWorkspace,qualityDetail,createQualityHold,inspectQualityHold,saveQualitySpec,transitionQualitySpec,qualityHoldSchema,qualityInspectionSchema,qualitySpecSchema,qualitySpecActionSchema} from "@erpx/server-quality/public/quality";
import {acknowledgeQualityAlert} from "@erpx/server-quality/public/quality-alerts";
import {nativeQualityAvailable,nativeQualityCancellationHistory,nativeQualityDetailCore,nativeQualityWorkspace,nativeQualitySaveSpecification,nativeQualityTransitionSpecification,nativeQualityInspect,nativeQualityRelease,nativeQualityDisposition} from "@erpx/feature-quality/public/native-quality";
import {ownedNativeQualityCancel} from "@erpx/feature-quality/public/native-quality";
type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
function historyCursor(request:Request){
  const url=new URL(request.url), query=url.searchParams, before=query.get('before'), head=query.get('head'), revision=query.get('revision');
  if(new TextEncoder().encode(url.search.slice(1)).length>256)throw new ApiError(422,'QUALITY_HISTORY_CURSOR_INVALID','Reload Quality cancellation history.');
  if([...query.keys()].some(key=>!['before','head','revision'].includes(key))||['before','head','revision'].some(key=>query.getAll(key).length>1))throw new ApiError(422,'QUALITY_HISTORY_CURSOR_INVALID','Reload Quality cancellation history.');
  if(before===null&&head===null&&revision===null)return undefined;
  if(before===null||head===null||revision===null||!/^(?:0|-?[1-9][0-9]*)$/.test(before)||!/^[1-9][0-9]*$/.test(head)||!Number.isSafeInteger(Number(before))||!Number.isSafeInteger(Number(head))||!/^qch1:[0-9a-f]{64}$/.test(revision))throw new ApiError(422,'QUALITY_HISTORY_CURSOR_INVALID','Reload Quality cancellation history.');
  return {before:Number(before),head:Number(head),revision};
}
export async function GET(request:Request,context:Context){try{const {holdingSlug,path=[]}=await context.params,actor=await requireWorkContext(holdingSlug);if(path.length>2)throw new ApiError(404,'NOT_FOUND','Quality route not found.');
  if(path.length===2){
    if(path[1]!=='cancellations')throw new ApiError(404,'NOT_FOUND','Quality route not found.');
    if(!nativeQualityAvailable())throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');
    const page=await nativeQualityCancellationHistory(holdingSlug,path[0],historyCursor(request));
    if(!page)throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');
    return Response.json(canonicalDestinationLinks({data:page}),{headers:{'Cache-Control':'private, no-store'}});
  }
  if(path.length){
    const data=await qualityDetail(actor,path[0]);
    if(!nativeQualityAvailable())return Response.json(canonicalDestinationLinks({data}),{headers:{'Cache-Control':'private, no-store'}});
    const native=await nativeQualityDetailCore(holdingSlug,path[0]);
    if(!native)throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');
    return Response.json(canonicalDestinationLinks({data:{...data,cancellations:[],canInspect:native.hold.decision.canInspect,canRelease:native.hold.decision.canRelease,canCancel:native.hold.decision.canCancel,canReviewCancellation:native.hold.decision.canReviewCancellation,canDisposition:native.hold.decision.canDisposition,hold:{...data.hold,version:native.hold.version,status:native.hold.status,last_result:native.hold.lastResult}}}),{headers:{'Cache-Control':'private, no-store'}});
  }
  const data=await qualityWorkspace(actor);
  if(!nativeQualityAvailable())return Response.json(canonicalDestinationLinks({data}),{headers:{'Cache-Control':'private, no-store'}});
  const native=await nativeQualityWorkspace(holdingSlug);
  if(!native)throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');
  return Response.json(canonicalDestinationLinks({data:{...data,canHold:native.canHold,canManageSpecs:native.canManageSpecs}}),{headers:{'Cache-Control':'private, no-store'}});
}catch(error){return errorResponse(error);}}
export async function POST(request:Request,context:Context){try{const origin=request.headers.get('origin');if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const {holdingSlug,path=[]}=await context.params,actor=await requireWorkContext(holdingSlug),key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const input=await request.json();let data;if(!path.length)data=await createQualityHold(actor,qualityHoldSchema.parse(input),key);else if(path.length===1&&path[0]==='specifications'){if(!nativeQualityAvailable())data=await saveQualitySpec(actor,null,qualitySpecSchema.parse(input),key);else{const native=await nativeQualitySaveSpecification(holdingSlug,input,key);if(!native)throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');data=native;}}else if(path.length===2&&path[0]==='specifications'){if(!nativeQualityAvailable())data=await transitionQualitySpec(actor,path[1],qualitySpecActionSchema.parse(input),key);else{const native=await nativeQualityTransitionSpecification(holdingSlug,path[1],input,key);if(!native)throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');data=native;}}else if(path.length===2&&path[1]==='cancel'){if(!nativeQualityAvailable())data=await cancelQualityHold(actor,path[0],input,key);else{const native=await ownedNativeQualityCancel(actor,holdingSlug,path[0],input,key);if(!native)throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');data=native;}}else if(path.length===2&&path[1]==='inspect'){if(!nativeQualityAvailable())data=await inspectQualityHold(actor,path[0],qualityInspectionSchema.parse(input),key);else{const native=await nativeQualityInspect(holdingSlug,path[0],input,key);if(!native)throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');data=native;}}else if(path.length===2&&path[1]==='release'){if(!nativeQualityAvailable())throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');const native=await nativeQualityRelease(holdingSlug,path[0],input,key);if(!native)throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');data=native;}else if(path.length===2&&path[1]==='disposition'){if(!nativeQualityAvailable())throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');const native=await nativeQualityDisposition(holdingSlug,path[0],input,key);if(!native)throw new ApiError(503,'NATIVE_QUALITY_UNAVAILABLE','The native Quality service is unavailable.');data=native;}else if(path.length===2&&path[1]==='acknowledge')data=await acknowledgeQualityAlert(actor,path[0],input,key);else throw new ApiError(404,'NOT_FOUND','Quality route not found.');return Response.json(canonicalDestinationLinks({data}),{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}}
