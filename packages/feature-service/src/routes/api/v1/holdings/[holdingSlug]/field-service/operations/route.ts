import {boundedCommandBody as fieldCommandBody} from '@erpx/server-runtime/public/http';
import {nativeFieldOperations,NativePartsSetupError} from '@erpx/feature-field_service/public/native-parts-setup';
import {partsSetupContext} from '../../../../../../../lib/parts-setup-context';
import {requireWorkContext} from '@erpx/server-identity_access/public/authorization';
import {requestPublicOrigin} from '@erpx/server-manage_erpx/public/public-origin';
import {ApiError,errorResponse} from '@erpx/server-runtime/public/http';
type Context={params:Promise<{holdingSlug:string}>};
const headers={'Cache-Control':'private, no-store'};
function failure(error:unknown){const response=errorResponse(error instanceof NativePartsSetupError?new ApiError(error.status,error.code,'Field operations is unavailable.'):error);response.headers.set('Cache-Control',headers['Cache-Control']);return response;}
export async function GET(request:Request,context:Context){
 try{if(new URL(request.url).search)throw new ApiError(422,'INVALID_QUERY','Use the Field operations workspace.');const {holdingSlug}=await context.params,actor=await requireWorkContext(holdingSlug);const data=await nativeFieldOperations(request.headers,holdingSlug,partsSetupContext(actor),'GET');return Response.json({data},{headers});}catch(error){return failure(error);}
}
export async function POST(request:Request,context:Context){
 try{if(request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');if(new URL(request.url).search)throw new ApiError(422,'INVALID_QUERY','Use the Field operations workspace.');const {holdingSlug}=await context.params,actor=await requireWorkContext(holdingSlug),key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const body=await fieldCommandBody(request);const data=await nativeFieldOperations(request.headers,holdingSlug,partsSetupContext(actor),'POST',body,key);return Response.json({data},{headers});}catch(error){return failure(error);}
}
