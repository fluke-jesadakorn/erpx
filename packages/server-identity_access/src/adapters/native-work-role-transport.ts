import 'server-only';
import type {NativeWorkRoleState} from '@erpx/contracts/field-service-reserve';
import {nativeWorkRolesRequest} from '@erpx/native-sdk/field-service-parts';
import {originProfile} from '@erpx/feature-runtime/lib/holding-hosts';
import {nativeHoldingConfiguration,type NativeHoldingConfiguration} from '@erpx/native-sdk/holding-target';
import {ApiError} from '@erpx/server-runtime/public/http';
type Identity={sessionId:string;userId:string;companyId?:string};
const fail=(code:string,status=503):never=>{throw new ApiError(status,code,'The native work role selection is unavailable.');};
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const integer=(v:unknown,min=0):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=min;
const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0&&new TextEncoder().encode(v).length<=8192;
const nullable=(v:unknown)=>v===null||text(v);
const strings=(v:unknown)=>Array.isArray(v)&&v.every(text);
function validPresentation(v:unknown):boolean{
 if(!object(v)||!integer(v.expiresAt,1)||typeof v.beta!=='boolean'||!strings(v.modules)||!object(v.iam))return false;
 const i=v.iam;return integer(i.roleRevision,1)&&integer(i.identityVersion)&&text(i.principalId)&&nullable(i.personId)&&strings(i.permissions)&&strings(i.grantablePermissions)&&strings(i.personUserIds)&&['hrAppointmentVersion','emergencyUntil','freshUntil'].every(k=>i[k]===null||integer(i[k]));
}
/** Mandatory native transport. Caller identities only bind the response; they
 * cannot construct the account proof, choose grants or authorize SQL writes. */
export function createNativeWorkRoleTransport(raw:string,send:typeof nativeWorkRolesRequest=nativeWorkRolesRequest){
 const profile=originProfile();let config:NativeHoldingConfiguration;try{config=nativeHoldingConfiguration(raw,profile);}catch{return fail('NATIVE_WORK_ROLE_CONFIGURATION_INVALID');}
 const {privateBaseUrl:base,timeoutMs,holdings:targets}=config;
 return async(incoming:Pick<Headers,'get'>,identity:Identity,mode:'current'|'restore'|'initialize'|'select'|'access'|'context',body?:string):Promise<NativeWorkRoleState>=>{
  const sourceHost=incoming.get('host'),loopback=profile==='loopback'&&sourceHost!==null&&/^127\.0\.0\.1:\d+$/.test(sourceHost);
  const target=loopback?targets[0]:targets.find(h=>new URL(h.origin).host===sourceHost);if(!target)return fail('NATIVE_WORK_ROLE_TARGET_UNAVAILABLE');
  const sourceOrigin=incoming.get('origin');
  if(loopback){if(sourceOrigin&&sourceOrigin!==`http://${sourceHost}`)return fail('ORIGIN_FORBIDDEN',403);if(mode==='select'&&!sourceOrigin)return fail('ORIGIN_FORBIDDEN',403);}
  const origin=loopback?target.origin:sourceOrigin;if(!loopback&&(origin&&origin!==target.origin||mode==='select'&&origin!==target.origin))return fail('ORIGIN_FORBIDDEN',403);
  const resolving=mode==='access'||mode==='context';
  const headers=new Headers({host:new URL(target.origin).host,accept:'application/json'});
  if(identity.companyId)headers.set(resolving?'x-erpx-requested-company':'x-erpx-context-company',identity.companyId);
  if(resolving){for(const name of ['x-erpx-front-company','x-erpx-destination','x-erpx-api','x-erpx-method']){const value=incoming.get(name);if(value)headers.set(name,value);}}
  if(mode==='context'){for(const name of ['x-erpx-context-company','x-erpx-context-assignment','x-erpx-context-revision','x-erpx-context-assignment-version']){const value=incoming.get(name);if(value)headers.set(name,value);}}
  const cookie=incoming.get('cookie');if(cookie){if(new TextEncoder().encode(cookie).length>8192)return fail('NATIVE_WORK_ROLE_COOKIE_LIMIT');headers.set('cookie',cookie);}
  for(const [name,routeName]of [['x-erpx-workspace-branch','x-erpx-branch-slug'],['x-erpx-workspace-department','x-erpx-department-slug']]as const){const captured=incoming.get(name),route=incoming.get(routeName);if(captured&&route&&captured!==route)return fail('WORK_CONTEXT_CHANGED',409);const value=route??captured;if(value)headers.set(name,value);}
  if(origin)headers.set('origin',origin);
  const method=mode==='select'?'POST':'GET';if(method==='POST')headers.set('content-type','application/json');
  let reply:Awaited<ReturnType<typeof send>>;try{reply=await send({baseUrl:base,timeoutMs,slug:target.holdingSlug,headers,method,...(body===undefined?{}:{body}),...(mode==='current'||mode==='initialize'||resolving?{mode:mode as 'current'|'initialize'|'access'|'context'}:{})});}catch{return fail('NATIVE_WORK_ROLE_UNAVAILABLE');}
  if(reply.status!==200){if(object(reply.body)&&object(reply.body.error)&&typeof reply.body.error.code==='string'&&reply.body.error.code.length<=100&&reply.status>=400&&reply.status<=599)return fail(reply.body.error.code,reply.status);return fail('NATIVE_WORK_ROLE_UNAVAILABLE');}
  const state=object(reply.body)?reply.body.data:undefined;
  if(!object(state)||state.holdingId!==target.holdingId||(!text(state.companyId)||identity.companyId!==undefined&&state.companyId!==identity.companyId)||state.userId!==identity.userId||state.sessionId!==identity.sessionId||!integer(state.expiresAt,1)||typeof state.valid!=='boolean'||!['branchId','departmentId','branchSlug','departmentSlug'].every(k=>nullable(state[k]))||!Array.isArray(state.options)||state.options.length>1000||!state.options.every(v=>object(v)&&text(v.id)&&integer(v.version,1)&&text(v.role)&&v.companyId===state.companyId&&nullable(v.branchId)&&Array.isArray(v.modules)&&v.modules.every(text)&&['holding','branch','department'].includes(String(v.scopeKind))&&nullable(v.scopeNodeId)&&text(v.label)&&integer(v.validUntil,1)&&nullable(v.branchSlug)&&nullable(v.departmentSlug)))return fail('NATIVE_WORK_ROLE_RESPONSE_INVALID',502);
  const s=state.selection;if(s!==null&&(!object(s)||!integer(s.revision)||!integer(s.expiresAt,1)||!nullable(s.assignmentId)||!(s.assignmentVersion===null||integer(s.assignmentVersion,1))))return fail('NATIVE_WORK_ROLE_RESPONSE_INVALID',502);
  if(resolving){
   if(!text(state.holdingName)||!text(state.companyName)||!text(state.companySlug)||!nullable(state.branchName)||!nullable(state.departmentName)||!Array.isArray(state.workspaces)||state.workspaces.length>4001||!state.workspaces.every(w=>object(w)&&['holding','branch','department'].includes(String(w.kind))&&text(w.id)&&text(w.label)&&nullable(w.branchSlug)&&nullable(w.departmentSlug)&&typeof w.inherited==='boolean'&&(w.kind!=='branch'||text(w.branchSlug))&&(w.kind!=='department'||text(w.departmentSlug))))return fail('NATIVE_WORK_ROLE_RESPONSE_INVALID',502);
   const c=state.context;if(c!==null&&!validPresentation(c))return fail('NATIVE_WORK_ROLE_RESPONSE_INVALID',502);
   if(mode==='access'&&c!==null||mode==='context'&&state.valid&&c===null)return fail('NATIVE_WORK_ROLE_RESPONSE_INVALID',502);
  }
  return state as NativeWorkRoleState;
 };
}
