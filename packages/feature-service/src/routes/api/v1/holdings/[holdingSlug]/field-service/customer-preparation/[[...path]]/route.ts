import {requireWorkContext} from '@erpx/server-identity_access/public/authorization';
import {requestPublicOrigin} from '@erpx/server-manage_erpx/public/public-origin';
import {ApiError,errorResponse,boundedCommandBody} from '@erpx/server-runtime/public/http';
import {nativeServiceCustomerPreparation,nativeActorContext,NativePartsSetupError} from '@erpx/feature-field_service/public/native-parts-setup';
type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
const headers={'Cache-Control':'private, no-store'};
const missing=()=>new ApiError(404,'NOT_FOUND','Customer preparation route not found.');
function failure(error:unknown){const response=errorResponse(error instanceof NativePartsSetupError?new ApiError(error.status,error.code,'Customer preparation is unavailable. Retry the original request.'):error);response.headers.set('Cache-Control',headers['Cache-Control']);return response;}
export async function GET(request:Request,context:Context){try{
 const {holdingSlug,path=[]}=await context.params;if(path.length||new URL(request.url).search)throw missing();
 const actor=await requireWorkContext(holdingSlug),data=await nativeServiceCustomerPreparation(request.headers,holdingSlug,nativeActorContext(actor),'read');return Response.json({data},{headers});
}catch(error){return failure(error);}}
export async function POST(request:Request,context:Context){try{
 if(request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');
 const {holdingSlug,path=[]}=await context.params;if(new URL(request.url).search||!(path.length===0||path.length===1&&path[0]==='confirmations'))throw missing();
 const actor=await requireWorkContext(holdingSlug),key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');
 const body=await boundedCommandBody(request),data=await nativeServiceCustomerPreparation(request.headers,holdingSlug,nativeActorContext(actor),path.length?'confirm':'create',body,key);
 return Response.json({data},{headers,status:path.length?200:201});
}catch(error){return failure(error);}}
