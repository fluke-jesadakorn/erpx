import {nativeInitialCount,nativeInitialCountSelected,NativeInitialCountError,type NativeInitialCountReadRoute,type NativeInitialCountCommandRoute} from '@erpx/feature-field_service/public/native-initial-count';
import {partsSetupContext} from '../../../../../../../../lib/parts-setup-context';
import {requireWorkContext} from '@erpx/server-identity_access/public/authorization';
import {requestPublicOrigin} from '@erpx/server-manage_erpx/public/public-origin';
import {ApiError,errorResponse} from '@erpx/server-runtime/public/http';
type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
const headers={'Cache-Control':'private, no-store'};
const missing=()=>new ApiError(404,'NOT_FOUND','Service initial physical stock route not found.');
function failure(error:unknown){const mapped=error instanceof NativeInitialCountError?new ApiError(error.status,error.code,'Service initial physical stock is unavailable.'):error instanceof SyntaxError?new ApiError(422,'INVALID_JSON','Use valid JSON.'):error,response=errorResponse(mapped);response.headers.set('Cache-Control',headers['Cache-Control']);return response;}
const actions=['submit','approve','post','reject','cancel'] as const;
type ServiceInitialPhysicalStockCountAction=typeof actions[number];
const isAction=(value:string):value is ServiceInitialPhysicalStockCountAction=>actions.some(action=>action===value);
export async function GET(request:Request,context:Context){
 try{
  const {holdingSlug,path=[]}=await context.params,actor=await requireWorkContext(holdingSlug),url=new URL(request.url);if(path[0]==='masters')throw missing();
  const nativeContext=partsSetupContext(actor);
  {
   const query=url.search;let route:NativeInitialCountReadRoute;
   if(path.length===0)route={kind:'selection',query};else if(path.length===1)route={kind:'record',id:path[0],query};else if(path.length===2&&path[1]==='flow')route={kind:'flow',id:path[0],query};else throw missing();
   const data=await nativeInitialCount(request.headers,holdingSlug,nativeContext,route);if(data===undefined)throw new ApiError(503,'NATIVE_INITIAL_COUNT_UNAVAILABLE','Service initial physical stock is unavailable.');
   return Response.json({data},{headers});
  }

 }catch(error){return failure(error);}
}
export async function POST(request:Request,context:Context){
 try{
  const origin=request.headers.get('origin');if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin for this action.');
  const {holdingSlug,path=[]}=await context.params,actor=await requireWorkContext(holdingSlug);
  if(!(path.length===0||path[0]!=='masters'&&path.length===2&&(path[1]==='flow'||isAction(path[1]))))throw missing();
  const key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');
  const nativeContext=partsSetupContext(actor);
  {
   if(!nativeInitialCountSelected(request.headers,holdingSlug,nativeContext))throw new ApiError(503,'NATIVE_INITIAL_COUNT_UNAVAILABLE','Service initial physical stock is unavailable.');
   if(!nativeContext)throw new ApiError(409,'WORK_CONTEXT_CHANGED','The work context changed.');
   for(const [name,value] of [['x-erpx-context-company',nativeContext.companyId],['x-erpx-context-assignment',nativeContext.assignmentId],['x-erpx-context-revision',String(nativeContext.revision)],['x-erpx-context-assignment-version',String(nativeContext.assignmentVersion)]] as const)if(request.headers.get(name)!==value)throw new ApiError(409,'WORK_CONTEXT_CHANGED','The work context changed.');
   const route:NativeInitialCountCommandRoute=path.length===0?{kind:'create'}:path[1]==='flow'?{kind:'assign',id:path[0]}:{kind:'action',id:path[0],action:path[1] as ServiceInitialPhysicalStockCountAction};
   const data=await nativeInitialCount(request.headers,holdingSlug,nativeContext,route,await nativeBody(request),key);if(data===undefined)throw new ApiError(503,'NATIVE_INITIAL_COUNT_UNAVAILABLE','Service initial physical stock is unavailable.');
   return Response.json({data},{status:path.length===0?201:200,headers});
  }
 }catch(error){return failure(error);}
}

/** Bound the BFF read without parsing or reserializing the captured command. */
async function nativeBody(request:Request):Promise<string>{
 const reader=request.body?.getReader();if(!reader)return '';const chunks:Uint8Array[]=[];let length=0;
 try{for(;;){const value=await reader.read();if(value.done)break;length+=value.value.byteLength;if(length>2*1024*1024){await reader.cancel();throw new ApiError(422,'VALIDATION_FAILED','The Count command exceeds its bounded input.');}chunks.push(value.value);}}finally{reader.releaseLock();}
 const bytes=new Uint8Array(length);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.byteLength;}return new TextDecoder().decode(bytes);
}
