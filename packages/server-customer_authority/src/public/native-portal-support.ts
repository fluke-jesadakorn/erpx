import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
type HoldingConfig=Readonly<{origin:string;portalOrigin?:string;holdingSlug:string;holdingId:string}>;
type Config=Readonly<{privateBaseUrl:string;holdings:readonly HoldingConfig[];timeoutMs:number}>;
type Receipt=Readonly<{id:string;requestId:string;bindingId:string;version:number;status:string;publishedAt:number}>;
const reserved=new Set([3000,3001,3100,3101,3103,5432,5433,5435,8080]);
const fail=(code:string,status=503,message='The native portal support service is unavailable.'):never=>{throw new ApiError(status,code,message)};
function config(raw:Config){
 let base:URL;try{base=new URL(raw.privateBaseUrl)}catch{return fail('NATIVE_PORTAL_SUPPORT_CONFIGURATION_INVALID')}
 const port=Number(base.port);
 if(base.protocol!=='http:'||!['127.0.0.1','[::1]'].includes(base.hostname)||!base.port||reserved.has(port)||(port>=3200&&port<=3299)||base.username||base.password||raw.privateBaseUrl!==base.origin||!Number.isInteger(raw.timeoutMs)||raw.timeoutMs<250||raw.timeoutMs>30000||raw.holdings.length<1||raw.holdings.length>64)return fail('NATIVE_PORTAL_SUPPORT_CONFIGURATION_INVALID');
 const holdings=new Map<string,{holdingId:string;holdingSlug:string}>(),portals=new Map<string,{holdingId:string;holdingSlug:string}>(),portalKey=(host:string,slug:string)=>`${host}|${slug}`;
 for(const h of raw.holdings){
  let o:URL;try{o=new URL(h.origin)}catch{return fail('NATIVE_PORTAL_SUPPORT_CONFIGURATION_INVALID')}
  if(o.protocol!=='https:'||h.origin!==o.origin||o.port||o.username||o.password||!h.holdingId||!h.holdingSlug||!/^[a-z0-9_-]+$/.test(h.holdingSlug)||holdings.has(o.host))return fail('NATIVE_PORTAL_SUPPORT_CONFIGURATION_INVALID');
  holdings.set(o.host,{holdingId:h.holdingId,holdingSlug:h.holdingSlug});
  if(h.portalOrigin!==undefined){
   let p:URL;try{p=new URL(h.portalOrigin)}catch{return fail('NATIVE_PORTAL_SUPPORT_CONFIGURATION_INVALID')}
   const key=portalKey(p.host,h.holdingSlug);
   if(p.protocol!=='https:'||h.portalOrigin!==p.origin||p.port||p.username||p.password||portals.has(key))return fail('NATIVE_PORTAL_SUPPORT_CONFIGURATION_INVALID');
   portals.set(key,{holdingId:h.holdingId,holdingSlug:h.holdingSlug});
  }
 }
 return{base,holdings,portals,timeout:raw.timeoutMs}
}
async function call(actor:WorkContext,requestId:string,path:'respond'|'close',input:unknown,key:string,incoming:Pick<Headers,'get'>):Promise<Receipt>{const raw=process.env.ERPX_NATIVE_PORTAL_SUPPORT_CONFIG;if(raw===undefined)throw new ApiError(503,'NATIVE_PORTAL_SUPPORT_UNAVAILABLE','The native portal support service is unavailable.');let parsed:Config;try{parsed=JSON.parse(raw) as Config}catch{return fail('NATIVE_PORTAL_SUPPORT_CONFIGURATION_INVALID')}const c=config(parsed),host=incoming.get('host'),holding=host?c.holdings.get(host):undefined;if(!holding||holding.holdingSlug!==actor.holdingSlug||holding.holdingId!==actor.holdingId)return fail('NATIVE_PORTAL_SUPPORT_AUTHORITY_INVALID');if(!actor.authority||!actor.iam)return fail('NATIVE_PORTAL_SUPPORT_CONTEXT_CHANGED',409);if(!/^[A-Za-z0-9:_.-]{8,200}$/.test(key))return fail('COMMAND_KEY_REQUIRED',422);const headers=new Headers({host:host!,accept:'application/json','content-type':'application/json','cache-control':'no-cache','idempotency-key':key,'x-erpx-context-company':actor.companyId,'x-erpx-context-assignment':actor.assignmentId,'x-erpx-context-revision':String(actor.authority.revision),'x-erpx-context-assignment-version':String(actor.authority.assignmentVersion)});const cookie=incoming.get('cookie');if(cookie)headers.set('cookie',cookie);const controller=new AbortController(),timer=setTimeout(()=>controller.abort(),c.timeout);try{const response=await transport('POST',new URL(`/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/customer-access/support/${encodeURIComponent(requestId)}/${path}`,c.base),headers,new TextEncoder().encode(JSON.stringify(input)),controller.signal);const bytes=new Uint8Array(await response.arrayBuffer());if(bytes.byteLength>64*1024)fail('NATIVE_PORTAL_SUPPORT_RESPONSE_LIMIT');if(response.status!==200){let code='NATIVE_PORTAL_SUPPORT_UNAVAILABLE',message='The native portal support service is unavailable.';try{const value=JSON.parse(new TextDecoder().decode(bytes));if(value?.error){if(typeof value.error.code==='string')code=value.error.code;if(typeof value.error.message==='string')message=value.error.message}}catch{}throw new ApiError(response.status,code,message)}const value=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));if(!value||Object.keys(value).length!==2||!value.data||!value.context)fail('NATIVE_PORTAL_SUPPORT_RESPONSE_INVALID');const ctx=value.context;if(ctx.userId!==actor.userId||ctx.companyId!==actor.companyId||ctx.assignmentId!==actor.assignmentId||ctx.revision!==actor.authority.revision||ctx.assignmentVersion!==actor.authority.assignmentVersion)fail('NATIVE_PORTAL_SUPPORT_CONTEXT_CHANGED',409);const r=value.data;if(typeof r.id!=='string'||typeof r.requestId!=='string'||typeof r.bindingId!=='string'||!Number.isInteger(r.version)||typeof r.status!=='string'||!Number.isInteger(r.publishedAt))fail('NATIVE_PORTAL_SUPPORT_RESPONSE_INVALID');return r as Receipt}finally{clearTimeout(timer)}}
export function nativePublishPortalSupportResponse(actor:WorkContext,requestId:string,input:Record<string,unknown>,key:string,incoming:Pick<Headers,'get'>){return call(actor,requestId,'respond',input,key,incoming)}
export function nativeClosePortalSupportRequest(actor:WorkContext,requestId:string,input:Record<string,unknown>,key:string,incoming:Pick<Headers,'get'>){return call(actor,requestId,'close',input,key,incoming)}
type PortalCustomerActor=Readonly<{userId:string;email:string;holdingId:string;holdingSlug:string}>;
type Acknowledgement=Readonly<{id:string;requestId:string;bindingId:string;responseId:string;responseVersion:number;status:'acknowledged';createdAt:number}>;
/** Customer acknowledgement through the verified portal-product session. The
 * native service re-proves the session before and after its deferred effects;
 * no account/work-context headers are accepted here. */
export async function nativeAcknowledgePortalSupportResponse(actor:PortalCustomerActor,requestId:string,input:unknown,key:string,incoming:Pick<Headers,'get'>):Promise<Acknowledgement>{
 const raw=process.env.ERPX_NATIVE_PORTAL_SUPPORT_CONFIG;if(raw===undefined)throw new ApiError(503,'NATIVE_PORTAL_SUPPORT_UNAVAILABLE','The native portal support service is unavailable.');
 let parsed:Config;try{parsed=JSON.parse(raw) as Config}catch{return fail('NATIVE_PORTAL_SUPPORT_CONFIGURATION_INVALID')}
 const c=config(parsed),host=incoming.get('host'),holding=host?c.portals.get(`${host}|${actor.holdingSlug}`):undefined;
 if(!holding||holding.holdingSlug!==actor.holdingSlug||holding.holdingId!==actor.holdingId)return fail('NATIVE_PORTAL_SUPPORT_AUTHORITY_INVALID');
 if(!/^[A-Za-z0-9:_.-]{8,200}$/.test(key))return fail('COMMAND_KEY_REQUIRED',422);
 const headers=new Headers({host:host!,accept:'application/json','content-type':'application/json','cache-control':'no-cache','idempotency-key':key});
 const cookie=incoming.get('cookie');if(cookie)headers.set('cookie',cookie);
 const origin=incoming.get('origin');if(origin)headers.set('origin',origin);
 const controller=new AbortController(),timer=setTimeout(()=>controller.abort(),c.timeout);
 try{
  const response=await transport('POST',new URL(`/api/v1/portals/${encodeURIComponent(actor.holdingSlug)}/customer-access/support/${encodeURIComponent(requestId)}/acknowledge`,c.base),headers,new TextEncoder().encode(JSON.stringify(input)),controller.signal);
  const bytes=new Uint8Array(await response.arrayBuffer());if(bytes.byteLength>64*1024)fail('NATIVE_PORTAL_SUPPORT_RESPONSE_LIMIT');
  if(response.status!==200){let code='NATIVE_PORTAL_SUPPORT_UNAVAILABLE',message='The native portal support service is unavailable.';try{const value=JSON.parse(new TextDecoder().decode(bytes));if(value?.error){if(typeof value.error.code==='string')code=value.error.code;if(typeof value.error.message==='string')message=value.error.message}}catch{}throw new ApiError(response.status,code,message)}
  const value=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));
  if(!value||Object.keys(value).length!==2||!value.data||!value.context)return fail('NATIVE_PORTAL_SUPPORT_RESPONSE_INVALID');
  const ctx=value.context;if(ctx.userId!==actor.userId||typeof ctx.email!=='string'||ctx.email.toLowerCase()!==actor.email.toLowerCase()||ctx.holdingId!==actor.holdingId)return fail('NATIVE_PORTAL_SUPPORT_SESSION_CHANGED',409);
  const expected=(input&&typeof input==='object'?input:null) as {bindingId?:unknown;responseId?:unknown;responseVersion?:unknown}|null;
  const r=value.data;if(Object.keys(r).length!==7||r.requestId!==requestId||r.bindingId!==expected?.bindingId||r.responseId!==expected?.responseId||r.responseVersion!==expected?.responseVersion||r.status!=='acknowledged'||typeof r.id!=='string'||!r.id||!Number.isInteger(r.createdAt)||r.createdAt<0)return fail('NATIVE_PORTAL_SUPPORT_RESPONSE_INVALID');
  return r as Acknowledgement;
 }finally{clearTimeout(timer)}
}
function transport(_method:'POST',url:URL,headers:Headers,body:Uint8Array,signal:AbortSignal):Promise<Response>{return new Promise((resolve,reject)=>{const agent=new Agent({keepAlive:false,maxSockets:1,maxTotalSockets:1,proxyEnv:{} as import('node:http').ProxyEnv});const outgoing=request(url,{method:'POST',agent,headers:{...Object.fromEntries(headers),'content-length':String(body.byteLength)},signal,maxHeaderSize:8192},(incoming)=>{incoming.once('close',()=>agent.destroy());try{const status=incoming.statusCode??502,values=new Headers();for(let i=0;i<incoming.rawHeaders.length;i+=2)values.append(incoming.rawHeaders[i]!,incoming.rawHeaders[i+1]!);resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>,{status:status>=200&&status<=599?status:502,headers:values}))}catch(error){incoming.destroy();agent.destroy();reject(error)}});outgoing.once('error',(error)=>{agent.destroy();reject(error)});outgoing.end(Buffer.from(body))})}
