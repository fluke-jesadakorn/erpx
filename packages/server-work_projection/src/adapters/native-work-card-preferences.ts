import 'server-only';
import {workCardIds,type WorkCardId,type WorkCardPreferences} from '@erpx/feature-work_projection/lib/work-cards';
import {nativeWorkCardPreferencesRequest} from '@erpx/native-sdk/field-service-parts';
import {originProfile} from '@erpx/feature-runtime/lib/holding-hosts';
import {nativeHoldingConfiguration,nativeHoldingTargetById,nativeHoldingTargetForHost,type NativeHoldingConfiguration} from '@erpx/native-sdk/holding-target';
import {ApiError} from '@erpx/server-runtime/public/http';

type Actor={
 userId:string;holdingId:string;holdingSlug:string;companyId:string;
 branchId?:string|null;departmentId?:string|null;assignmentId:string;
 authority?:{revision:number;assignmentVersion:number};
};

const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const integer=(v:unknown,min=0):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=min;
const identifier=(v:unknown):v is string=>typeof v==='string'&&v.length>0&&v.length<=200&&!/[\u0000-\u001f\u007f/\\]/.test(v);
const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0&&new TextEncoder().encode(v).length<=8192;
const fail=(code:string,status:number,message:string):never=>{throw new ApiError(status,code,message);};

function configuration(raw:string):NativeHoldingConfiguration{
 const profile=originProfile();
 try{return nativeHoldingConfiguration(raw,profile);}catch{return fail('NATIVE_WORK_CARD_CONFIGURATION_INVALID',503,'The native work-card preference is not configured.');}
}

function validPreferences(value:unknown):WorkCardPreferences{
 const known=new Set<string>(workCardIds);
 if(!object(value)||!integer(value.version)||!(value.pinnedIds===null||Array.isArray(value.pinnedIds)&&value.pinnedIds.length<=6&&value.pinnedIds.every(id=>text(id)&&known.has(id))))return fail('NATIVE_WORK_CARD_RESPONSE_INVALID',502,'The native work-card preference response did not match its contract.');
 return {pinnedIds:value.pinnedIds as WorkCardId[]|null,version:value.version};
}

function target(config:NativeHoldingConfiguration,actor:Actor,incoming:Pick<Headers,'get'>){
 const sourceHost=incoming.get('host');
 const loopback=originProfile()==='loopback'&&sourceHost!==null&&/^127\.0\.0\.1:\d+$/.test(sourceHost);
 const selected=loopback
  ?config.holdings.find(h=>h.holdingSlug===actor.holdingSlug)??nativeHoldingTargetById(config,actor.holdingId)
  :(nativeHoldingTargetForHost(config,sourceHost)??nativeHoldingTargetById(config,actor.holdingId));
 if(!selected||selected.holdingId!==actor.holdingId||selected.holdingSlug!==actor.holdingSlug)
  return fail('NATIVE_WORK_CARD_TARGET_UNAVAILABLE',503,'The native work-card preference target is unavailable.');
 return selected;
}

function requestHeaders(incoming:Pick<Headers,'get'>,target:{origin:string},actor:Actor,method:'GET'|'POST',key?:string){
 const headers=new Headers({
  host:new URL(target.origin).host,
  origin:target.origin,
  accept:'application/json',
  'cache-control':'no-cache',
  'x-erpx-context-company':actor.companyId,
  'x-erpx-context-assignment':actor.assignmentId,
  'x-erpx-context-revision':String(actor.authority?.revision),
  'x-erpx-context-assignment-version':String(actor.authority?.assignmentVersion),
 });
 if(actor.branchId)headers.set('x-erpx-workspace-branch',actor.branchId);
 if(actor.departmentId)headers.set('x-erpx-workspace-department',actor.departmentId);
 const cookie=incoming.get('cookie');
 if(cookie){if(new TextEncoder().encode(cookie).length>8192)return fail('NATIVE_WORK_CARD_COOKIE_LIMIT',503,'The native work-card preference request is unavailable.');headers.set('cookie',cookie);}
 if(method==='POST'){headers.set('content-type','application/json');if(key)headers.set('idempotency-key',key);}
 return headers;
}

async function send(incoming:Pick<Headers,'get'>,actor:Actor,method:'GET'|'POST',body?:string,key?:string):Promise<WorkCardPreferences>{
 if(!identifier(actor.holdingId)||!identifier(actor.companyId)||!identifier(actor.assignmentId)||!actor.authority||!integer(actor.authority.revision)||!integer(actor.authority.assignmentVersion,1))
  return fail('WORK_CONTEXT_CHANGED',409,'Your work role changed. Reload and review before retrying.');
 const raw=process.env.ERPX_NATIVE_FIELD_SERVICE_WORKSPACE_CONFIG;
 if(!raw)return fail('NATIVE_WORK_CARD_CONFIGURATION_REQUIRED',503,'The native work-card preference is not configured.');
 const config=configuration(raw),selected=target(config,actor,incoming),headers=requestHeaders(incoming,selected,actor,method,key);
 let reply:Awaited<ReturnType<typeof nativeWorkCardPreferencesRequest>>;
 try{reply=await nativeWorkCardPreferencesRequest({baseUrl:config.privateBaseUrl,timeoutMs:config.timeoutMs,slug:selected.holdingSlug,headers,method,...(body===undefined?{}:{body})});}
 catch{return fail('NATIVE_WORK_CARD_UNAVAILABLE',503,'The native work-card preference is unavailable.');}
 if(reply.status!==200){
  const error=object(reply.body)&&object(reply.body.error)?reply.body.error:undefined;
  const code=typeof error?.code==='string'&&/^[A-Z0-9_]{3,100}$/.test(error.code)?error.code:'NATIVE_WORK_CARD_UNAVAILABLE';
  const message=typeof error?.message==='string'&&error.message.length<=300?error.message:'The native work-card preference is unavailable.';
  return fail(code,reply.status>=400&&reply.status<=599?reply.status:503,message);
 }
 if(!object(reply.body)||!object(reply.body.context))return fail('NATIVE_WORK_CARD_RESPONSE_INVALID',502,'The native work-card preference response did not match its contract.');
 const context=reply.body.context;
 if(context.companyId!==actor.companyId||context.assignmentId!==actor.assignmentId||context.revision!==actor.authority.revision)return fail('WORK_CONTEXT_CHANGED',409,'The current work context changed.');
 return validPreferences(reply.body.data);
}

/** Reads the current work-card preference through the native owner route. */
export async function readNativeWorkCardPreferences(incoming:Pick<Headers,'get'>,actor:Actor){return send(incoming,actor,'GET');}

/** Saves through the native owner route; availability is enforced there. */
export async function saveNativeWorkCardPreferences(incoming:Pick<Headers,'get'>,actor:Actor,body:string,key:string){return send(incoming,actor,'POST',body,key);}
