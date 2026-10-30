import 'server-only';
import {Agent,request} from 'node:http';
import {Readable} from 'node:stream';
import type {ServiceInitialCountReadResponse,ServiceInitialCountCommandResponse} from '@erpx/contracts/field-service-initial-count';

/** Selection and authorization belong to the caller; transport never retries. */
export type NativeCountRoute={kind:'selection';query?:string}|{kind:'record';id:string;query?:string}|{kind:'flow';id:string;query?:string}|{kind:'create'}|{kind:'update';id:string}|{kind:'action';id:string;action:'submit'|'approve'|'post'|'reject'|'cancel'}|{kind:'generic-action';id:string;action:'submit'|'approve'|'post'|'reject'|'cancel'}|{kind:'assign';id:string}|{kind:'generic-assign';id:string};
export type NativeCountRequest={baseUrl:string;slug:string;route:NativeCountRoute;headers:Headers;timeoutMs:number;body?:string};
export type NativeCountReply={status:number;body:unknown};
export type NativeCountTypedResponse=ServiceInitialCountReadResponse|ServiceInitialCountCommandResponse;
export type NativeCountTransportInput=NativeCountRequest&{method:'GET'|'POST';suffix:string};
export type NativeCountTransport=(input:NativeCountTransportInput)=>Promise<NativeCountReply>;
const reserved=new Set([3000,3001,3100,3101,3103,5432,5433,5435,8080]);
const actions=new Set(['submit','approve','post','reject','cancel']);
function idPath(id:string){if(typeof id!=='string'||!id||id==='.'||id==='..'||new TextEncoder().encode(id).length>200)throw Error('NATIVE_INITIAL_COUNT_REQUEST_INVALID');return encodeURIComponent(id);}
function querySuffix(query:string|undefined){const value=query??'';if(value&&(!value.startsWith('?')||/[#\u0000-\u001f\u007f]/.test(value))||new TextEncoder().encode(value).length>2*1024*1024)throw Error('NATIVE_INITIAL_COUNT_REQUEST_INVALID');return value;}
export async function nativeInitialCountRequest(input:NativeCountRequest,transport:NativeCountTransport=nodeTransport):Promise<NativeCountReply>{
 const route=input.route;let suffix:string,method:'GET'|'POST';
 switch(route.kind){
  case 'selection':suffix=querySuffix(route.query);method='GET';break;
  case 'record':suffix=`/${idPath(route.id)}${querySuffix(route.query)}`;method='GET';break;
  case 'flow':suffix=`/${idPath(route.id)}/flow${querySuffix(route.query)}`;method='GET';break;
  case 'create':suffix='';method='POST';break;
  case 'update':suffix=`/${idPath(route.id)}/update`;method='POST';break;
  case 'generic-assign':suffix=`/${idPath(route.id)}/stock-count/flow`;method='POST';break;
  case 'assign':suffix=`/${idPath(route.id)}/flow`;method='POST';break;
  case 'generic-action':if(!actions.has(route.action))throw Error('NATIVE_INITIAL_COUNT_REQUEST_INVALID');suffix=`/${idPath(route.id)}/stock-count/${route.action}`;method='POST';break;
  case 'action':if(!actions.has(route.action))throw Error('NATIVE_INITIAL_COUNT_REQUEST_INVALID');suffix=`/${idPath(route.id)}/${route.action}`;method='POST';break;
  default:throw Error('NATIVE_INITIAL_COUNT_REQUEST_INVALID');
 }
 if(method==='GET'&&input.body!==undefined||method==='POST'&&input.body===undefined)throw Error('NATIVE_INITIAL_COUNT_REQUEST_INVALID');
 return transport({...input,method,suffix});
}
async function nodeTransport(input:NativeCountTransportInput):Promise<NativeCountReply>{
 let origin:URL;try{origin=new URL(input.baseUrl);}catch{throw Error('NATIVE_INITIAL_COUNT_CONFIGURATION_INVALID');}
 const port=Number(origin.port);
 if(origin.protocol!=='http:'||!['127.0.0.1','[::1]'].includes(origin.hostname)||!origin.port||reserved.has(port)||port>=3200&&port<=3299||origin.username||origin.password||origin.origin!==input.baseUrl||!Number.isSafeInteger(input.timeoutMs)||input.timeoutMs<250||input.timeoutMs>30000||!/^[a-z0-9_-]{1,128}$/.test(input.slug))throw Error('NATIVE_INITIAL_COUNT_CONFIGURATION_INVALID');
 if(input.method==='GET'&&(input.body!==undefined)||input.body!==undefined&&new TextEncoder().encode(input.body).length>2*1024*1024)throw Error('NATIVE_INITIAL_COUNT_REQUEST_INVALID');
 const url=new URL(`/api/v1/holdings/${encodeURIComponent(input.slug)}/field-service/initial-physical-stock${input.suffix}`,origin),controller=new AbortController(),timer=setTimeout(()=>controller.abort(),input.timeoutMs);
 try{
  const response=await new Promise<Response>((resolve,reject)=>{const agent=new Agent({keepAlive:false,maxSockets:1,maxTotalSockets:1,proxyEnv:{} as import('node:http').ProxyEnv}),req=request(url,{method:input.method,headers:Object.fromEntries(input.headers),agent,signal:controller.signal,maxHeaderSize:8192},incoming=>{incoming.once('close',()=>agent.destroy());try{const headers=new Headers();for(let i=0;i<incoming.rawHeaders.length;i+=2)headers.append(incoming.rawHeaders[i]!,incoming.rawHeaders[i+1]!);resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>,{status:incoming.statusCode??502,headers}));}catch(error){incoming.destroy();agent.destroy();reject(error);}});req.once('error',error=>{agent.destroy();reject(error);});req.end(input.body);});
  if(response.headers.get('content-type')?.split(';')[0]?.trim()!=='application/json'||!response.body)throw Error('NATIVE_INITIAL_COUNT_RESPONSE_INVALID');
  const reader=response.body.getReader(),chunks:Uint8Array[]=[];let size=0;const cancel=()=>{void reader.cancel().catch(()=>undefined);};controller.signal.addEventListener('abort',cancel,{once:true});try{for(;;){const part=await reader.read();if(controller.signal.aborted)throw Error('NATIVE_INITIAL_COUNT_TIMEOUT');if(part.done)break;size+=part.value.length;if(size>2*1024*1024){await reader.cancel();throw Error('NATIVE_INITIAL_COUNT_RESPONSE_LIMIT');}chunks.push(part.value);}}finally{controller.signal.removeEventListener('abort',cancel);reader.releaseLock();}
  const bytes=new Uint8Array(size);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.length;}
  let body:unknown;try{body=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));}catch{throw Error('NATIVE_INITIAL_COUNT_RESPONSE_INVALID');}
  return {status:response.status,body};
 }finally{clearTimeout(timer);}
}
