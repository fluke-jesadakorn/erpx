import 'server-only';
import {Agent,request} from 'node:http';
import {Readable} from 'node:stream';

/** Private transport only; selection and authorization remain the caller's responsibility. */
export type NativePartsSetupRequest={baseUrl:string;slug:string;method:'GET'|'POST';existing?:boolean;headers:Headers;body?:string;timeoutMs:number};
export type NativePartsSetupReply={status:number;body:unknown};
const reserved=new Set([3000,3001,3100,3101,3103,5432,5433,5435,8080]);
export type NativePartsPrerequisiteOperation='read'|'chart'|'account'|'period'|'rate';
export type NativePartsPrerequisiteRequest=Omit<NativePartsSetupRequest,'method'|'existing'>&{operation:NativePartsPrerequisiteOperation};
const prerequisitePaths=new Map<NativePartsPrerequisiteOperation,string>([['read','/prerequisites'],['chart','/charts'],['account','/accounts'],['period','/periods'],['rate','/rates']]);
export async function nativePartsPrerequisiteRequest(input:NativePartsPrerequisiteRequest):Promise<NativePartsSetupReply>{
 const suffix=prerequisitePaths.get(input.operation);
 if(suffix===undefined)throw Error('NATIVE_PARTS_SETUP_REQUEST_INVALID');
 return nativePartsTransport({...input,method:input.operation==='read'?'GET':'POST'},suffix);
}
export async function nativePartsSetupRequest(input:NativePartsSetupRequest):Promise<NativePartsSetupReply>{
 return nativePartsTransport(input,input.existing?'/existing':'');
}
async function nativePartsTransport(input:NativePartsSetupRequest,suffix:string):Promise<NativePartsSetupReply>{
 let origin:URL;try{origin=new URL(input.baseUrl);}catch{throw Error('NATIVE_PARTS_SETUP_CONFIGURATION_INVALID');}
 const port=Number(origin.port);
 if(origin.protocol!=='http:'||!['127.0.0.1','[::1]'].includes(origin.hostname)||!origin.port||reserved.has(port)||port>=3200&&port<=3299||origin.username||origin.password||origin.origin!==input.baseUrl||!Number.isSafeInteger(input.timeoutMs)||input.timeoutMs<250||input.timeoutMs>30000||!/^[a-z0-9_-]{1,128}$/.test(input.slug))throw Error('NATIVE_PARTS_SETUP_CONFIGURATION_INVALID');
 if(input.method==='GET'&&(input.existing||input.body!==undefined)||input.body!==undefined&&new TextEncoder().encode(input.body).length>65536)throw Error('NATIVE_PARTS_SETUP_REQUEST_INVALID');
 const url=new URL(`/api/v1/holdings/${encodeURIComponent(input.slug)}/field-service/parts-setup${suffix}`,origin),controller=new AbortController(),timer=setTimeout(()=>controller.abort(),input.timeoutMs);
 try{
  const response=await new Promise<Response>((resolve,reject)=>{const agent=new Agent({keepAlive:false,maxSockets:1,maxTotalSockets:1,proxyEnv:{} as import('node:http').ProxyEnv}),req=request(url,{method:input.method,headers:Object.fromEntries(input.headers),agent,signal:controller.signal,maxHeaderSize:8192},incoming=>{incoming.once('close',()=>agent.destroy());try{const headers=new Headers();for(let i=0;i<incoming.rawHeaders.length;i+=2)headers.append(incoming.rawHeaders[i]!,incoming.rawHeaders[i+1]!);resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>,{status:incoming.statusCode??502,headers}));}catch(error){incoming.destroy();agent.destroy();reject(error);}});req.once('error',error=>{agent.destroy();reject(error);});req.end(input.body);});
  if(response.headers.get('content-type')?.split(';')[0]?.trim()!=='application/json'||!response.body)throw Error('NATIVE_PARTS_SETUP_RESPONSE_INVALID');
  const reader=response.body.getReader(),chunks:Uint8Array[]=[];let size=0;const cancel=()=>{void reader.cancel().catch(()=>undefined);};controller.signal.addEventListener('abort',cancel,{once:true});try{for(;;){const part=await reader.read();if(controller.signal.aborted)throw Error('NATIVE_PARTS_SETUP_TIMEOUT');if(part.done)break;size+=part.value.length;if(size>2*1024*1024){await reader.cancel();throw Error('NATIVE_PARTS_SETUP_RESPONSE_LIMIT');}chunks.push(part.value);}}finally{controller.signal.removeEventListener('abort',cancel);reader.releaseLock();}
  const bytes=new Uint8Array(size);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.length;}
  let body:unknown;try{body=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));}catch{throw Error('NATIVE_PARTS_SETUP_RESPONSE_INVALID');}
  return {status:response.status,body};
 }finally{clearTimeout(timer);}
}
