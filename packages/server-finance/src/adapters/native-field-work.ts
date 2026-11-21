import 'server-only';
import {Agent,request} from 'node:http';
import {Readable} from 'node:stream';
import {cache} from 'react';
import {headers} from 'next/headers';
import {holdingHostForHost} from '@erpx/contracts/holding-hosts';
import type {FieldFinancialWorkContribution,FlowView} from '@erpx/contracts/command-flow';
import type {FinanceFieldWorkResponse} from '@erpx/contracts/finance-journal-read';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {ApiError} from '@erpx/server-runtime/public/http';

type Transport=(url:URL,headers:Headers,signal:AbortSignal)=>Promise<Response>;
type Settings={privateBaseUrl:string;timeoutMs:number;holdings:{origin:string;holdingSlug:string;holdingId:string}[]};
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0;
const integer=(v:unknown,min=0):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=min;
const strings=(v:unknown):v is string[]=>Array.isArray(v)&&v.every(x=>typeof x==='string');
const sameSet=(a:readonly string[],b:unknown)=>strings(b)&&a.length===b.length&&new Set(a).size===new Set(b).size&&a.every(v=>b.includes(v));
function fail(status=503,code='NATIVE_FINANCIAL_WORK_UNAVAILABLE'):never{throw new ApiError(status,code,'The native financial Work contribution is unavailable.');}
function configuration(raw:string):Settings{
 let v:unknown;try{v=JSON.parse(raw);}catch{return fail();}
 if(!object(v)||!text(v.privateBaseUrl)||!Array.isArray(v.holdings)||!v.holdings.length||v.holdings.length>100)return fail();
 let base:URL;try{base=new URL(v.privateBaseUrl);}catch{return fail();}
 const port=Number(base.port),timeout=v.timeoutMs??3000;
 if(base.protocol!=='http:'||!['127.0.0.1','[::1]'].includes(base.hostname)||v.privateBaseUrl!==base.origin||base.username||base.password||!integer(port,1024)||port>65535||[3000,3001,3100,3101,3103,5432,5433,5435,8080].includes(port)||port>=3200&&port<=3299||!integer(timeout,100)||timeout>10000)return fail();
 const origins=new Set(),slugs=new Set();
 for(const h of v.holdings){
  if(!object(h)||!text(h.origin)||!text(h.holdingSlug)||!/^[a-z0-9_-]{1,128}$/.test(h.holdingSlug)||!text(h.holdingId)||Buffer.byteLength(h.holdingId)>200)return fail();
  let origin:URL;try{origin=new URL(h.origin);}catch{return fail();}
  const contributor=holdingHostForHost(origin.host,'contributor');
  if(origin.protocol!=='https:'||origin.origin!==h.origin||origin.username||origin.password||origin.port&&(!contributor||contributor.holdingId!==h.holdingId||contributor.holdingSlug!==h.holdingSlug)||origins.has(origin.host)||slugs.has(h.holdingSlug))return fail();
  origins.add(origin.host);slugs.add(h.holdingSlug);
 }
 return {...v,timeoutMs:timeout} as Settings;
}
function flow(value:unknown,id:string,kind:string,status:string):value is FlowView{
 const person=(p:unknown)=>object(p)&&text(p.userId)&&typeof p.name==='string'&&(p.teamId===null||typeof p.teamId==='string')&&typeof p.teamName==='string';
 const step=(s:unknown)=>object(s)&&text(s.key)&&typeof s.action==='string'&&text(s.role)&&strings(s.allowedRoles)&&typeof s.teamName==='string'&&(s.primary===null||person(s.primary))&&Array.isArray(s.candidates)&&s.candidates.length<=500&&s.candidates.every(person)&&['current','upcoming','recorded'].includes(String(s.status))&&(s.blocker===null||typeof s.blocker==='string');
 return object(value)&&value.kind===kind&&value.recordId===id&&value.state===status&&integer(value.recordVersion,1)&&integer(value.version)&&integer(value.defaultsRevision)&&typeof value.canAct==='boolean'&&typeof value.canManage==='boolean'&&(value.blocker===null||typeof value.blocker==='string')&&Array.isArray(value.steps)&&value.steps.length===(kind==='invoice'?2:1)&&value.steps.every(step)&&(value.current===null||step(value.current))&&(value.next===null||step(value.next))&&Array.isArray(value.history)&&value.history.length<=10000&&value.history.every(h=>object(h)&&text(h.action)&&typeof h.actorName==='string'&&integer(h.at)&&typeof h.reason==='string'&&typeof h.step==='string');
}
function validate(value:unknown,actor:WorkContext):FieldFinancialWorkContribution[]{
 if(!object(value)||Object.keys(value).sort().join(',')!=='context,data'||!object(value.context)||!Array.isArray(value.data)||value.data.length>10000||!actor.authority||!actor.iam)return fail();
 const c=value.context,expected={userId:actor.userId,holdingId:actor.holdingId,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,branchSlug:actor.scope.kind==='holding'?null:actor.scope.branchSlug??null,departmentSlug:actor.scope.kind==='department'?actor.scope.departmentSlug:null,assignmentId:actor.assignmentId,assignmentVersion:actor.authority.assignmentVersion,revision:actor.authority.revision,role:actor.role,assignmentScopeKind:actor.assignmentScope};
 if(Object.keys(c).sort().join(',')!==[...Object.keys(expected),'modules','permissions','expiresAt'].sort().join(',')||Object.entries(expected).some(([k,v])=>c[k]!==v)||!sameSet(actor.modules,c.modules)||!sameSet(actor.iam.permissions,c.permissions)||!integer(c.expiresAt,1)||c.expiresAt<=Date.now())return fail(409,'WORK_CONTEXT_CHANGED');
 const allowed=['id','type','actionKind','title','detail','href','status','updatedAt','queue','flow','canAct','category','setupReason','relationships','nextAction','blocker','responsibleQueue'];
 const seen=new Set();
 for(const row of value.data){
  if(!object(row)||Object.keys(row).length!==allowed.length||Object.keys(row).some(k=>!allowed.includes(k))||!text(row.id)||!['invoice','journal'].includes(String(row.type))||row.actionKind!=='work'||!['title','detail','href','status','nextAction','responsibleQueue'].every(k=>typeof row[k]==='string')||!integer(row.updatedAt)||!['todo','waiting','completed'].includes(String(row.queue))||typeof row.canAct!=='boolean'||!['operational','setup'].includes(String(row.category))||!(row.setupReason===null||typeof row.setupReason==='string')||!(row.blocker===null||typeof row.blocker==='string')||!Array.isArray(row.relationships)||!row.relationships.every(r=>['requested','assigned'].includes(String(r)))||!flow(row.flow,row.id,String(row.type),String(row.status))||row.flow.canAct!==row.canAct||row.href!==`/finance/${row.type==='invoice'?'invoices':'journals'}/${encodeURIComponent(row.id)}`)return fail();
  const key=JSON.stringify([row.type,row.id]);if(seen.has(key))return fail();seen.add(key);
 }
 // The generated wire DTO is deliberately narrowed only after context and
 // read-model validation; no grant or queue decision is manufactured here.
 const wire=value as unknown as FinanceFieldWorkResponse;
 return wire.data as unknown as FieldFinancialWorkContribution[];
}
function direct(url:URL,headers:Headers,signal:AbortSignal):Promise<Response>{return new Promise((resolve,reject)=>{
 const agent=new Agent({keepAlive:false,maxSockets:1,maxTotalSockets:1,proxyEnv:{} as import('node:http').ProxyEnv});
 const req=request(url,{method:'GET',headers:Object.fromEntries(headers),signal,agent,maxHeaderSize:8192},incoming=>{
  incoming.once('close',()=>agent.destroy());
  try{const output=new Headers();for(let i=0;i<incoming.rawHeaders.length;i+=2)output.append(incoming.rawHeaders[i]!,incoming.rawHeaders[i+1]!);resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>,{status:incoming.statusCode??502,headers:output}));}catch(error){incoming.destroy();agent.destroy();reject(error);}
 });req.once('error',error=>{agent.destroy();reject(error);});req.end();
});}
export function createNativeFieldFinancialWorkReader(raw:string,transport:Transport=direct){
 const config=configuration(raw);
 return async(incoming:Pick<Headers,'get'>,actor:WorkContext):Promise<FieldFinancialWorkContribution[]>=>{
  const target=config.holdings.find(h=>new URL(h.origin).host===incoming.get('host')&&h.holdingSlug===actor.holdingSlug);
  if(!target||target.holdingId!==actor.holdingId||!actor.authority||!actor.iam)return fail(409,'WORK_CONTEXT_CHANGED');
  const h=new Headers({host:new URL(target.origin).host,accept:'application/json','cache-control':'no-cache','x-erpx-context-company':actor.companyId,'x-erpx-context-assignment':actor.assignmentId,'x-erpx-context-revision':String(actor.authority.revision),'x-erpx-context-assignment-version':String(actor.authority.assignmentVersion)});
  const cookie=incoming.get('cookie');if(cookie){if(Buffer.byteLength(cookie)>8192)return fail();h.set('cookie',cookie);}
  const origin=incoming.get('origin');if(origin!==null){if(origin!==target.origin)return fail(403,'ORIGIN_FORBIDDEN');h.set('origin',origin);}
  if(actor.scope.kind!=='holding'&&actor.scope.branchSlug)h.set('x-erpx-workspace-branch',actor.scope.branchSlug);
  if(actor.scope.kind==='department')h.set('x-erpx-workspace-department',actor.scope.departmentSlug);
  const controller=new AbortController(),timer=setTimeout(()=>controller.abort(),config.timeoutMs);
  try{
   const response=await transport(new URL(`/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/finance/field-work`,config.privateBaseUrl),h,controller.signal);
   if(response.redirected||response.status!==200){void response.body?.cancel().catch(()=>undefined);return fail([401,403,404,409].includes(response.status)?response.status:503);}
   if(response.headers.get('content-type')?.split(';')[0]?.trim()!=='application/json'||!response.body)return fail();
   const reader=response.body.getReader(),chunks:Uint8Array[]=[];let length=0;
   const cancel=()=>{void reader.cancel().catch(()=>undefined);};controller.signal.addEventListener('abort',cancel,{once:true});
   try{for(;;){const p=await reader.read();if(controller.signal.aborted)return fail();if(p.done)break;length+=p.value.length;if(length>2*1024*1024){await reader.cancel();return fail();}chunks.push(p.value);}}finally{controller.signal.removeEventListener('abort',cancel);reader.releaseLock();}
   const body=new Uint8Array(length);let offset=0;for(const chunk of chunks){body.set(chunk,offset);offset+=chunk.length;}
   let value:unknown;try{value=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(body));}catch{return fail();}
   return validate(value,actor);
  }catch(error){if(error instanceof ApiError)throw error;return fail();}finally{clearTimeout(timer);}
 };
}
/** Temporary Next transport for the shared Work renderer. Remove it when
 * that renderer consumes the native Work owner directly; no business fallback. */
export const nativeFieldFinancialWork=cache(async(actor:WorkContext):Promise<FieldFinancialWorkContribution[]|undefined>=>{
 const raw=process.env.ERPX_NATIVE_FINANCE_SERVICE_INVOICE_CONFIG;if(raw===undefined)return undefined;
 if(process.env.NODE_ENV==='production')return fail();
 return createNativeFieldFinancialWorkReader(raw)(await headers(),actor);
});
