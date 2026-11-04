import 'server-only';
import {Agent,request} from 'node:http';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {supportDetail,supportRecipients,supportWorkspace} from '@erpx/server-support/public/support';

export type NativeSupportWorkspace = Awaited<ReturnType<typeof supportWorkspace>>;
export type NativeSupportDetail = Awaited<ReturnType<typeof supportDetail>>;
export type NativeSupportRecipients = Awaited<ReturnType<typeof supportRecipients>>;

type Settings={privateBaseUrl:string;timeoutMs:number;holdings:{origin:string;holdingSlug:string;holdingId:string}[];production?:boolean};
export type SupportReadTransport=(url:URL,headers:Headers,signal:AbortSignal)=>Promise<Response>;
const reserved=new Set([3000,3001,3100,3103,5432,5433,5435,8080]);
const allowed=(port:number,production:boolean)=>Number.isInteger(port)&&port>=1&&port<=65535&&!(port>=3200&&port<=3299)&&!reserved.has(port)&&(production||port!==3101);
const fail=(code:string,status=503):never=>{throw new ApiError(status,code,'The native Support reader is unavailable.');};
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
function configuration(raw:string):Settings{
 let value:unknown;try{value=JSON.parse(raw);}catch{return fail('NATIVE_SUPPORT_READ_CONFIGURATION_INVALID');}
 if(!object(value)||(value.production!==undefined&&typeof value.production!=='boolean')||typeof value.privateBaseUrl!=='string'||!Number.isSafeInteger(value.timeoutMs)||Number(value.timeoutMs)<250||Number(value.timeoutMs)>30000||!Array.isArray(value.holdings)||!value.holdings.length||value.holdings.length>64)return fail('NATIVE_SUPPORT_READ_CONFIGURATION_INVALID');
 let base:URL;try{base=new URL(value.privateBaseUrl);}catch{return fail('NATIVE_SUPPORT_READ_CONFIGURATION_INVALID');}
 const port=Number(base.port);
 if(base.protocol!=='http:'||!['127.0.0.1','[::1]'].includes(base.hostname)||!base.port||!allowed(port,value.production===true)||base.origin!==value.privateBaseUrl||base.username||base.password)return fail('NATIVE_SUPPORT_READ_CONFIGURATION_INVALID');
 const hosts=new Set<string>(),slugs=new Set<string>();
 for(const holding of value.holdings){
  if(!object(holding)||typeof holding.origin!=='string'||typeof holding.holdingSlug!=='string'||typeof holding.holdingId!=='string'||!holding.holdingId||!/^[a-z0-9_-]{1,128}$/.test(holding.holdingSlug))return fail('NATIVE_SUPPORT_READ_CONFIGURATION_INVALID');
  let origin:URL;try{origin=new URL(holding.origin);}catch{return fail('NATIVE_SUPPORT_READ_CONFIGURATION_INVALID');}
  if(origin.protocol!=='https:'||origin.origin!==holding.origin||origin.username||origin.password||hosts.has(origin.host)||slugs.has(holding.holdingSlug))return fail('NATIVE_SUPPORT_READ_CONFIGURATION_INVALID');
  hosts.add(origin.host);slugs.add(holding.holdingSlug);
 }
 return value as Settings;
}
function direct(url:URL,headers:Headers,signal:AbortSignal):Promise<Response>{
 return new Promise((resolve,reject)=>{
  const agent=new Agent({keepAlive:false,maxSockets:1,maxTotalSockets:1,proxyEnv:{} as import('node:http').ProxyEnv});
  const req=request(url,{method:'GET',agent,headers:Object.fromEntries(headers),signal,maxHeaderSize:8192},incoming=>{
   const chunks:Uint8Array[]=[];let total=0;
   incoming.on('data',(chunk:Uint8Array)=>{total+=chunk.byteLength;if(total>4*1024*1024){incoming.destroy();reject(new Error('NATIVE_SUPPORT_READ_RESPONSE_LIMIT'));return;}chunks.push(chunk);});
   incoming.on('end',()=>{const bytes=new Uint8Array(total);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.byteLength;}resolve(new Response(bytes,{status:incoming.statusCode??0,headers:new Headers(Object.entries(incoming.headers).flatMap(([key,value])=>value===undefined?[]:[[key,Array.isArray(value)?value.join(', '):value]]))}));});
   incoming.on('error',reject);
  });
  req.on('error',reject);
  req.end();
 });
}
export function createNativeSupportReader(raw:string,transport:SupportReadTransport=direct){
 const settings=configuration(raw);
 const read=async(incoming:Pick<Headers,'get'>,slug:string,search:string):Promise<unknown>=>{
  const host=incoming.get('host');
  const target=settings.holdings.find(h=>new URL(h.origin).host===host&&h.holdingSlug===slug);
  if(!target)return fail('NATIVE_SUPPORT_READ_AUTHORITY_INVALID');
  const out=new Headers({host:host!,accept:'application/json'});
  for(const name of ['cookie','x-erpx-context-company','x-erpx-context-assignment','x-erpx-context-revision','x-erpx-context-assignment-version','x-erpx-workspace-branch','x-erpx-workspace-department','origin']){const value=incoming.get(name);if(value)out.set(name,value);}
  const url=new URL(`/api/v1/holdings/${encodeURIComponent(slug)}/support${search}`,settings.privateBaseUrl);
  const controller=new AbortController();const timer=setTimeout(()=>controller.abort(),Number(settings.timeoutMs));
  try{
   const response=await transport(url,out,controller.signal);
   if(response.status!==200){
    const payload=await response.json().catch(()=>null);
    const detail=object(payload)&&object(payload.error)?payload.error:null;
    const code=detail&&typeof detail.code==='string'?detail.code:'NATIVE_SUPPORT_READ_UNAVAILABLE';
    return fail(code,response.status>=400&&response.status<=599?response.status:503);
   }
   const value=await response.json().catch(()=>null);
   if(!object(value)||!(Array.isArray(value.data)||object(value.data)))return fail('NATIVE_SUPPORT_READ_RESPONSE_INVALID');
   return value.data;
  }catch(error){if(error instanceof ApiError)throw error;return fail(controller.signal.aborted?'NATIVE_SUPPORT_READ_ABORTED':'NATIVE_SUPPORT_READ_UNAVAILABLE');}finally{clearTimeout(timer);}
 };
 return {
  workspace:(incoming:Pick<Headers,'get'>,slug:string)=>read(incoming,slug,'') as Promise<NativeSupportWorkspace>,
  detail:(incoming:Pick<Headers,'get'>,slug:string,id:string)=>read(incoming,slug,`?id=${encodeURIComponent(id)}`) as Promise<NativeSupportDetail>,
  recipients:(incoming:Pick<Headers,'get'>,slug:string,orderId:string)=>read(incoming,slug,`?orderId=${encodeURIComponent(orderId)}`) as Promise<NativeSupportRecipients>,
 };
}
