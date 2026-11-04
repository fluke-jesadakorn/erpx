import 'server-only';
// Bounded native transport for the Field visit-completion evidence. Retained
// shared consumer: the Runtime/Apps executive evidence reader. Removal
// condition: move that reader to the native report contract directly; this
// adapter must not grow Field SQL, policy or retry authority in the interim.
import {headers} from 'next/headers';
import {nativeHoldingConfiguration} from '@erpx/native-sdk/holding-target';
import {nativeFieldEvidenceRequest} from '@erpx/native-sdk/field-service-parts';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {ApiError} from '@erpx/server-runtime/public/http';

type Search={provider:string;metric:'field-visit-completion';fields:string[];items:{id:string;label:string;summary:string;status:string;version:number;href:string}[];hasMore:boolean;nextCursor:string|null;queryHash:string};
type Open={provider:string;metric:'field-visit-completion';fields:string[];record:{id:string;scheduled_at:number;visit_completed_at:number|null;status:string;version:number;href:string};queryHash:string};
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const integer=(v:unknown):v is number=>typeof v==='number'&&Number.isSafeInteger(v);
const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0;
const sameSet=(a:readonly string[],b:unknown)=>Array.isArray(b)&&b.every(text)&&a.length===b.length&&new Set(a).size===new Set(b).size&&a.every(v=>b.includes(v));
function fail(code='NATIVE_FIELD_EVIDENCE_UNAVAILABLE',status=503):never{throw new ApiError(status,code,'The native Field evidence is unavailable.');}
/** Transport and read-model validation only. Rust owns source eligibility,
 * cursor/filter policy and the access audit; this adapter never retries. */
export function createNativeFieldEvidenceReader(raw:string,send:typeof nativeFieldEvidenceRequest=nativeFieldEvidenceRequest){
 const profile=process.env.NODE_ENV==='development'&&process.env.NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE==='container-localhost'?'contributor':'public';
 let config:ReturnType<typeof nativeHoldingConfiguration>;try{config=nativeHoldingConfiguration(raw,profile);}catch{return fail('NATIVE_FIELD_EVIDENCE_CONFIGURATION_INVALID');}
 return async<A extends 'search'|'open'>(incoming:Pick<Headers,'get'>,actor:WorkContext,action:A,input:unknown):Promise<A extends 'search'?Search:Open>=>{
  const target=config.holdings.find(h=>new URL(h.origin).host===incoming.get('host')&&h.holdingId===actor.holdingId&&h.holdingSlug===actor.holdingSlug);
  if(!target||!actor.authority||!actor.iam)fail('WORK_CONTEXT_CHANGED',409);
  const origin=incoming.get('origin');if(origin!==null&&origin!==target.origin)fail('ORIGIN_FORBIDDEN',403);
  const h=new Headers({host:new URL(target.origin).host,accept:'application/json','content-type':'application/json','cache-control':'no-cache','x-erpx-context-company':actor.companyId,'x-erpx-context-assignment':actor.assignmentId,'x-erpx-context-revision':String(actor.authority.revision),'x-erpx-context-assignment-version':String(actor.authority.assignmentVersion)});
  const cookie=incoming.get('cookie');if(cookie){if(Buffer.byteLength(cookie)>8192)fail();h.set('cookie',cookie);}if(origin)h.set('origin',origin);
  if(actor.scope.kind!=='holding'&&actor.scope.branchSlug)h.set('x-erpx-workspace-branch',actor.scope.branchSlug);if(actor.scope.kind==='department')h.set('x-erpx-workspace-department',actor.scope.departmentSlug);
  let reply:Awaited<ReturnType<typeof send>>;try{reply=await send({baseUrl:config.privateBaseUrl,slug:target.holdingSlug,headers:h,timeoutMs:config.timeoutMs,action,body:JSON.stringify(input)});}catch{return fail();}
  const b=reply.body;if(reply.status!==200){if(object(b)&&object(b.error)&&text(b.error.code))fail(b.error.code,[401,403,404,409,422].includes(reply.status)?reply.status:503);return fail();}
  if(!object(b)||!object(b.context)||!object(b.data))fail('NATIVE_FIELD_EVIDENCE_RESPONSE_INVALID',502);
  const c=b.context,expected={holdingId:actor.holdingId,userId:actor.userId,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,branchSlug:actor.scope.kind==='holding'?null:actor.scope.branchSlug??null,departmentSlug:actor.scope.kind==='department'?actor.scope.departmentSlug:null,assignmentId:actor.assignmentId,assignmentVersion:actor.authority.assignmentVersion,revision:actor.authority.revision,role:actor.role,assignmentScopeKind:actor.assignmentScope};
  if(Object.entries(expected).some(([k,v])=>c[k]!==v)||!sameSet(actor.modules,c.modules)||!sameSet(actor.iam.permissions,c.permissions)||!integer(c.expiresAt)||c.expiresAt<=Date.now())fail('WORK_CONTEXT_CHANGED',409);
  const d=b.data,fields=['scheduled_at','visit_completed_at','status','version'];
  const row=(r:unknown)=>object(r)&&text(r.id)&&text(r.status)&&integer(r.version)&&r.version>0&&r.href===`/field-service/${encodeURIComponent(r.id)}`;
  if(d.provider!=='field-service-orders'||d.metric!=='field-visit-completion'||!Array.isArray(d.fields)||JSON.stringify(d.fields)!==JSON.stringify(fields)||typeof d.queryHash!=='string'||!/^[a-f0-9]{64}$/.test(d.queryHash))fail('NATIVE_FIELD_EVIDENCE_RESPONSE_INVALID',502);
  if(action==='search'){
   if(!Array.isArray(d.items)||d.items.length>50||!d.items.every(r=>row(r)&&typeof r.label==='string'&&typeof r.summary==='string')||new Set(d.items.map(r=>r.id)).size!==d.items.length||typeof d.hasMore!=='boolean'||!(d.nextCursor===null||text(d.nextCursor)&&d.nextCursor.length<=2000)||d.hasMore!==(d.nextCursor!==null)||d.hasMore&&d.items.length===0)fail('NATIVE_FIELD_EVIDENCE_RESPONSE_INVALID',502);
  }else if(!row(d.record)||!object(d.record)||!integer(d.record.scheduled_at)||!(d.record.visit_completed_at===null||integer(d.record.visit_completed_at))||!object(input)||d.record.id!==input.recordId)fail('NATIVE_FIELD_EVIDENCE_RESPONSE_INVALID',502);
  // The bounded DTO is released only after binding it to the current request.
  const result=action==='search'?{provider:d.provider,metric:d.metric,fields,items:(d.items as Search['items']).map(({id,label,summary,status,version,href})=>({id,label,summary,status,version,href})),hasMore:d.hasMore,nextCursor:d.nextCursor,queryHash:d.queryHash}:{provider:d.provider,metric:d.metric,fields,record:(({id,scheduled_at,visit_completed_at,status,version,href})=>({id,scheduled_at,visit_completed_at,status,version,href}))(d.record as Open['record']),queryHash:d.queryHash};
  return result as A extends 'search'?Search:Open;
 };
}
export async function nativeFieldEvidence<A extends 'search'|'open'>(actor:WorkContext,action:A,input:unknown){
 const raw=process.env.ERPX_NATIVE_FIELD_SERVICE_SOURCE_RECORD_CONFIG;if(!raw)fail('NATIVE_FIELD_EVIDENCE_CONFIGURATION_REQUIRED');
 return createNativeFieldEvidenceReader(raw)(await headers(),actor,action,input);
}
