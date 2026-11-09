import 'server-only';
import type {NativeAccountPreferences} from '@erpx/contracts/field-service-reserve';
import {nativeAccountPreferencesRequest} from '@erpx/native-sdk/field-service-parts';
import {nativeHoldingConfiguration,type NativeHoldingConfiguration} from '@erpx/native-sdk/holding-target';
import {originProfile} from '@erpx/feature-runtime/lib/holding-hosts';
import {ApiError} from '@erpx/server-runtime/public/http';
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0&&new TextEncoder().encode(v).length<=8192;
const nullable=(v:unknown)=>v===null||typeof v==='string';
const fail=(code:string,status=503):never=>{throw new ApiError(status,code,'The native account preference is unavailable.');};
export function createNativeAccountPreferencesClient(raw:string,send:typeof nativeAccountPreferencesRequest=nativeAccountPreferencesRequest){
 const profile=originProfile();let config:NativeHoldingConfiguration;try{config=nativeHoldingConfiguration(raw,profile);}catch{return fail('NATIVE_ACCOUNT_PREFERENCE_CONFIGURATION_INVALID');}
 return async(incoming:Pick<Headers,'get'>,userId:string,method:'GET'|'POST',body?:string,holdingId?:string,holdingSlug?:string,workspace=false):Promise<NativeAccountPreferences>=>{
  const sourceHost=incoming.get('host'),loopback=profile==='loopback'&&sourceHost!==null&&/^127\.0\.0\.1:\d+$/.test(sourceHost);
  const target=loopback?config.holdings[0]:config.holdings.find(h=>new URL(h.origin).host===sourceHost);
  if(!target||holdingId!==undefined&&holdingId!==target.holdingId||holdingSlug!==undefined&&holdingSlug!==target.holdingSlug)return fail('NATIVE_ACCOUNT_PREFERENCE_TARGET_UNAVAILABLE');
  const sourceOrigin=incoming.get('origin');
  if(loopback){if(sourceOrigin&&sourceOrigin!==`http://${sourceHost}`)return fail('ORIGIN_FORBIDDEN',403);}else if(sourceOrigin&&sourceOrigin!==target.origin||method==='POST'&&sourceOrigin!==target.origin)return fail('ORIGIN_FORBIDDEN',403);
  const origin=loopback?target.origin:sourceOrigin;
  const headers=new Headers({host:new URL(target.origin).host,accept:'application/json'});if(origin)headers.set('origin',origin);
  const cookie=incoming.get('cookie');if(cookie){if(new TextEncoder().encode(cookie).length>8192)return fail('NATIVE_ACCOUNT_PREFERENCE_COOKIE_LIMIT');headers.set('cookie',cookie);}
  if(method==='POST'){
   headers.set('content-type','application/json');
   for(const name of ['x-erpx-context-company','x-erpx-context-assignment','x-erpx-context-revision','x-erpx-context-assignment-version']){const value=incoming.get(name);if(value)headers.set(name,value);}
   for(const [name,routeName]of [['x-erpx-workspace-branch','x-erpx-branch-slug'],['x-erpx-workspace-department','x-erpx-department-slug']]as const){const captured=incoming.get(name),route=incoming.get(routeName);if(captured&&route&&captured!==route)return fail('WORK_CONTEXT_CHANGED',409);const value=route??captured;if(value)headers.set(name,value);}
  }
  let reply:Awaited<ReturnType<typeof send>>;try{reply=await send({baseUrl:config.privateBaseUrl,timeoutMs:config.timeoutMs,slug:target.holdingSlug,headers,method,...(body===undefined?{}:{body}),...(workspace?{workspace:true}:{})});}catch{return fail('NATIVE_ACCOUNT_PREFERENCE_UNAVAILABLE');}
  if(reply.status!==200){if(object(reply.body)&&object(reply.body.error)&&typeof reply.body.error.code==='string'&&reply.body.error.code.length<=100&&reply.status>=400&&reply.status<=599)return fail(reply.body.error.code,reply.status);return fail('NATIVE_ACCOUNT_PREFERENCE_UNAVAILABLE');}
  const state=object(reply.body)?reply.body.data:undefined;
  if(!object(state)||state.holdingId!==target.holdingId||state.holdingSlug!==target.holdingSlug||typeof state.holdingName!=='string'||state.userId!==userId||!text(state.sessionId)||!(state.workspace===null||object(state.workspace)&&nullable(state.workspace.workspacePath)&&nullable(state.workspace.platformAssignmentId))||!(state.company===null||object(state.company)&&text(state.company.companyId)))return fail('NATIVE_ACCOUNT_PREFERENCE_RESPONSE_INVALID',502);
  return state as NativeAccountPreferences;
 };
}
