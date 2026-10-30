import 'server-only';
import {nativeInitialCount,nativeInitialCountContext,nativeInitialCountSelected,NativeInitialCountError,type NativeInitialCountCommandRoute} from '@erpx/feature-field_service/public/native-initial-count';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {ApiError} from '@erpx/server-runtime/public/http';

/** Read the original command only after authentication and source selection. */
async function body(request:Request):Promise<string>{
 const reader=request.body?.getReader();if(!reader)return '';const chunks:Uint8Array[]=[];let size=0;
 try{for(;;){const part=await reader.read();if(part.done)break;size+=part.value.byteLength;if(size>2*1024*1024){await reader.cancel();throw new ApiError(422,'VALIDATION_FAILED','The Count command exceeds its bounded input.');}chunks.push(part.value);}}finally{reader.releaseLock();}
 const bytes=new Uint8Array(size);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.length;}
 try{return new TextDecoder('utf-8',{fatal:true}).decode(bytes);}catch{throw new ApiError(422,'INVALID_JSON','Use valid JSON.');}
}
export async function serviceCountCommand(request:Request,actor:WorkContext,slug:string,route:NativeInitialCountCommandRoute,key:string){
 const context=nativeInitialCountContext(actor);if(!context)throw new ApiError(409,'WORK_CONTEXT_CHANGED','The work context changed.');
 // Selection and captured work headers must be checked before consuming the body.
 for(const [name,value] of [['x-erpx-context-company',context.companyId],['x-erpx-context-assignment',context.assignmentId],['x-erpx-context-revision',String(context.revision)],['x-erpx-context-assignment-version',String(context.assignmentVersion)]] as const)if(request.headers.get(name)!==value)throw new ApiError(409,'WORK_CONTEXT_CHANGED','The work context changed.');
 try{if(!nativeInitialCountSelected(request.headers,slug,context))throw new ApiError(503,'NATIVE_INITIAL_COUNT_UNAVAILABLE','Service initial physical stock is unavailable.');const raw=await body(request),data=await nativeInitialCount(request.headers,slug,context,route,raw,key);if(!data)throw new ApiError(503,'NATIVE_INITIAL_COUNT_UNAVAILABLE','Service initial physical stock is unavailable.');return {data,context,raw};}catch(error){if(error instanceof NativeInitialCountError)throw new ApiError(error.status,error.code,'Service initial physical stock is unavailable.');throw error;}
}
export async function serviceCountFlow(request:Request,actor:WorkContext,slug:string,id:string,context:NonNullable<ReturnType<typeof nativeInitialCountContext>>){
 try{const data=await nativeInitialCount(request.headers,slug,context,{kind:'flow',id});if(!data)throw new ApiError(503,'NATIVE_INITIAL_COUNT_UNAVAILABLE','Service initial physical stock is unavailable.');return data;}catch(error){if(error instanceof NativeInitialCountError)throw new ApiError(error.status,error.code,'Service initial physical stock is unavailable.');throw error;}
}
