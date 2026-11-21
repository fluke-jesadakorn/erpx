import 'server-only';
import {Agent,request} from 'node:http';
import {Readable} from 'node:stream';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {ExternalConnections} from '@erpx/feature-shared/lib/executive/connections';

/**
 * Native Executive external-connection client. The mounted Rust router is the
 * only create/list/revoke authority; without its private configuration every
 * call fails closed instead of falling back to a TypeScript writer.
 */
export type NativeExecutiveConnection = {id:string;secret:string;expiresAt:number;scope:string;role:string};
export type NativeExecutiveConnections = ExternalConnections;

type Holding={origin:string;holdingSlug:string;holdingId:string};
type Settings={privateBaseUrl:string;timeoutMs:number;holdings:Holding[];production?:boolean};
type Transport=(url:URL,method:'GET'|'POST',headers:Headers,body:string|undefined,signal:AbortSignal)=>Promise<Response>;
const reserved=new Set([3000,3001,3100,3103,5432,5433,5435,8080]);
const allowed=(port:number,production:boolean)=>Number.isInteger(port)&&port>=1&&port<=65535&&!(port>=3200&&port<=3299)&&!reserved.has(port)&&(production||port!==3101);
const fail=(code:string,status=503):never=>{throw new ApiError(status,code,'The native Executive connection service is unavailable.');};
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
function configuration(raw:string):Settings{
 let value:unknown;try{value=JSON.parse(raw);}catch{return fail('NATIVE_EXECUTIVE_CONNECTIONS_CONFIGURATION_INVALID');}
 if(!object(value)||(value.production!==undefined&&typeof value.production!=='boolean')||typeof value.privateBaseUrl!=='string'||!Number.isSafeInteger(value.timeoutMs)||Number(value.timeoutMs)<250||Number(value.timeoutMs)>30000||!Array.isArray(value.holdings)||!value.holdings.length||value.holdings.length>64)return fail('NATIVE_EXECUTIVE_CONNECTIONS_CONFIGURATION_INVALID');
 let base:URL;try{base=new URL(value.privateBaseUrl);}catch{return fail('NATIVE_EXECUTIVE_CONNECTIONS_CONFIGURATION_INVALID');}
 const port=Number(base.port);
 if(base.protocol!=='http:'||!['127.0.0.1','[::1]'].includes(base.hostname)||!base.port||!allowed(port,value.production===true)||base.origin!==value.privateBaseUrl||base.username||base.password)return fail('NATIVE_EXECUTIVE_CONNECTIONS_CONFIGURATION_INVALID');
 const hosts=new Set<string>(),slugs=new Set<string>(),ids=new Set<string>();
 for(const holding of value.holdings){
  if(!object(holding)||typeof holding.origin!=='string'||typeof holding.holdingSlug!=='string'||typeof holding.holdingId!=='string'||!holding.holdingId||!/^[a-z0-9_-]{1,128}$/.test(holding.holdingSlug))return fail('NATIVE_EXECUTIVE_CONNECTIONS_CONFIGURATION_INVALID');
  let origin:URL;try{origin=new URL(holding.origin);}catch{return fail('NATIVE_EXECUTIVE_CONNECTIONS_CONFIGURATION_INVALID');}
  if(origin.protocol!=='https:'||origin.origin!==holding.origin||origin.username||origin.password||hosts.has(origin.host)||slugs.has(holding.holdingSlug)||ids.has(holding.holdingId))return fail('NATIVE_EXECUTIVE_CONNECTIONS_CONFIGURATION_INVALID');
  hosts.add(origin.host);slugs.add(holding.holdingSlug);ids.add(holding.holdingId);
 }
 return value as Settings;
}
function direct(url:URL,method:'GET'|'POST',headers:Headers,body:string|undefined,signal:AbortSignal):Promise<Response>{
 return new Promise((resolve,reject)=>{
  const agent=new Agent({keepAlive:false,maxSockets:1,maxTotalSockets:1,proxyEnv:{} as import('node:http').ProxyEnv});
  const req=request(url,{method,agent,headers:Object.fromEntries(headers),signal,maxHeaderSize:8192},incoming=>{
   const chunks:Uint8Array[]=[];let total=0;
   incoming.on('data',(chunk:Uint8Array)=>{total+=chunk.byteLength;if(total>1024*1024){incoming.destroy();reject(new Error('NATIVE_EXECUTIVE_CONNECTIONS_RESPONSE_LIMIT'));return;}chunks.push(chunk);});
   incoming.on('end',()=>{const bytes=new Uint8Array(total);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.byteLength;}resolve(new Response(bytes,{status:incoming.statusCode??0,headers:new Headers(Object.entries(incoming.headers).flatMap(([key,value])=>value===undefined?[]:[[key,Array.isArray(value)?value.join(', '):value]]))}));});
   incoming.on('error',reject);
  });
  req.on('error',reject);
  if(body===undefined)req.end();else Readable.from([body]).pipe(req);
 });
}
const forwarded=['cookie','x-erpx-context-company','x-erpx-context-assignment','x-erpx-context-revision','x-erpx-context-assignment-version','x-erpx-workspace-branch','x-erpx-workspace-department','origin'] as const;
export function createNativeExecutiveConnections(raw:string|null,transport:Transport=direct){
 const settings=raw?configuration(raw):fail('NATIVE_EXECUTIVE_CONNECTIONS_CONFIGURATION_REQUIRED');
 const call=async(incoming:Pick<Headers,'get'>,slug:string,method:'GET'|'POST',body:string|undefined,key?:string):Promise<unknown>=>{
  if(body!==undefined&&body.length>4096)return fail('BODY_TOO_LARGE',413);
  const host=incoming.get('host');
  const target=settings.holdings.find(h=>new URL(h.origin).host===host&&h.holdingSlug===slug);
  if(!target)return fail('NATIVE_EXECUTIVE_CONNECTIONS_AUTHORITY_INVALID');
  const out=new Headers({host:host!,accept:'application/json'});
  if(method==='POST'){out.set('content-type','application/json');if(key)out.set('idempotency-key',key);}
  for(const name of forwarded){const value=incoming.get(name);if(value)out.set(name,value);}
  const url=new URL(`/api/v1/holdings/${encodeURIComponent(slug)}/executive/connections`,settings.privateBaseUrl);
  const controller=new AbortController();const timer=setTimeout(()=>controller.abort(),Number(settings.timeoutMs));
  try{
   const response=await transport(url,method,out,body,controller.signal);
   if(response.status!==200){
    const payload=await response.json().catch(()=>null);
    const detail=object(payload)&&object(payload.error)?payload.error:null;
    const code=detail&&typeof detail.code==='string'?detail.code:'NATIVE_EXECUTIVE_CONNECTIONS_UNAVAILABLE';
    return fail(code,response.status>=400&&response.status<=599?response.status:503);
   }
   const value=await response.json().catch(()=>null);
   if(!object(value)||!object(value.data))return fail('NATIVE_EXECUTIVE_CONNECTIONS_RESPONSE_INVALID');
   return value.data;
  }catch(error){if(error instanceof ApiError)throw error;return fail(controller.signal.aborted?'NATIVE_EXECUTIVE_CONNECTIONS_ABORTED':'NATIVE_EXECUTIVE_CONNECTIONS_UNAVAILABLE');}finally{clearTimeout(timer);}
 };
 return {
  list:(incoming:Pick<Headers,'get'>,slug:string)=>call(incoming,slug,'GET',undefined) as Promise<NativeExecutiveConnections>,
  command:(incoming:Pick<Headers,'get'>,slug:string,body:string,key:string)=>call(incoming,slug,'POST',body,key) as Promise<NativeExecutiveConnection|{id:string}>,
 };
}
/** Retained per-request MCP budget. The bearer secret authenticates the call. */
export async function consumeNativeExecutiveRateLimit(authorization:string|null,rawConfig=process.env.ERPX_NATIVE_EXECUTIVE_CONNECTIONS_CONFIG,transport:Transport=direct):Promise<void>{
 const settings=rawConfig?configuration(rawConfig):fail('NATIVE_EXECUTIVE_CONNECTIONS_CONFIGURATION_REQUIRED');
 if(!authorization)return fail('INVALID_TOKEN',401);
 const url=new URL('/api/v1/executive/mcp/rate-limit',settings.privateBaseUrl);
 const controller=new AbortController();const timer=setTimeout(()=>controller.abort(),Number(settings.timeoutMs));
 try{
  const response=await transport(url,'POST',new Headers({authorization,'content-type':'application/json'}),'{}',controller.signal);
  if(response.status===200)return;
  const payload=await response.json().catch(()=>null);
  const detail=object(payload)&&object(payload.error)?payload.error:null;
  const code=detail&&typeof detail.code==='string'?detail.code:(response.status===429?'RATE_LIMITED':'NATIVE_EXECUTIVE_CONNECTIONS_UNAVAILABLE');
  fail(code,response.status===401?401:response.status===429?429:503);
 }catch(error){if(error instanceof ApiError)throw error;fail(controller.signal.aborted?'NATIVE_EXECUTIVE_CONNECTIONS_ABORTED':'NATIVE_EXECUTIVE_CONNECTIONS_UNAVAILABLE');}finally{clearTimeout(timer);}
}
