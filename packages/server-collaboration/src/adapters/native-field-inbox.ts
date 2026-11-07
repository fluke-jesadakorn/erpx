import 'server-only';
import {headers} from 'next/headers';
import {nativeHoldingConfiguration} from '@erpx/native-sdk/holding-target';
import {nativeFieldInboxStateRequest,nativeFieldInboxCandidatesRequest,type NativeFieldInboxQuery} from '@erpx/native-sdk/field-service-parts';
const originProfile=()=>process.env.NODE_ENV==='development'&&process.env.NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE==='container-localhost'?'contributor' as const:'public' as const;
import {ApiError} from '@erpx/server-runtime/public/http';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {Candidate} from '../domain/inbox';
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0;
const integer=(v:unknown):v is number=>typeof v==='number'&&Number.isSafeInteger(v);
const nullableNumber=(v:unknown):v is number|null=>v===null||integer(v);
const equalSet=(a:readonly string[],b:unknown)=>Array.isArray(b)&&b.every(text)&&new Set(a).size===new Set(b).size&&a.every(x=>b.includes(x));
function fail(code:string,status=503):never{throw new ApiError(status,code,'The native Field Inbox is unavailable.');}
export function createNativeFieldInboxReader(raw:string,send:typeof nativeFieldInboxCandidatesRequest=nativeFieldInboxCandidatesRequest){
 let config:ReturnType<typeof nativeHoldingConfiguration>;try{config=nativeHoldingConfiguration(raw,originProfile());}catch{return fail('NATIVE_FIELD_INBOX_CONFIGURATION_INVALID');}
 return async(incoming:Pick<Headers,'get'>,actor:WorkContext,query:NativeFieldInboxQuery):Promise<Candidate[]>=>{
  const target=config.holdings.find(h=>new URL(h.origin).host===incoming.get('host')&&h.holdingId===actor.holdingId&&h.holdingSlug===actor.holdingSlug);
  if(!target||!actor.authority||!actor.iam)fail('WORK_CONTEXT_CHANGED',409);
  const origin=incoming.get('origin');if(origin&&origin!==target.origin)fail('ORIGIN_FORBIDDEN',403);
  const h=new Headers({host:new URL(target.origin).host,accept:'application/json','cache-control':'no-cache'}),cookie=incoming.get('cookie');
  if(cookie){if(new TextEncoder().encode(cookie).length>8192)fail('NATIVE_FIELD_INBOX_COOKIE_LIMIT');h.set('cookie',cookie);}if(origin)h.set('origin',origin);
  h.set('x-erpx-context-company',actor.companyId);h.set('x-erpx-context-assignment',actor.assignmentId);h.set('x-erpx-context-revision',String(actor.authority.revision));h.set('x-erpx-context-assignment-version',String(actor.authority.assignmentVersion));
  if(actor.scope.kind!=='holding'&&actor.scope.branchSlug)h.set('x-erpx-workspace-branch',actor.scope.branchSlug);if(actor.scope.kind==='department')h.set('x-erpx-workspace-department',actor.scope.departmentSlug);
  let reply:Awaited<ReturnType<typeof send>>;try{reply=await send({baseUrl:config.privateBaseUrl,slug:target.holdingSlug,headers:h,timeoutMs:config.timeoutMs,query});}catch{return fail('NATIVE_FIELD_INBOX_UNAVAILABLE');}
  if(reply.status!==200){const b=reply.body;if(object(b)&&object(b.error)&&text(b.error.code))fail(b.error.code,[401,403,404,409,422].includes(reply.status)?reply.status:503);fail('NATIVE_FIELD_INBOX_UNAVAILABLE');}
  const b=reply.body;if(!object(b)||!object(b.context)||!Array.isArray(b.data)||b.data.length>query.limit)fail('NATIVE_FIELD_INBOX_RESPONSE_INVALID',502);
  const c=b.context;
  const expected={holdingId:actor.holdingId,userId:actor.userId,companyId:actor.companyId,assignmentId:actor.assignmentId,assignmentVersion:actor.authority.assignmentVersion,revision:actor.authority.revision,role:actor.role,branchId:actor.branchId,departmentId:actor.departmentId};
  if(Object.entries(expected).some(([k,v])=>c[k]!==v)||!equalSet(actor.modules,c.modules)||!equalSet(actor.iam.permissions,c.permissions)||!integer(c.expiresAt)||c.expiresAt<=Date.now())fail('WORK_CONTEXT_CHANGED',409);
  const ids=new Set<string>(),result:Candidate[]=[];
  for(const r of b.data){
   if(!object(r)||!text(r.id)||ids.has(r.id)||!text(r.event_id)||r.company_id!==actor.companyId||r.user_id!==actor.userId||!integer(r.delivered_at)||!nullableNumber(r.read_at)||!nullableNumber(r.archived_at)||(typeof r.source_type!=='string'||!['field_service_order','operational_exception','collaboration_action','support_ticket','stock_count','invoice','journal'].includes(r.source_type))||!text(r.source_id)||!integer(r.source_version)||(typeof r.category!=='string'||!['work','calendar','mention','notice','system'].includes(r.category))||typeof r.preview!=='string'||!object(r.native_field_item))fail('NATIVE_FIELD_INBOX_RESPONSE_INVALID',502);
   ids.add(r.id);const i=r.native_field_item,href=r.source_type==='field_service_order'?'/field-service/'+encodeURIComponent(r.source_id):r.source_type==='operational_exception'?'/operations/'+encodeURIComponent(r.source_id):r.source_type==='support_ticket'?'/field-service/support/'+encodeURIComponent(r.source_id):r.source_type==='journal'?'/finance/journals/'+encodeURIComponent(r.source_id):r.source_type==='invoice'?'/finance/invoices/'+encodeURIComponent(r.source_id):r.source_type==='stock_count'?'/services/field-service/settings/initial-stock/'+encodeURIComponent(r.source_id):'/collaboration';
   const source=r.source_type==='field_service_order'?{kind:'field_service_order',id:r.source_id,href,label:'Field service order'}:undefined;
   const sourceMatches=source?object(i.source)&&Object.entries(source).every(([key,value])=>i.source&&object(i.source)&&i.source[key]===value):i.source===undefined||i.source===null;
   if(i.id!==r.id||i.eventId!==r.event_id||i.sourceType!==r.source_type||i.sourceId!==r.source_id||i.category!==r.category||i.message!==r.preview||i.createdAt!==r.delivered_at||i.readAt!==r.read_at||i.archivedAt!==r.archived_at||i.href!==href||!sourceMatches)fail('NATIVE_FIELD_INBOX_RESPONSE_INVALID',502);
   result.push({id:r.id,event_id:r.event_id,company_id:actor.companyId,user_id:actor.userId,delivered_at:r.delivered_at,read_at:r.read_at,archived_at:r.archived_at,source_type:r.source_type,source_id:r.source_id,source_version:r.source_version,category:r.category as Candidate['category'],preview:r.preview,native_field_item:{id:r.id,eventId:r.event_id,category:r.category as Candidate['category'],message:r.preview,href,createdAt:r.delivered_at,readAt:r.read_at,archivedAt:r.archived_at,sourceType:r.source_type,sourceId:r.source_id,...(source?{source}:{})}});
  }
  return result;
 };
}
export async function nativeFieldInboxCandidates(actor:WorkContext,query:NativeFieldInboxQuery):Promise<Candidate[]>{
 // No relevant owner authority is a presentation filter, never a grant. The legacy stream
 // independently excludes this entire source partition for every caller.
 if(!actor.modules.includes('field_service')&&!actor.iam?.permissions.includes('support.read')&&!actor.iam?.permissions.includes('records.read')&&!actor.iam?.permissions.includes('inventory.read')&&!actor.iam?.permissions.includes('finance.read'))return [];
 const raw=process.env.ERPX_NATIVE_FIELD_SERVICE_SOURCE_RECORD_CONFIG;if(!raw)fail('NATIVE_FIELD_INBOX_CONFIGURATION_REQUIRED');
 return createNativeFieldInboxReader(raw)(await headers(),actor,query);
}

export function createNativeFieldInboxState(raw:string,send:typeof nativeFieldInboxStateRequest=nativeFieldInboxStateRequest){
 let config:ReturnType<typeof nativeHoldingConfiguration>;try{config=nativeHoldingConfiguration(raw,originProfile());}catch{return fail('NATIVE_FIELD_INBOX_CONFIGURATION_INVALID');}
 return async(incoming:Pick<Headers,'get'>,actor:WorkContext,id:string,body:string,key:string)=>{
  const target=config.holdings.find(h=>new URL(h.origin).host===incoming.get('host')&&h.holdingId===actor.holdingId&&h.holdingSlug===actor.holdingSlug);
  if(!target||!actor.authority||!actor.iam)fail('WORK_CONTEXT_CHANGED',409);
  if(incoming.get('origin')!==target.origin)fail('ORIGIN_FORBIDDEN',403);
  if(!key||new TextEncoder().encode(key).length>200)fail('COMMAND_KEY_REQUIRED',422);
  const cookie=incoming.get('cookie'),h=new Headers({host:new URL(target.origin).host,origin:target.origin,accept:'application/json','content-type':'application/json','idempotency-key':key});
  if(cookie){if(new TextEncoder().encode(cookie).length>8192)fail('NATIVE_FIELD_INBOX_COOKIE_LIMIT');h.set('cookie',cookie);}
  h.set('x-erpx-context-company',actor.companyId);h.set('x-erpx-context-assignment',actor.assignmentId);h.set('x-erpx-context-revision',String(actor.authority.revision));h.set('x-erpx-context-assignment-version',String(actor.authority.assignmentVersion));
  if(actor.scope.kind!=='holding'&&actor.scope.branchSlug)h.set('x-erpx-workspace-branch',actor.scope.branchSlug);if(actor.scope.kind==='department')h.set('x-erpx-workspace-department',actor.scope.departmentSlug);
  let reply:Awaited<ReturnType<typeof send>>;try{reply=await send({baseUrl:config.privateBaseUrl,slug:target.holdingSlug,headers:h,timeoutMs:config.timeoutMs,id,body});}catch{return fail('NATIVE_FIELD_INBOX_UNAVAILABLE');}
  const b=reply.body;if(reply.status!==200){if(object(b)&&object(b.error)&&text(b.error.code))fail(b.error.code,[401,403,404,409,422].includes(reply.status)?reply.status:503);fail('NATIVE_FIELD_INBOX_UNAVAILABLE');}
  if(!object(b)||!object(b.context)||!object(b.data))fail('NATIVE_FIELD_INBOX_RESPONSE_INVALID',502);
  const c=b.context,expected={holdingId:actor.holdingId,userId:actor.userId,companyId:actor.companyId,assignmentId:actor.assignmentId,assignmentVersion:actor.authority.assignmentVersion,revision:actor.authority.revision,role:actor.role,branchId:actor.branchId,departmentId:actor.departmentId};
  if(Object.entries(expected).some(([k,v])=>c[k]!==v)||!equalSet(actor.modules,c.modules)||!equalSet(actor.iam.permissions,c.permissions)||!integer(c.expiresAt)||c.expiresAt<=Date.now())fail('WORK_CONTEXT_CHANGED',409);
  if(b.data.id!==id||!nullableNumber(b.data.readAt)||!nullableNumber(b.data.archivedAt))fail('NATIVE_FIELD_INBOX_RESPONSE_INVALID',502);
  return{id,readAt:b.data.readAt,archivedAt:b.data.archivedAt};
 };
}
export async function nativeFieldInboxState(actor:WorkContext,id:string,body:string,key:string){
 const raw=process.env.ERPX_NATIVE_FIELD_SERVICE_SOURCE_RECORD_CONFIG;if(!raw)fail('NATIVE_FIELD_INBOX_CONFIGURATION_REQUIRED');
 return createNativeFieldInboxState(raw)(await headers(),actor,id,body,key);
}
