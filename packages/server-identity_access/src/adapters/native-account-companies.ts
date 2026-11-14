import 'server-only';
import type {NativeAccountCompanies} from '@erpx/contracts/field-service-reserve';
import {nativeAccountCompaniesRequest} from '@erpx/native-sdk/field-service-parts';
import {originProfile} from '@erpx/feature-runtime/lib/holding-hosts';
import {nativeHoldingConfiguration,nativeHoldingTargetById,nativeHoldingTargetForHost,type NativeHoldingConfiguration} from '@erpx/native-sdk/holding-target';
import {ApiError} from '@erpx/server-runtime/public/http';
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0&&new TextEncoder().encode(v).length<=8192;
const fail=(code:string,status=503):never=>{throw new ApiError(status,code,'The native Company selection is unavailable.');};
export function createNativeAccountCompaniesReader(raw:string,send:typeof nativeAccountCompaniesRequest=nativeAccountCompaniesRequest){
 const profile=originProfile();let config:NativeHoldingConfiguration;try{config=nativeHoldingConfiguration(raw,profile);}catch{return fail('NATIVE_ACCOUNT_COMPANIES_CONFIGURATION_INVALID');}
 const {privateBaseUrl:baseUrl,timeoutMs}=config;
 return async(incoming:Pick<Headers,'get'>,identity:{sessionId:string;userId:string},holdingId?:string):Promise<NativeAccountCompanies>=>{
  const sourceHost=incoming.get('host'),loopback=profile==='loopback'&&sourceHost!==null&&/^127\.0\.0\.1:\d+$/.test(sourceHost);
  const target=loopback?(holdingId?nativeHoldingTargetById(config,holdingId):config.holdings[0]):(nativeHoldingTargetForHost(config,sourceHost)??(holdingId?nativeHoldingTargetById(config,holdingId):undefined));if(!target||holdingId!==undefined&&target.holdingId!==holdingId)return fail('NATIVE_ACCOUNT_COMPANIES_TARGET_UNAVAILABLE');
  const sourceOrigin=incoming.get('origin');
  if(loopback){if(sourceOrigin&&sourceOrigin!==`http://${sourceHost}`)return fail('ORIGIN_FORBIDDEN',403);}else if(sourceOrigin&&new URL(sourceOrigin).host!==sourceHost)return fail('ORIGIN_FORBIDDEN',403);
  const origin=loopback?target.origin:sourceOrigin;
  const headers=new Headers({host:loopback?new URL(target.origin).host:(sourceHost??new URL(target.origin).host),accept:'application/json'});if(origin)headers.set('origin',origin);const cookie=incoming.get('cookie');if(cookie){if(new TextEncoder().encode(cookie).length>8192)return fail('NATIVE_ACCOUNT_COMPANIES_COOKIE_LIMIT');headers.set('cookie',cookie);}
  let reply:Awaited<ReturnType<typeof send>>;try{reply=await send({baseUrl,timeoutMs,slug:target.holdingSlug,headers});}catch{return fail('NATIVE_ACCOUNT_COMPANIES_UNAVAILABLE');}
  if(reply.status!==200){if(object(reply.body)&&object(reply.body.error)&&typeof reply.body.error.code==='string'&&reply.body.error.code.length<=100&&reply.status>=400&&reply.status<=599)return fail(reply.body.error.code,reply.status);return fail('NATIVE_ACCOUNT_COMPANIES_UNAVAILABLE');}
  const state=object(reply.body)?reply.body.data:undefined;
  if(!object(state)||state.holdingId!==target.holdingId||state.userId!==identity.userId||state.sessionId!==identity.sessionId||typeof state.name!=='string'||typeof state.email!=='string'||typeof state.expiresAt!=='number'||!Number.isSafeInteger(state.expiresAt)||state.expiresAt<=0||!Array.isArray(state.companies)||state.companies.length>256||!state.companies.every(x=>object(x)&&text(x.id)&&text(x.slug)&&typeof x.name==='string')||new Set(state.companies.map(x=>(x as {id:string}).id)).size!==state.companies.length)return fail('NATIVE_ACCOUNT_COMPANIES_RESPONSE_INVALID',502);
  return state as NativeAccountCompanies;
 };
}
