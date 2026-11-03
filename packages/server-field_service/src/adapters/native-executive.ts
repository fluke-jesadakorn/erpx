import 'server-only';
import {headers} from 'next/headers';
import {nativeHoldingConfiguration} from '@erpx/native-sdk/holding-target';
import {nativeFieldExecutiveRequest,type NativeFieldExecutiveQuery} from '@erpx/native-sdk/field-service-parts';
const originProfile=()=>process.env.NODE_ENV==='development'&&process.env.NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE==='container-localhost'?'contributor' as const:'public' as const;
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {RawMetricRow} from '@erpx/server-runtime/public/executive/metric-projection-contract';
import type {MetricSourceRow} from '@erpx/server-runtime/public/executive/metric-sources';
import type {FieldVisitEvidenceRow} from '@erpx/server-runtime/public/executive/evidence-contract';
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0;
const integer=(v:unknown):v is number=>typeof v==='number'&&Number.isSafeInteger(v);
const count=(v:unknown):v is string=>typeof v==='string'&&/^(0|[1-9][0-9]{0,15})$/.test(v)&&BigInt(v)<=BigInt(Number.MAX_SAFE_INTEGER);
const setEqual=(a:readonly string[],b:unknown)=>Array.isArray(b)&&b.every(text)&&new Set(a).size===new Set(b).size&&a.every(v=>b.includes(v));
function fail(code:string,status=503):never{throw new ApiError(status,code,'The native Field report is unavailable.');}
type Source=MetricSourceRow&{label:string};
export type NativeFieldReport={available?:boolean;raw:RawMetricRow[];sources:Source[];evidence:FieldVisitEvidenceRow[]};
/** Bounded private transport and DTO mapping only. No Field SQL, source policy,
 * aggregate calculation, authorization snapshot or legacy fallback lives here. */
export function createNativeFieldReportReader(raw:string,send:typeof nativeFieldExecutiveRequest=nativeFieldExecutiveRequest){
 let config:ReturnType<typeof nativeHoldingConfiguration>;try{config=nativeHoldingConfiguration(raw,originProfile());}catch{return fail('NATIVE_FIELD_REPORT_CONFIGURATION_INVALID');}
 return async(incoming:Pick<Headers,'get'>,actor:WorkContext,query:NativeFieldExecutiveQuery):Promise<NativeFieldReport>=>{
  const target=config.holdings.find(h=>new URL(h.origin).host===incoming.get('host')&&h.holdingId===actor.holdingId&&h.holdingSlug===actor.holdingSlug);
  if(!target||!actor.authority||!actor.iam)fail('WORK_CONTEXT_CHANGED',409);
  const origin=incoming.get('origin');if(origin&&origin!==target.origin)fail('ORIGIN_FORBIDDEN',403);
  const h=new Headers({host:new URL(target.origin).host,accept:'application/json','cache-control':'no-cache'}),cookie=incoming.get('cookie');
  if(cookie){if(new TextEncoder().encode(cookie).length>8192)fail('NATIVE_FIELD_REPORT_COOKIE_LIMIT');h.set('cookie',cookie);}if(origin)h.set('origin',origin);
  h.set('x-erpx-context-company',actor.companyId);h.set('x-erpx-context-assignment',actor.assignmentId);h.set('x-erpx-context-revision',String(actor.authority.revision));h.set('x-erpx-context-assignment-version',String(actor.authority.assignmentVersion));
  if(actor.scope.kind!=='holding'&&actor.scope.branchSlug)h.set('x-erpx-workspace-branch',actor.scope.branchSlug);if(actor.scope.kind==='department')h.set('x-erpx-workspace-department',actor.scope.departmentSlug);
  let reply:Awaited<ReturnType<typeof send>>;try{reply=await send({baseUrl:config.privateBaseUrl,slug:target.holdingSlug,headers:h,timeoutMs:config.timeoutMs,query});}catch{return fail('NATIVE_FIELD_REPORT_UNAVAILABLE');}
  const b=reply.body;if(reply.status!==200){if(object(b)&&object(b.error)&&text(b.error.code))fail(b.error.code,[401,403,404,409,422].includes(reply.status)?reply.status:503);fail('NATIVE_FIELD_REPORT_UNAVAILABLE');}
  if(!object(b)||!object(b.context)||!object(b.data))fail('NATIVE_FIELD_REPORT_RESPONSE_INVALID',502);
  const c=b.context,expected={holdingId:actor.holdingId,userId:actor.userId,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,assignmentId:actor.assignmentId,assignmentVersion:actor.authority.assignmentVersion,revision:actor.authority.revision,role:actor.role};
  if(Object.entries(expected).some(([k,v])=>c[k]!==v)||!setEqual(actor.modules,c.modules)||!setEqual(actor.iam.permissions,c.permissions)||!integer(c.expiresAt)||c.expiresAt<=Date.now())fail('WORK_CONTEXT_CHANGED',409);
  const result:NativeFieldReport={raw:[],sources:[],evidence:[]},data=b.data;
  if(query.accessOnly){
   if(typeof data.available!=='boolean')fail('NATIVE_FIELD_REPORT_RESPONSE_INVALID',502);result.available=data.available;
  }else if(query.evidenceLimit!==undefined){
   if(!Array.isArray(data.evidence)||data.evidence.length>query.evidenceLimit+1)fail('NATIVE_FIELD_REPORT_RESPONSE_INVALID',502);
   const ids=new Set<string>();for(const r of data.evidence){if(!object(r)||!text(r.id)||ids.has(r.id)||!integer(r.scheduled_at)||!(r.visit_completed_at===null||integer(r.visit_completed_at))||!text(r.status)||!integer(r.version)||r.version<1)fail('NATIVE_FIELD_REPORT_RESPONSE_INVALID',502);ids.add(r.id);result.evidence.push({id:r.id,scheduled_at:r.scheduled_at,visit_completed_at:r.visit_completed_at,status:r.status,version:r.version});}
  }else{
   if(!Array.isArray(data.raw)||data.raw.length>1000||!Array.isArray(data.sources)||data.sources.length>1000)fail('NATIVE_FIELD_REPORT_RESPONSE_INVALID',502);
   const labels=new Set<string>();for(const r of data.raw){if(!object(r)||!text(r.label)||r.label.length>3000||labels.has(r.label))fail('NATIVE_FIELD_REPORT_RESPONSE_INVALID',502);labels.add(r.label);if(query.metric==='field-visit-completion'){if(!count(r.numerator)||!count(r.denominator)||BigInt(r.numerator)>BigInt(r.denominator))fail('NATIVE_FIELD_REPORT_RESPONSE_INVALID',502);result.raw.push({label:r.label,numerator:r.numerator,denominator:r.denominator});}else{if(!count(r.value))fail('NATIVE_FIELD_REPORT_RESPONSE_INVALID',502);result.raw.push({label:r.label,value:r.value});}}
   const ids=new Set<string>();for(const r of data.sources){if(!object(r)||!text(r.id)||ids.has(r.id)||!text(r.label)||!labels.has(r.label)||!object(r.facts)||r.facts.id!==r.id||!text(r.facts.status)||!integer(r.facts.version)||r.facts.version<1||!integer(r.facts.created_at)||!integer(r.facts.updated_at)||r.owner_kind!=='field_service'||r.owner_id!==r.id||r.owner_args!==null)fail('NATIVE_FIELD_REPORT_RESPONSE_INVALID',502);ids.add(r.id);result.sources.push({id:r.id,label:r.label,facts:{id:r.id,status:r.facts.status,version:r.facts.version,created_at:r.facts.created_at,updated_at:r.facts.updated_at},owner_kind:'field_service',owner_id:r.id,owner_args:null});}
  }
  return result;
 };
}
export async function readNativeFieldReport(actor:WorkContext,query:NativeFieldExecutiveQuery){
 const raw=process.env.ERPX_NATIVE_FIELD_SERVICE_SOURCE_RECORD_CONFIG;if(!raw)fail('NATIVE_FIELD_REPORT_CONFIGURATION_REQUIRED');
 return createNativeFieldReportReader(raw)(await headers(),actor,query);
}

export async function nativeFieldReportAccess(actor:WorkContext){
 try{return (await readNativeFieldReport(actor,{accessOnly:true,metric:'open-service-orders',breakdown:'default',fromAt:0,toAt:0})).available===true;}
 catch(error){if(error instanceof ApiError&&error.status===403&&['PERMISSION_REQUIRED','MODULE_FORBIDDEN'].includes(error.code))return false;throw error;}
}
