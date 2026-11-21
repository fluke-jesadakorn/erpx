import 'server-only';
import type {NativeServiceCompanies,NativeServicePackageOverview} from '@erpx/contracts/commercial-service';
import {nativeServicePackageRequest,type NativeServicePackageRequest} from '@erpx/native-sdk/field-service-parts';
import {nativeHoldingConfiguration} from '@erpx/native-sdk/holding-target';
import {ApiError} from '@erpx/server-runtime/public/http';
const fail=(code:string,status=503):never=>{throw new ApiError(status,code,'The native Service package lifecycle is unavailable.');};
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
/** Technical proxy only. Rust derives current authority and checks the source;
 * a private transport failure never selects a TypeScript business writer. */
export function createNativeServiceCommercialTransport(raw:string,send:typeof nativeServicePackageRequest=nativeServicePackageRequest){
 let config:ReturnType<typeof nativeHoldingConfiguration>;
 try{config=nativeHoldingConfiguration(raw,process.env.NODE_ENV==='development'&&process.env.NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE==='container-localhost'?'contributor':'public');}catch{return fail('NATIVE_SERVICE_COMMERCIAL_CONFIGURATION_INVALID');}
 return async(incoming:Pick<Headers,'get'>,operation:NativeServicePackageRequest['operation'],options:{holdingId?:string;companyId?:string;body?:string}={})=>{
  const target=config.holdings.find(h=>new URL(h.origin).host===incoming.get('host'));
  if(!target)return fail('NOT_FOUND',404);
  const origin=incoming.get('origin'),write=operation==='preview'||operation==='apply';
  if(write?origin!==target.origin:origin!==null&&origin!==target.origin)return fail('ORIGIN_FORBIDDEN',403);
  const headers=new Headers({host:new URL(target.origin).host,accept:'application/json'});
  for(const name of ['cookie','origin','content-type','idempotency-key','x-erpx-platform-assignment','x-erpx-platform-role']){const value=incoming.get(name);if(value!==null){if(new TextEncoder().encode(value).length>8192)return fail('NATIVE_SERVICE_COMMERCIAL_HEADER_LIMIT',413);headers.set(name,value);}}
  let reply:Awaited<ReturnType<typeof send>>;
  try{reply=await send({baseUrl:config.privateBaseUrl,timeoutMs:config.timeoutMs,slug:target.holdingSlug,headers,operation,...options});}catch{return fail('NATIVE_SERVICE_COMMERCIAL_UNAVAILABLE');}
  if(reply.status!==200){if(object(reply.body)&&object(reply.body.error)&&typeof reply.body.error.code==='string'&&/^[A-Z0-9_]{1,100}$/.test(reply.body.error.code)&&reply.status>=400&&reply.status<=599)return fail(reply.body.error.code,reply.status);return fail('NATIVE_SERVICE_COMMERCIAL_RESPONSE_INVALID',502);}
  if(!object(reply.body)||!object(reply.body.data))return fail('NATIVE_SERVICE_COMMERCIAL_RESPONSE_INVALID',502);
  return reply.body.data;
 };
}
function transport(){const raw=process.env.ERPX_NATIVE_FIELD_SERVICE_WORKSPACE_CONFIG;if(!raw)return fail('NATIVE_SERVICE_COMMERCIAL_TARGET_UNAVAILABLE');return createNativeServiceCommercialTransport(raw);}
export async function nativeServiceCompanies(headers:Pick<Headers,'get'>):Promise<NativeServiceCompanies>{return await transport()(headers,'companies') as NativeServiceCompanies;}
export async function nativeServicePackageOverview(headers:Pick<Headers,'get'>,holdingId:string,companyId:string):Promise<NativeServicePackageOverview>{return await transport()(headers,'overview',{holdingId,companyId}) as NativeServicePackageOverview;}
export async function nativeServicePackageMutation(headers:Pick<Headers,'get'>,operation:'preview'|'apply',body:string){return transport()(headers,operation,{body});}
export function nativeServiceCommercialHost(headers:Pick<Headers,'get'>){
 const raw=process.env.ERPX_NATIVE_FIELD_SERVICE_WORKSPACE_CONFIG;if(!raw)return false;
 let c:ReturnType<typeof nativeHoldingConfiguration>;try{c=nativeHoldingConfiguration(raw,process.env.NODE_ENV==='development'&&process.env.NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE==='container-localhost'?'contributor':'public');}catch{return fail('NATIVE_SERVICE_COMMERCIAL_CONFIGURATION_INVALID');}
 return c.holdings.some(h=>new URL(h.origin).host===headers.get('host'));
}
