import 'server-only';
import {allowedPrivateLoopbackPort} from '@erpx/platform-contracts/private-loopback';
import {nativeHoldingConfiguration,type NativeHoldingConfiguration} from './holding-target';
import {Agent,request} from 'node:http';
import {Readable} from 'node:stream';

/** Private transport for native Benefits evidence. It never authorizes a read
 * or retries a failed owner response; composition owns those decisions. */
export type NativeBenefitsEvidenceRequest={
 baseUrl:string;
 slug:string;
 headers:Headers;
 timeoutMs:number;
 action:'search'|'open';
 body:string;
 production?:boolean;
};
export type NativeBenefitsEvidenceReply={status:number;body:unknown};
export {nativeHoldingConfiguration};
export type {NativeHoldingConfiguration};

export async function nativeBenefitsEvidenceRequest(input:NativeBenefitsEvidenceRequest):Promise<NativeBenefitsEvidenceReply>{
 let base:URL;try{base=new URL(input.baseUrl);}catch{throw Error('NATIVE_BENEFITS_EVIDENCE_CONFIGURATION_INVALID');}
 const port=Number(base.port);
 if(base.protocol!=='http:'||!['127.0.0.1','[::1]'].includes(base.hostname)||!base.port||!allowedPrivateLoopbackPort(port,input.production===true)||base.username||base.password||base.origin!==input.baseUrl)throw Error('NATIVE_BENEFITS_EVIDENCE_CONFIGURATION_INVALID');
 if(!/^[a-z0-9_-]{1,128}$/.test(input.slug)||!['search','open'].includes(input.action)||new TextEncoder().encode(input.body).length>64*1024)throw Error('NATIVE_BENEFITS_EVIDENCE_REQUEST_INVALID');
 const url=new URL(`/api/v1/holdings/${encodeURIComponent(input.slug)}/benefits/executive-evidence/${input.action}`,base);
 const controller=new AbortController(),timer=setTimeout(()=>controller.abort(),input.timeoutMs);
 try{
  const response=await new Promise<Response>((resolve,reject)=>{
   const agent=new Agent({keepAlive:false,maxSockets:1,maxTotalSockets:1,proxyEnv:{} as import('node:http').ProxyEnv});
   const req=request(url,{method:'POST',headers:Object.fromEntries(input.headers),agent,signal:controller.signal,maxHeaderSize:8192},incoming=>{
    incoming.once('close',()=>agent.destroy());
    try{
     const headers=new Headers();
     for(let index=0;index<incoming.rawHeaders.length;index+=2)headers.append(incoming.rawHeaders[index]!,incoming.rawHeaders[index+1]!);
     resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>,{status:incoming.statusCode??502,headers}));
    }catch(error){incoming.destroy();agent.destroy();reject(error);}
   });
   req.once('error',error=>{agent.destroy();reject(error);});
   req.end(input.body);
  });
  if(response.headers.get('content-type')?.split(';')[0]?.trim()!=='application/json'||!response.body)throw Error('NATIVE_BENEFITS_EVIDENCE_RESPONSE_INVALID');
  const reader=response.body.getReader(),chunks:Uint8Array[]=[];let size=0;
  const cancel=()=>{void reader.cancel().catch(()=>undefined);};
  controller.signal.addEventListener('abort',cancel,{once:true});
  try{for(;;){const part=await reader.read();if(controller.signal.aborted)throw Error('NATIVE_BENEFITS_EVIDENCE_TIMEOUT');if(part.done)break;size+=part.value.length;if(size>2*1024*1024){await reader.cancel();throw Error('NATIVE_BENEFITS_EVIDENCE_RESPONSE_LIMIT');}chunks.push(part.value);}}
  finally{controller.signal.removeEventListener('abort',cancel);reader.releaseLock();}
  const bytes=new Uint8Array(size);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.length;}
  let body:unknown;try{body=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));}catch{throw Error('NATIVE_BENEFITS_EVIDENCE_RESPONSE_INVALID');}
  return {status:response.status,body};
 }finally{clearTimeout(timer);}
}
