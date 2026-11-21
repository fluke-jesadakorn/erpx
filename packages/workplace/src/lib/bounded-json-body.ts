import {ApiError} from '@erpx/server-runtime/public/http';

/** Read once after authorization; retain the exact raw bytes for owner dispatch. */
export async function boundedJsonBody(request:Request,limit:number):Promise<{raw:Uint8Array;value:unknown}>{
 const reader=request.body?.getReader(),chunks:Uint8Array[]=[];let size=0;
 if(reader){try{for(;;){const part=await reader.read();if(part.done)break;size+=part.value.byteLength;if(size>limit){await reader.cancel('JSON_BODY_LIMIT').catch(()=>undefined);throw new ApiError(413,'JSON_BODY_LIMIT','The request body is too large.');}chunks.push(part.value);}}finally{reader.releaseLock();}}
 const raw=new Uint8Array(size);let offset=0;for(const chunk of chunks){raw.set(chunk,offset);offset+=chunk.byteLength;}
 return {raw,value:JSON.parse(new TextDecoder().decode(raw))};
}
