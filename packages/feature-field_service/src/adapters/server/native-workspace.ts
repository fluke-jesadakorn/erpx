import 'server-only';
import {Agent,request} from 'node:http';
import {Readable} from 'node:stream';
import {headers} from 'next/headers';
import type {FieldServiceWorkspaceResponse} from '@erpx/contracts/field-service-reserve';
import type {FieldWorkspace} from '../../lib/workspace';
import type {ReserveContext} from '../../lib/native-reserve-command';

type Settings={privateBaseUrl:string;timeoutMs:number;holdings:{origin:string;holdingSlug:string;holdingId:string}[]};
type Transport=(url:URL,headers:Headers,signal:AbortSignal)=>Promise<Response>;
export class NativeServiceWorkspaceError extends Error {constructor(readonly code:string,readonly status?:number){super('The native Service workspace is unavailable.');}}
const fail=(code:string,status?:number):never=>{throw new NativeServiceWorkspaceError(code,status);};
const obj=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const text=(v:unknown)=>typeof v==='string'&&v.length>0;
const integer=(v:unknown,min=0):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=min;
const row=(v:unknown):v is Record<string,string|number|null>=>obj(v)&&Object.values(v).every(x=>x===null||typeof x==='string'||typeof x==='number'&&Number.isSafeInteger(x));
const list=(v:unknown):v is Record<string,string|number|null>[]=>Array.isArray(v)&&v.length<=10000&&v.every(row);
const equalSet=(a:readonly string[],b:unknown)=>Array.isArray(b)&&b.every(x=>typeof x==='string')&&new Set(a).size===new Set(b).size&&a.every(x=>b.includes(x));
function configuration(raw:string):Settings {
 let v:unknown;try{v=JSON.parse(raw);}catch{return fail('NATIVE_SERVICE_CONFIGURATION_INVALID');}
 if(!obj(v)||typeof v.privateBaseUrl!=='string'||!integer(v.timeoutMs,250)||v.timeoutMs>30000||!Array.isArray(v.holdings)||v.holdings.length<1||v.holdings.length>64)return fail('NATIVE_SERVICE_CONFIGURATION_INVALID');
 let base:URL;try{base=new URL(v.privateBaseUrl);}catch{return fail('NATIVE_SERVICE_CONFIGURATION_INVALID');}
 const port=Number(base.port),reserved=[3000,3001,3100,3101,3103,5432,5433,5435,8080];
 if(base.protocol!=='http:'||!['127.0.0.1','[::1]'].includes(base.hostname)||!base.port||reserved.includes(port)||port>=3200&&port<=3299||base.username||base.password||v.privateBaseUrl!==base.origin)return fail('NATIVE_SERVICE_CONFIGURATION_INVALID');
 const hosts=new Set<string>(),slugs=new Set<string>();for(const h of v.holdings){if(!obj(h)||typeof h.origin!=='string'||typeof h.holdingSlug!=='string'||!/^[a-z0-9_-]{1,128}$/.test(h.holdingSlug)||!text(h.holdingId)||new TextEncoder().encode(h.holdingId as string).length>200)return fail('NATIVE_SERVICE_CONFIGURATION_INVALID');let origin:URL;try{origin=new URL(h.origin);}catch{return fail('NATIVE_SERVICE_CONFIGURATION_INVALID');}if(origin.protocol!=='https:'||h.origin!==origin.origin||origin.port||origin.username||origin.password||!origin.hostname.includes('.')||!origin.hostname.split('.').every(p=>/^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(p))||hosts.has(origin.host)||slugs.has(h.holdingSlug))return fail('NATIVE_SERVICE_CONFIGURATION_INVALID');hosts.add(origin.host);slugs.add(h.holdingSlug);}
 return v as Settings;
}
function workspace(value:unknown,actor:ReserveContext):FieldWorkspace {
 if(!obj(value)||!obj(value.context)||!obj(value.data))return fail('NATIVE_SERVICE_RESPONSE_INVALID');
 const c=value.context,d=value.data;
 for(const [key,expected] of Object.entries(actor)){if(key==='modules'||key==='permissions')continue;if(c[key]!==expected)return fail('NATIVE_SERVICE_CONTEXT_CHANGED');}
 if(!equalSet(actor.modules,c.modules)||!equalSet(actor.permissions,c.permissions)||!integer(c.expiresAt,1))return fail('NATIVE_SERVICE_CONTEXT_CHANGED');
 if(!list(d.agreements)||!list(d.orders)||!list(d.customers)||!list(d.assets)||!list(d.tickets)||!list(d.people)||!list(d.nodes)||!obj(d.referenceScope)||typeof d.workspaceKey!=='string'||!['canWrite','canCreateAgreement','canCreateOrder','canUseParts','canUseAssetReferences'].every(k=>typeof d[k]==='boolean'))return fail('NATIVE_SERVICE_RESPONSE_INVALID');
 if(d.workspaceKey!==JSON.stringify([actor.holdingId,actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.revision])||d.referenceScope.branchId!==actor.branchId||d.referenceScope.departmentId!==actor.departmentId)return fail('NATIVE_SERVICE_CONTEXT_CHANGED');
 if(d.agreements.some(r=>!text(r.id)||!integer(r.version,1)||!text(r.status)||!text(r.created_by)||!integer(r.updated_at)||!integer(r.can_review)||!integer(r.warranty_discount_bps)||!['can_activate','can_reject','can_retire','can_request'].every(k=>integer(r[k])))||d.orders.some(r=>!text(r.id)||!integer(r.version,1)||!text(r.status)||!text(r.technician_id)||!text(r.created_by)||!integer(r.updated_at)||!(r.scheduled_at===null||typeof r.scheduled_at==='number'&&Number.isSafeInteger(r.scheduled_at))||!integer(r.technician_eligible)||!integer(r.can_review)||!integer(r.can_work)||!integer(r.can_release_billing)||typeof r.issue!=='string'||typeof r.site!=='string'||!(r.exception_reason===null||typeof r.exception_reason==='string')))return fail('NATIVE_SERVICE_RESPONSE_INVALID');
 if(d.customers.some(r=>!text(r.id)||typeof r.name!=='string'||!text(r.currency))||d.assets.some(r=>!text(r.id)||typeof r.code!=='string'||typeof r.name!=='string'||!integer(r.version,1)||!text(r.branch_id)||!(r.department_id===null||text(r.department_id)))||d.people.some(r=>!text(r.id)||typeof r.name!=='string'||!text(r.branch_id)||!(r.department_id===null||text(r.department_id)))||d.nodes.some(r=>!text(r.id)||!['branch','headquarters','department'].includes(String(r.kind))||typeof r.display_name!=='string'||!text(r.branch_id))||d.tickets.some(r=>!text(r.id)||!integer(r.version,1)||typeof r.title!=='string'||!text(r.customer_id)||!text(r.branch_id)||!(r.department_id===null||text(r.department_id))))return fail('NATIVE_SERVICE_RESPONSE_INVALID');
 const response=value as unknown as FieldServiceWorkspaceResponse;
 return response.data as FieldWorkspace;
}
function direct(url:URL,headers:Headers,signal:AbortSignal):Promise<Response>{return new Promise((resolve,reject)=>{const agent=new Agent({keepAlive:false,maxSockets:1,maxTotalSockets:1,proxyEnv:{} as import('node:http').ProxyEnv});const req=request(url,{method:'GET',agent,headers:Object.fromEntries(headers),signal,maxHeaderSize:8192},incoming=>{incoming.once('close',()=>agent.destroy());try{const values=new Headers();for(let i=0;i<incoming.rawHeaders.length;i+=2)values.append(incoming.rawHeaders[i]!,incoming.rawHeaders[i+1]!);resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>,{status:incoming.statusCode??502,headers:values}));}catch(error){incoming.destroy();agent.destroy();reject(error);}});req.once('error',error=>{agent.destroy();reject(error);});req.end();});}
export function createNativeServiceWorkspaceReader(raw:string,transport:Transport=direct){
 const config=configuration(raw);return async(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext):Promise<FieldWorkspace>=>{
  const host=incoming.get('host'),target=config.holdings.find(h=>new URL(h.origin).host===host&&h.holdingSlug===slug);
  if(!target||target.holdingId!==actor.holdingId)return fail('NATIVE_SERVICE_AUTHORITY_INVALID');
  const out=new Headers({host:host!,accept:'application/json','cache-control':'no-cache'}),cookie=incoming.get('cookie');if(cookie){if(new TextEncoder().encode(cookie).length>8192)return fail('NATIVE_SERVICE_COOKIE_LIMIT');out.set('cookie',cookie);}
  out.set('x-erpx-context-company',actor.companyId);out.set('x-erpx-context-assignment',actor.assignmentId);out.set('x-erpx-context-revision',String(actor.revision));out.set('x-erpx-context-assignment-version',String(actor.assignmentVersion));if(actor.branchSlug)out.set('x-erpx-workspace-branch',actor.branchSlug);if(actor.departmentSlug)out.set('x-erpx-workspace-department',actor.departmentSlug);
  const controller=new AbortController(),timer=setTimeout(()=>controller.abort(),config.timeoutMs);
  try{const url=new URL(`/api/v1/holdings/${encodeURIComponent(slug)}/field-service/workspace`,config.privateBaseUrl),response=await transport(url,out,controller.signal);if(response.redirected||response.status!==200){void response.body?.cancel().catch(()=>undefined);return fail('NATIVE_SERVICE_UNAVAILABLE',response.status);}if(response.headers.get('content-type')?.split(';')[0]?.trim()!=='application/json'||!response.body)return fail('NATIVE_SERVICE_RESPONSE_INVALID');const reader=response.body.getReader(),chunks:Uint8Array[]=[];let length=0;const cancel=()=>{void reader.cancel().catch(()=>undefined);};controller.signal.addEventListener('abort',cancel,{once:true});try{for(;;){const part=await reader.read();if(controller.signal.aborted)return fail('NATIVE_SERVICE_ABORTED');if(part.done)break;length+=part.value.length;if(length>2*1024*1024){await reader.cancel();return fail('NATIVE_SERVICE_RESPONSE_LIMIT');}chunks.push(part.value);}}finally{controller.signal.removeEventListener('abort',cancel);reader.releaseLock();}const bytes=new Uint8Array(length);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.length;}let data:unknown;try{data=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));}catch{return fail('NATIVE_SERVICE_RESPONSE_INVALID');}return workspace(data,actor);
  }catch(error){if(error instanceof NativeServiceWorkspaceError)throw error;return fail(controller.signal.aborted?'NATIVE_SERVICE_ABORTED':'NATIVE_SERVICE_UNAVAILABLE');}finally{clearTimeout(timer);}
 };
}
export async function nativeServiceWorkspace(slug:string,actor:ReserveContext|undefined):Promise<FieldWorkspace|undefined>{const raw=process.env.ERPX_NATIVE_FIELD_SERVICE_WORKSPACE_CONFIG;if(raw===undefined)return undefined;if(!actor)return fail('NATIVE_SERVICE_CONTEXT_CHANGED');return createNativeServiceWorkspaceReader(raw)(await headers(),slug,actor);}
