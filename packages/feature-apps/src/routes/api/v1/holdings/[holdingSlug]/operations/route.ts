import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {requestPublicOrigin} from "@erpx/server-manage_erpx/public/public-origin";
import {ApiError,errorResponse,boundedCommandBody} from "@erpx/server-runtime/public/http";
import {draftQueuePolicy,reviewQueuePolicy,openOperationsException,changeOperationsException,nativeFieldException,nativeFieldPolicy} from "@erpx/server-operations/public/operations-exceptions";
import {nativeFieldOperations,nativeFieldOperationsDetail,nativeActorContext,NativePartsSetupError} from '@erpx/feature-field_service/public/native-parts-setup';
import {validFieldOperationsWorkspace,validFieldOperationsDetailRead} from '@erpx/feature-operations/lib/operations-work-model';
import {nativeOperationsAvailable,nativeOperationsCommand,nativeOperationsDetail,nativeOperationsFailure,nativeOperationsWorkspace} from '@erpx/workplace/lib/native-operations';
type Context={params:Promise<{holdingSlug:string}>};
const headers={"Cache-Control":"private, no-store"};
function failure(error:unknown){const response=errorResponse(error instanceof NativePartsSetupError?new ApiError(error.status,error.code,error.message):error);response.headers.set('Cache-Control','private, no-store');return response;}
function unavailable():never{throw new ApiError(503,'NATIVE_OPERATIONS_UNAVAILABLE','The native Operations reader is unavailable.');}
export async function GET(r:Request,c:Context){
 try{
  const {holdingSlug}=await c.params,a=await requireWorkContext(holdingSlug),id=new URL(r.url).searchParams.get("id");
  if(!nativeOperationsAvailable())unavailable();
  try{
   const data=id?await nativeOperationsDetail(a,holdingSlug,id):await nativeOperationsWorkspace(a,holdingSlug);
   if(!data)unavailable();
   return Response.json(canonicalDestinationLinks({data}),{headers});
  }catch(error){
   const native=nativeOperationsFailure(error);
   if(native?.status!==404||!id||!a.modules.includes('field_service'))throw native?new ApiError(native.status,native.code,native.message):error;
   const context=nativeActorContext(a),workspace=await nativeFieldOperations(r.headers,holdingSlug,context,'GET'),record=await nativeFieldOperationsDetail(r.headers,holdingSlug,context,id);
   if(!validFieldOperationsWorkspace(workspace,a.companyId)||!validFieldOperationsDetailRead(record,a.companyId,id))throw new ApiError(503,'NATIVE_FIELD_OPERATIONS_RESPONSE_INVALID','Field operations is unavailable.');
   return Response.json(canonicalDestinationLinks({data:{...workspace,row:{...record.row,source:record.source},history:record.history,canWrite:record.canWrite,canReview:record.canReview}}),{headers});
  }
 }catch(e){return failure(e);}
}
export async function POST(r:Request,c:Context){
 try{
  if(r.headers.get("origin")&&r.headers.get("origin")!==requestPublicOrigin(r))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
  const key=r.headers.get("Idempotency-Key");if(!key)throw new ApiError(422,"COMMAND_KEY_REQUIRED","Idempotency-Key is required.");
  const {holdingSlug}=await c.params,a=await requireWorkContext(holdingSlug),body=await boundedCommandBody(r);
  let input:unknown;try{input=JSON.parse(body);}catch{throw new ApiError(422,'INVALID_JSON','Use a JSON command.');}
  if(!input||typeof input!=="object"||Array.isArray(input)||!('entity'in input)||!["policy","exception"].includes(String(input.entity)))throw new ApiError(422,"OPERATIONS_ENTITY_REQUIRED","Choose a policy or exception command.");
  const value=input as Record<string,unknown>,source=value.source;
  const native=typeof value.id==='string'
   ?await(value.entity==='policy'?nativeFieldPolicy(a,value.id):nativeFieldException(a,value.id))
   :value.entity==='policy'?Array.isArray(value.sourceKinds)&&value.sourceKinds.includes('field_service_order')
   :!!source&&typeof source==='object'&&'kind'in source&&source.kind==='field_service_order';
  if(native){
   if(r.headers.get('origin')!==requestPublicOrigin(r))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');
   // Forward the original bounded bytes/key, including uncertain retries. No
   // legacy application is called for a native-owned record or policy.
   const data=await nativeFieldOperations(r.headers,holdingSlug,nativeActorContext(a),'POST',body,key);
   return Response.json(canonicalDestinationLinks({data}),{headers});
  }
  if(nativeOperationsAvailable()){
   try{
    const data=await nativeOperationsCommand(a,holdingSlug,body,key);
    if(!data)unavailable();
    return Response.json(canonicalDestinationLinks({data}),{headers});
   }catch(error){
    const mapped=nativeOperationsFailure(error);
    if(mapped)throw new ApiError(mapped.status,mapped.code,mapped.message);
    throw error;
   }
  }
  const data=value.entity==="policy"?typeof value.id==="string"?await reviewQueuePolicy(a,value.id,value,key):await draftQueuePolicy(a,value,key):typeof value.id==="string"?await changeOperationsException(a,value.id,value,key):await openOperationsException(a,value,key);
  return Response.json(canonicalDestinationLinks({data}),{headers});
 }catch(e){return failure(e);}
}
