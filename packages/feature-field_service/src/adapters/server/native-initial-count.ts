import 'server-only';
import {nativeInitialCountRequest,type NativeCountRoute,type NativeCountTransport} from '@erpx/native-sdk/field-service-initial-count';
import type {ServiceInitialCountReadResponse,ServiceInitialCountCommandResponse} from '@erpx/contracts/field-service-initial-count';
import {validServiceInitialPhysicalStockCountView,type ServiceInitialPhysicalCountView} from '../../lib/initial-physical-stock-count-command';
import type {FlowView} from '@erpx/feature-shared/lib/flow';
import type {ReserveContext} from '../../lib/native-reserve-command';
type Target={origin:string;holdingSlug:string;holdingId:string};
type Settings={privateBaseUrl:string;timeoutMs:number;holdings:Target[]};
export class NativeInitialCountError extends Error {constructor(readonly code:string,readonly status=502){super('The native Service initial Count is unavailable.');}}
const fail=(code:string,status=502):never=>{throw new NativeInitialCountError(code,status);};
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const setEqual=(expected:readonly string[],actual:unknown)=>Array.isArray(actual)&&actual.every(x=>typeof x==='string')&&new Set(expected).size===new Set(actual).size&&expected.every(x=>actual.includes(x));
function settings(raw:string):Settings{
 let value:unknown;try{value=JSON.parse(raw);}catch{return fail('NATIVE_INITIAL_COUNT_CONFIG_INVALID');}
 if(!object(value)||typeof value.privateBaseUrl!=='string'||typeof value.timeoutMs!=='number'||!Number.isSafeInteger(value.timeoutMs)||value.timeoutMs<250||value.timeoutMs>30000||!Array.isArray(value.holdings)||!value.holdings.length||value.holdings.length>64)return fail('NATIVE_INITIAL_COUNT_CONFIG_INVALID');
 let base:URL;try{base=new URL(value.privateBaseUrl);}catch{return fail('NATIVE_INITIAL_COUNT_CONFIG_INVALID');}const port=Number(base.port);
 if(base.protocol!=='http:'||!['127.0.0.1','[::1]'].includes(base.hostname)||!base.port||[3000,3001,3100,3101,3103,5432,5433,5435,8080].includes(port)||port>=3200&&port<=3299||base.username||base.password||base.origin!==value.privateBaseUrl)return fail('NATIVE_INITIAL_COUNT_CONFIG_INVALID');
 const hosts=new Set<string>(),slugs=new Set<string>();for(const entry of value.holdings){if(!object(entry)||typeof entry.origin!=='string'||typeof entry.holdingSlug!=='string'||!/^[a-z0-9_-]{1,128}$/.test(entry.holdingSlug)||typeof entry.holdingId!=='string'||!entry.holdingId||new TextEncoder().encode(entry.holdingId).length>200)return fail('NATIVE_INITIAL_COUNT_CONFIG_INVALID');let origin:URL;try{origin=new URL(entry.origin);}catch{return fail('NATIVE_INITIAL_COUNT_CONFIG_INVALID');}if(origin.protocol!=='https:'||origin.origin!==entry.origin||origin.port||origin.username||origin.password||!origin.hostname.includes('.')||!origin.hostname.split('.').every(part=>/^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(part))||hosts.has(origin.host)||slugs.has(entry.holdingSlug))return fail('NATIVE_INITIAL_COUNT_CONFIG_INVALID');hosts.add(origin.host);slugs.add(entry.holdingSlug);}
 return value as Settings;
}
function selected(raw:string|undefined,headers:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined):{config:Settings;target:Target}|undefined{
 if(raw===undefined)return undefined;const config=settings(raw),host=headers.get('host');const target=config.holdings.find(value=>value.holdingSlug===slug);if(!target)return undefined;if(new URL(target.origin).host!==host)return fail('NATIVE_INITIAL_COUNT_HOST_INVALID',404);if(!actor||target.holdingId!==actor.holdingId)return fail('WORK_CONTEXT_CHANGED',409);return {config,target};
}
function validatedContext(value:unknown,actor:ReserveContext){if(!object(value))return fail('WORK_CONTEXT_CHANGED',409);for(const [key,expected] of Object.entries(actor)){if(key==='modules'||key==='permissions')continue;if(value[key]!==expected)return fail('WORK_CONTEXT_CHANGED',409);}if(!setEqual(actor.modules,value.modules)||!setEqual(actor.permissions,value.permissions)||typeof value.expiresAt!=='number'||!Number.isSafeInteger(value.expiresAt)||value.expiresAt<=Date.now())return fail('WORK_CONTEXT_CHANGED',409);}

const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0;
const nullable=(v:unknown)=>v===null||text(v);
const integer=(v:unknown)=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=0;
function validFlow(value:unknown,id:string):value is FlowView{
 if(!object(value)||value.kind!=='stock_count'||value.recordId!==id||!integer(value.recordVersion)||Number(value.recordVersion)<1||!integer(value.version)||!integer(value.defaultsRevision)||!text(value.state)||typeof value.canAct!=='boolean'||typeof value.canManage!=='boolean'||!nullable(value.blocker))return false;
 const person=(p:unknown)=>object(p)&&text(p.userId)&&text(p.name)&&nullable(p.teamId)&&typeof p.teamName==='string';
 const step=(s:unknown)=>object(s)&&text(s.key)&&text(s.action)&&text(s.role)&&Array.isArray(s.allowedRoles)&&s.allowedRoles.every(text)&&typeof s.teamName==='string'&&(s.primary===null||person(s.primary))&&Array.isArray(s.candidates)&&s.candidates.every(person)&&['current','upcoming','recorded'].includes(String(s.status))&&nullable(s.blocker);
 return Array.isArray(value.steps)&&value.steps.every(step)&&(value.current===null||step(value.current))&&(value.next===null||step(value.next))&&Array.isArray(value.history)&&value.history.every(h=>object(h)&&text(h.action)&&text(h.actorName)&&typeof h.at==='number'&&Number.isSafeInteger(h.at)&&typeof h.reason==='string'&&typeof h.step==='string');
}
export function nativeInitialCountSelected(headers:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined):boolean{return !!selected(process.env.ERPX_NATIVE_FIELD_SERVICE_INITIAL_COUNT_CONFIG,headers,slug,actor);}
export type NativeInitialCountReadRoute=Extract<NativeCountRoute,{kind:'selection'|'record'|'flow'}>;
export type NativeInitialCountCommandRoute=Extract<NativeCountRoute,{kind:'create'|'update'|'action'|'assign'}>;
export function nativeInitialCount(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,route:Extract<NativeInitialCountReadRoute,{kind:'selection'|'record'}>,body?:undefined,key?:undefined,transport?:NativeCountTransport):Promise<ServiceInitialPhysicalCountView|undefined>;
export function nativeInitialCount(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,route:Extract<NativeInitialCountReadRoute,{kind:'flow'}>,body?:undefined,key?:undefined,transport?:NativeCountTransport):Promise<FlowView|undefined>;
export function nativeInitialCount(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,route:NativeInitialCountReadRoute,body?:undefined,key?:undefined,transport?:NativeCountTransport):Promise<ServiceInitialPhysicalCountView|FlowView|undefined>;
export function nativeInitialCount(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,route:NativeInitialCountCommandRoute,body:string,key:string,transport?:NativeCountTransport):Promise<Record<string,unknown>|undefined>;
export async function nativeInitialCount(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,route:NativeCountRoute,body?:string,key?:string,transport?:NativeCountTransport):Promise<ServiceInitialPhysicalCountView|FlowView|Record<string,unknown>|undefined>{
 const choice=selected(process.env.ERPX_NATIVE_FIELD_SERVICE_INITIAL_COUNT_CONFIG,incoming,slug,actor);if(!choice)return undefined;if(!actor)return fail('WORK_CONTEXT_CHANGED',409);
 const command=route.kind==='create'||route.kind==='update'||route.kind==='action'||route.kind==='assign',headers=new Headers({host:new URL(choice.target.origin).host,accept:'application/json','cache-control':'no-cache'}),cookie=incoming.get('cookie');if(cookie){if(new TextEncoder().encode(cookie).length>8192)return fail('NATIVE_INITIAL_COUNT_COOKIE_LIMIT');headers.set('cookie',cookie);}
 for(const [name,value] of [['x-erpx-context-company',actor.companyId],['x-erpx-context-assignment',actor.assignmentId],['x-erpx-context-revision',String(actor.revision)],['x-erpx-context-assignment-version',String(actor.assignmentVersion)]] as const){const captured=incoming.get(name);if(command&&captured!==value)return fail('WORK_CONTEXT_CHANGED',409);headers.set(name,command?captured!:value);}
 if(actor.branchSlug)headers.set('x-erpx-workspace-branch',actor.branchSlug);if(actor.departmentSlug)headers.set('x-erpx-workspace-department',actor.departmentSlug);
 if(command){if(!key||body===undefined)return fail('NATIVE_INITIAL_COUNT_KEY_REQUIRED',422);const origin=incoming.get('origin');if(origin){if(origin!==choice.target.origin)return fail('ORIGIN_FORBIDDEN',403);headers.set('origin',origin);}headers.set('idempotency-key',key);headers.set('content-type','application/json');}
 let response:Awaited<ReturnType<typeof nativeInitialCountRequest>>;try{response=await nativeInitialCountRequest({baseUrl:choice.config.privateBaseUrl,timeoutMs:choice.config.timeoutMs,slug,route,headers,...(command?{body}: {})},transport);}catch{return fail('NATIVE_INITIAL_COUNT_UNAVAILABLE',503);}
 if(response.status!==(route.kind==='create'?201:200)){if(object(response.body)&&object(response.body.error)&&typeof response.body.error.code==='string'&&response.body.error.code.length<=100&&response.status>=400&&response.status<=599)return fail(response.body.error.code,response.status);return fail('NATIVE_INITIAL_COUNT_UNAVAILABLE',503);}
 if(!object(response.body))return fail('NATIVE_INITIAL_COUNT_RESPONSE_INVALID');validatedContext(response.body.context,actor);const data=response.body.data;
 if(!command){const typed=response.body as unknown as ServiceInitialCountReadResponse;if(route.kind==='flow'){if(!validFlow(typed.data,route.id))return fail('NATIVE_INITIAL_COUNT_RESPONSE_INVALID');return typed.data;}if(!validServiceInitialPhysicalStockCountView(typed.data,actor.companyId)||route.kind==='record'&&typed.data.available&&typed.data.record?.id!==route.id)return fail('NATIVE_INITIAL_COUNT_RESPONSE_INVALID');return typed.data;}
 let input:unknown;try{input=JSON.parse(body!);}catch{return fail('NATIVE_INITIAL_COUNT_REQUEST_INVALID',422);}if(!object(input)||!object(data))return fail('NATIVE_INITIAL_COUNT_RESPONSE_INVALID');
 if(route.kind==='create'){if(Object.keys(data).sort().join(',')!=='id,status,version'||data.id!==`count-${key}`||data.status!=='draft'||data.version!==1)return fail('NATIVE_INITIAL_COUNT_RESPONSE_INVALID');}
 else if(route.kind==='update'){if(Object.keys(data).sort().join(',')!=='id,status,version'||data.id!==route.id||data.status!=='draft'||!integer(input.expectedVersion)||!integer(data.version)||data.version!==Number(input.expectedVersion)+1)return fail('NATIVE_INITIAL_COUNT_RESPONSE_INVALID');}
 else if(route.kind==='assign'){if(Object.keys(data).sort().join(',')!=='id,version'||data.id!==route.id||!integer(input.expectedVersion)||!integer(data.version)||data.version!==(Number(input.expectedVersion)||1)+1)return fail('NATIVE_INITIAL_COUNT_RESPONSE_INVALID');}
 else {const statuses={submit:'submitted',approve:'approved',post:'posted',reject:'rejected',cancel:'cancelled'} as const;if(Object.keys(data).sort().join(',')!=='id,journalId,status,version'||data.id!==route.id||data.status!==statuses[route.action]||!integer(input.expectedVersion)||!integer(data.version)||data.version!==Number(input.expectedVersion)+1||!nullable(data.journalId))return fail('NATIVE_INITIAL_COUNT_RESPONSE_INVALID');}
 const typed=response.body as unknown as ServiceInitialCountCommandResponse;return typed.data;
}
