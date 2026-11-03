import {validServiceCustomerPreparation,type ServiceCustomerPreparation} from '../../lib/customer-preparation-model';
import {validFieldOperationsWorkspace,validFieldOperationsDetailRead,matchesOperationsReceipt} from '@erpx/feature-operations/lib/operations-work-model';
import {matchesCollaborationReceipt,validFieldCollaborationReadModel} from '@erpx/feature-collaboration/lib/collaboration-work-read-model';
import 'server-only';
import {allowedPrivateLoopbackPort} from '@erpx/native-sdk/loopback';
import {contributorExactOrigin} from '@erpx/feature-runtime/lib/holding-hosts';
import {nativeServiceCustomerPreparationRequest,nativeFieldOperationsRequest,nativeFieldSupportRequest,nativeFieldCollaborationRequest,nativePartsSetupRequest,nativePartsPrerequisiteRequest,nativePartsStockRequest,type NativePartsStockOperation,type NativePartsPrerequisiteOperation} from '@erpx/native-sdk/field-service-parts';
import {validPostingPrerequisites} from '@erpx/feature-commerce/lib/posting-prerequisite-read-model';
import {validServicePartsReceipt,validServicePartsStockWorkspace,type ServicePartsRequest,type ServicePartsPrerequisitesView} from '../../lib/parts-setup-command';
import type {ServiceStockReadResponse,ServiceStockMasterResponse,ServicePrerequisitesReadResponse} from '@erpx/contracts/field-service-reserve';
import {validServicePartsPostingSetup,type ServicePartsPostingSetupView} from '../../lib/parts-setup-model';
import type {ServicePostingReadResponse,ServicePostingCommandResponse} from '@erpx/contracts/field-service-reserve';
import type {ReserveContext} from '../../lib/native-reserve-command';

type Target={origin:string;holdingSlug:string;holdingId:string};
type Settings={privateBaseUrl:string;timeoutMs:number;holdings:Target[];production?:boolean};
export class NativePartsSetupError extends Error {constructor(readonly code:string,readonly status=502){super('The native Service parts setup is unavailable.');}}
const fail=(code:string,status=502):never=>{throw new NativePartsSetupError(code,status);};
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const setEqual=(expected:readonly string[],actual:unknown)=>Array.isArray(actual)&&actual.every(x=>typeof x==='string')&&new Set(expected).size===new Set(actual).size&&expected.every(x=>actual.includes(x));
function settings(raw:string):Settings{
 let value:unknown;try{value=JSON.parse(raw);}catch{return fail('NATIVE_PARTS_SETUP_CONFIG_INVALID');}
 if(!object(value)||(value.production!==undefined&&typeof value.production!=='boolean')||typeof value.privateBaseUrl!=='string'||typeof value.timeoutMs!=='number'||!Number.isSafeInteger(value.timeoutMs)||value.timeoutMs<250||value.timeoutMs>30000||!Array.isArray(value.holdings)||!value.holdings.length||value.holdings.length>64)return fail('NATIVE_PARTS_SETUP_CONFIG_INVALID');
 let base:URL;try{base=new URL(value.privateBaseUrl);}catch{return fail('NATIVE_PARTS_SETUP_CONFIG_INVALID');}const port=Number(base.port);
 if(base.protocol!=='http:'||!['127.0.0.1','[::1]'].includes(base.hostname)||!base.port||!allowedPrivateLoopbackPort(port,value.production===true)||port>=3200&&port<=3299||base.username||base.password||base.origin!==value.privateBaseUrl)return fail('NATIVE_PARTS_SETUP_CONFIG_INVALID');
 const hosts=new Set<string>(),slugs=new Set<string>();for(const entry of value.holdings){if(!object(entry)||typeof entry.origin!=='string'||typeof entry.holdingSlug!=='string'||!/^[a-z0-9_-]{1,128}$/.test(entry.holdingSlug)||typeof entry.holdingId!=='string'||!entry.holdingId||new TextEncoder().encode(entry.holdingId).length>200)return fail('NATIVE_PARTS_SETUP_CONFIG_INVALID');let origin:URL;try{origin=new URL(entry.origin);}catch{return fail('NATIVE_PARTS_SETUP_CONFIG_INVALID');}if(origin.protocol!=='https:'||origin.origin!==entry.origin||(origin.port&&contributorExactOrigin(origin.host)!==entry.origin)||origin.username||origin.password||!origin.hostname.includes('.')||!origin.hostname.split('.').every(part=>/^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(part))||hosts.has(origin.host)||slugs.has(entry.holdingSlug))return fail('NATIVE_PARTS_SETUP_CONFIG_INVALID');hosts.add(origin.host);slugs.add(entry.holdingSlug);}
 return value as Settings;
}
function selected(raw:string|undefined,headers:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined):{config:Settings;target:Target}|undefined{
 if(raw===undefined)return undefined;const config=settings(raw),host=headers.get('host');const target=config.holdings.find(value=>value.holdingSlug===slug);if(!target)return undefined;if(new URL(target.origin).host!==host)return fail('NATIVE_PARTS_SETUP_HOST_INVALID',404);if(!actor||target.holdingId!==actor.holdingId)return fail('WORK_CONTEXT_CHANGED',409);return {config,target};
}
export function nativePartsSetupSelected(headers:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined):boolean{return !!selected(process.env.ERPX_NATIVE_FIELD_SERVICE_PARTS_SETUP_CONFIG,headers,slug,actor);}
function validatedContext(value:unknown,actor:ReserveContext){if(!object(value))return fail('WORK_CONTEXT_CHANGED',409);for(const [key,expected] of Object.entries(actor)){if(key==='modules'||key==='permissions')continue;if(value[key]!==expected)return fail('WORK_CONTEXT_CHANGED',409);}if(!setEqual(actor.modules,value.modules)||!setEqual(actor.permissions,value.permissions)||typeof value.expiresAt!=='number'||!Number.isSafeInteger(value.expiresAt)||value.expiresAt<=Date.now())return fail('WORK_CONTEXT_CHANGED',409);}
export function nativePartsSetup(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,method:'GET'):Promise<ServicePartsPostingSetupView|undefined>;
export function nativePartsSetup(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,method:'POST',body:unknown,key:string,existing?:boolean):Promise<{id:string;status:string;version:number}|undefined>;
export async function nativePartsSetup(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,method:'GET'|'POST',body?:unknown,key?:string,existing=false):Promise<ServicePartsPostingSetupView|{id:string;status:string;version:number}|undefined>{
 const choice=selected(process.env.ERPX_NATIVE_FIELD_SERVICE_PARTS_SETUP_CONFIG,incoming,slug,actor);if(!choice)return undefined;if(!actor)return fail('WORK_CONTEXT_CHANGED',409);
 const headers=new Headers({host:new URL(choice.target.origin).host,accept:'application/json','cache-control':'no-cache'}),cookie=incoming.get('cookie');if(cookie){if(new TextEncoder().encode(cookie).length>8192)return fail('NATIVE_PARTS_SETUP_COOKIE_LIMIT');headers.set('cookie',cookie);}
 for(const [name,value] of [['x-erpx-context-company',actor.companyId],['x-erpx-context-assignment',actor.assignmentId],['x-erpx-context-revision',String(actor.revision)],['x-erpx-context-assignment-version',String(actor.assignmentVersion)]] as const){const captured=incoming.get(name);if(method==='POST'&&captured!==value)return fail('WORK_CONTEXT_CHANGED',409);headers.set(name,method==='POST'?captured!:value);}
 if(actor.branchSlug)headers.set('x-erpx-workspace-branch',actor.branchSlug);if(actor.departmentSlug)headers.set('x-erpx-workspace-department',actor.departmentSlug);
 if(method==='POST'){if(!key)return fail('NATIVE_PARTS_SETUP_KEY_REQUIRED',422);const origin=incoming.get('origin');if(origin){if(origin!==choice.target.origin)return fail('ORIGIN_FORBIDDEN',403);headers.set('origin',origin);}headers.set('idempotency-key',key);headers.set('content-type','application/json');}
 let response:Awaited<ReturnType<typeof nativePartsSetupRequest>>;try{response=await nativePartsSetupRequest({baseUrl:choice.config.privateBaseUrl,timeoutMs:choice.config.timeoutMs,slug,method,existing,headers,...(method==='POST'?{body:JSON.stringify(body)}:{})});}catch{return fail('NATIVE_PARTS_SETUP_UNAVAILABLE',503);}
 if(response.status!==200){if(object(response.body)&&object(response.body.error)&&typeof response.body.error.code==='string'&&response.body.error.code.length<=100&&response.status>=400&&response.status<=599)return fail(response.body.error.code,response.status);return fail('NATIVE_PARTS_SETUP_UNAVAILABLE',503);}
 if(!object(response.body))return fail('NATIVE_PARTS_SETUP_RESPONSE_INVALID');validatedContext(response.body.context,actor);const data=response.body.data;
 if(method==='GET'){const typed=response.body as unknown as ServicePostingReadResponse;if(!validServicePartsPostingSetup(typed.data,actor.companyId)||!Object.hasOwn(typed.data,'expectedSalesDefaults')||!Object.hasOwn(typed.data,'approvedSalesDefaults'))return fail('NATIVE_PARTS_SETUP_RESPONSE_INVALID');return typed.data;}
 if(!object(data)||Object.keys(data).sort().join(',')!=='id,status,version'||data.id!==actor.companyId||data.status!==(existing?'confirmed-existing':'configured')||data.version!==1)return fail('NATIVE_PARTS_SETUP_RESPONSE_INVALID');const typed=response.body as unknown as ServicePostingCommandResponse;return {id:typed.data.id,status:typed.data.status,version:typed.data.version};
}

export function nativePartsPrerequisitesSelected(headers:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined):boolean{return !!selected(process.env.ERPX_NATIVE_FIELD_SERVICE_PREREQUISITES_CONFIG,headers,slug,actor);}
export function nativePartsPrerequisites(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,operation:'read'):Promise<ServicePartsPrerequisitesView|undefined>;
export function nativePartsPrerequisites(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,operation:Exclude<NativePartsPrerequisiteOperation,'read'>,body:unknown,key:string):Promise<Record<string,unknown>|undefined>;
export async function nativePartsPrerequisites(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,operation:NativePartsPrerequisiteOperation,body?:unknown,key?:string):Promise<ServicePartsPrerequisitesView|Record<string,unknown>|undefined>{
 const choice=selected(process.env.ERPX_NATIVE_FIELD_SERVICE_PREREQUISITES_CONFIG,incoming,slug,actor);if(!choice)return undefined;if(!actor)return fail('WORK_CONTEXT_CHANGED',409);
 const headers=new Headers({host:new URL(choice.target.origin).host,accept:'application/json','cache-control':'no-cache'}),cookie=incoming.get('cookie');if(cookie){if(new TextEncoder().encode(cookie).length>8192)return fail('NATIVE_PARTS_SETUP_COOKIE_LIMIT');headers.set('cookie',cookie);}
 for(const [name,value] of [['x-erpx-context-company',actor.companyId],['x-erpx-context-assignment',actor.assignmentId],['x-erpx-context-revision',String(actor.revision)],['x-erpx-context-assignment-version',String(actor.assignmentVersion)]] as const){const captured=incoming.get(name);if(operation!=='read'&&captured!==value)return fail('WORK_CONTEXT_CHANGED',409);headers.set(name,operation==='read'?value:captured!);}
 if(actor.branchSlug)headers.set('x-erpx-workspace-branch',actor.branchSlug);if(actor.departmentSlug)headers.set('x-erpx-workspace-department',actor.departmentSlug);
 if(operation!=='read'){if(!key)return fail('NATIVE_PARTS_SETUP_KEY_REQUIRED',422);const origin=incoming.get('origin');if(origin){if(origin!==choice.target.origin)return fail('ORIGIN_FORBIDDEN',403);headers.set('origin',origin);}headers.set('idempotency-key',key);headers.set('content-type','application/json');}
 let response:Awaited<ReturnType<typeof nativePartsPrerequisiteRequest>>;try{response=await nativePartsPrerequisiteRequest({baseUrl:choice.config.privateBaseUrl,timeoutMs:choice.config.timeoutMs,slug,operation,headers,...(operation!=='read'?{body:JSON.stringify(body)}:{})});}catch{return fail('NATIVE_PARTS_SETUP_UNAVAILABLE',503);}
 if(response.status!==200){if(object(response.body)&&object(response.body.error)&&typeof response.body.error.code==='string'&&response.body.error.code.length<=100&&response.status>=400&&response.status<=599)return fail(response.body.error.code,response.status);return fail('NATIVE_PARTS_SETUP_UNAVAILABLE',503);}
 if(!object(response.body))return fail('NATIVE_PARTS_SETUP_RESPONSE_INVALID');validatedContext(response.body.context,actor);const data=response.body.data;
 if(operation==='read'){const typed=response.body as unknown as ServicePrerequisitesReadResponse;if(!validPostingPrerequisites(typed.data,actor.companyId)||![typed.data.charts,typed.data.accounts,typed.data.periods,typed.data.rates].every(rows=>rows.every(row=>Object.values(row).every(value=>value===null||typeof value==='string'||typeof value==='number'&&Number.isSafeInteger(value)))))return fail('NATIVE_PARTS_SETUP_RESPONSE_INVALID');return typed.data;}
 const kinds={chart:'charts',account:'accounts',period:'periods',rate:'rates'} as const;
 if(!object(body)||!object(body.input)||!validServicePartsReceipt(data,{kind:kinds[operation],body} as ServicePartsRequest,actor.companyId,key!)||operation==='account'&&(data.parentId??null)!==(body.input.parentId??null)||operation==='rate'&&!Number.isSafeInteger(data.confirmedAt))return fail('NATIVE_PARTS_SETUP_RESPONSE_INVALID');return data;
}

export function nativePartsStockSelected(headers:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined):boolean{return !!selected(process.env.ERPX_NATIVE_FIELD_SERVICE_STOCK_SETUP_CONFIG,headers,slug,actor);}
export function nativePartsStock(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,operation:'read'):Promise<import('@erpx/contracts/field-service-reserve').ServiceStockWorkspace|undefined>;
export function nativePartsStock(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,operation:Exclude<NativePartsStockOperation,'read'>,body:unknown,key:string,originalBody:string):Promise<Record<string,unknown>|undefined>;
export async function nativePartsStock(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,operation:NativePartsStockOperation,body?:unknown,key?:string,originalBody?:string){
 const choice=selected(process.env.ERPX_NATIVE_FIELD_SERVICE_STOCK_SETUP_CONFIG,incoming,slug,actor);if(!choice)return undefined;if(!actor)return fail('WORK_CONTEXT_CHANGED',409);
 const headers=new Headers({host:new URL(choice.target.origin).host,accept:'application/json','cache-control':'no-cache'}),cookie=incoming.get('cookie');if(cookie){if(new TextEncoder().encode(cookie).length>8192)return fail('NATIVE_PARTS_SETUP_COOKIE_LIMIT');headers.set('cookie',cookie);}
 for(const [name,value] of [['x-erpx-context-company',actor.companyId],['x-erpx-context-assignment',actor.assignmentId],['x-erpx-context-revision',String(actor.revision)],['x-erpx-context-assignment-version',String(actor.assignmentVersion)]] as const){const captured=incoming.get(name);if(operation!=='read'&&captured!==value)return fail('WORK_CONTEXT_CHANGED',409);headers.set(name,operation==='read'?value:captured!);}
 if(actor.branchSlug)headers.set('x-erpx-workspace-branch',actor.branchSlug);if(actor.departmentSlug)headers.set('x-erpx-workspace-department',actor.departmentSlug);
 if(operation!=='read'){if(!key||originalBody===undefined)return fail('NATIVE_PARTS_SETUP_KEY_REQUIRED',422);const origin=incoming.get('origin');if(origin){if(origin!==choice.target.origin)return fail('ORIGIN_FORBIDDEN',403);headers.set('origin',origin);}headers.set('idempotency-key',key);headers.set('content-type','application/json');}
 let response:Awaited<ReturnType<typeof nativePartsStockRequest>>;try{response=await nativePartsStockRequest({baseUrl:choice.config.privateBaseUrl,timeoutMs:choice.config.timeoutMs,slug,operation,headers,...(operation!=='read'?{body:originalBody}:{})});}catch{return fail('NATIVE_PARTS_SETUP_UNAVAILABLE',503);}
 if(response.status!==200){if(object(response.body)&&object(response.body.error)&&typeof response.body.error.code==='string'&&response.body.error.code.length<=100&&response.status>=400&&response.status<=599)return fail(response.body.error.code,response.status);return fail('NATIVE_PARTS_SETUP_UNAVAILABLE',503);}
 if(!object(response.body))return fail('NATIVE_PARTS_SETUP_RESPONSE_INVALID');validatedContext(response.body.context,actor);const data=response.body.data;
 if(operation==='read'){const typed=response.body as unknown as ServiceStockReadResponse;if(!validServicePartsStockWorkspace(typed.data,actor.companyId))return fail('NATIVE_PARTS_SETUP_RESPONSE_INVALID');return typed.data;}
 if(!object(data)||Object.keys(data).sort().join(',')!=='id,version'||!validServicePartsReceipt(data,{kind:operation,body} as ServicePartsRequest,actor.companyId,key!))return fail('NATIVE_PARTS_SETUP_RESPONSE_INVALID');const typed=response.body as unknown as ServiceStockMasterResponse;return typed.data;
}

/** Service entry must use the native owner for the selected role. */
export async function requiredNativePartsEntry(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,role:'stock'|'finance'):Promise<boolean>{
 const value=role==='stock'?await nativePartsStock(incoming,slug,actor,'read'):await nativePartsPrerequisites(incoming,slug,actor,'read');
 if(!value)return fail('NATIVE_PARTS_SETUP_UNAVAILABLE',503);
 return value.available;
}

async function nativeFieldRecordRead(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,method:'GET'|'POST',body?:string,key?:string,supportId?:string,operations:boolean|{id:string}|{customer:'read'|'create'|'confirm'}=false):Promise<unknown>{
 const customer=typeof operations==='object'&&'customer'in operations?operations.customer:undefined;
 const code=customer?'NATIVE_SERVICE_CUSTOMER':operations?'NATIVE_FIELD_OPERATIONS':supportId===undefined?'NATIVE_FIELD_COLLABORATION':'NATIVE_FIELD_SUPPORT';
 const choice=selected(process.env.ERPX_NATIVE_FIELD_SERVICE_SOURCE_RECORD_CONFIG,incoming,slug,actor);if(!choice||!actor)return fail(`${code}_UNAVAILABLE`,503);
 const headers=new Headers({host:new URL(choice.target.origin).host,accept:'application/json','cache-control':'no-cache'}),cookie=incoming.get('cookie');
 if(cookie){if(new TextEncoder().encode(cookie).length>8192)return fail(`${code}_COOKIE_LIMIT`);headers.set('cookie',cookie);}
 for(const [name,value]of [['x-erpx-context-company',actor.companyId],['x-erpx-context-assignment',actor.assignmentId],['x-erpx-context-revision',String(actor.revision)],['x-erpx-context-assignment-version',String(actor.assignmentVersion)]]as const){if(method==='POST'&&incoming.get(name)!==value)return fail('WORK_CONTEXT_CHANGED',409);headers.set(name,value);}
 if(actor.branchSlug)headers.set('x-erpx-workspace-branch',actor.branchSlug);if(actor.departmentSlug)headers.set('x-erpx-workspace-department',actor.departmentSlug);
 if(method==='POST'){if(!key)return fail('COMMAND_KEY_REQUIRED',422);if(incoming.get('origin')!==choice.target.origin)return fail('ORIGIN_FORBIDDEN',403);headers.set('origin',choice.target.origin);headers.set('idempotency-key',key);headers.set('content-type','application/json');}
 const reply=customer
 ?await nativeServiceCustomerPreparationRequest({baseUrl:choice.config.privateBaseUrl,slug,method,headers,body,timeoutMs:choice.config.timeoutMs,confirmation:customer==='confirm'})
 :operations
 ?await nativeFieldOperationsRequest({baseUrl:choice.config.privateBaseUrl,slug,method,headers,body,timeoutMs:choice.config.timeoutMs,...(typeof operations==='object'&&'id'in operations?{id:operations.id}:{})})
 :supportId!==undefined
 ?await nativeFieldSupportRequest({baseUrl:choice.config.privateBaseUrl,slug,headers,timeoutMs:choice.config.timeoutMs,id:supportId})
 :await nativeFieldCollaborationRequest({baseUrl:choice.config.privateBaseUrl,slug,method,headers,body,timeoutMs:choice.config.timeoutMs});
 if(reply.status!==(customer==='create'?201:200)){if(object(reply.body)&&object(reply.body.error)&&typeof reply.body.error.code==='string')return fail(reply.body.error.code,[401,403,404,409,422].includes(reply.status)?reply.status:503);return fail(`${code}_UNAVAILABLE`,503);}
 if(!object(reply.body))return fail(`${code}_RESPONSE_INVALID`,503);validatedContext(reply.body.context,actor);
 if(customer){const data=reply.body.data;
  if(customer==='read'){if(!validServiceCustomerPreparation(data,actor.companyId))return fail(`${code}_RESPONSE_INVALID`,503);return data;}
  if(!object(data)||typeof data.id!=='string'||!data.id.startsWith('party-')||data.kind!=='customer')return fail(`${code}_RESPONSE_INVALID`,503);
  if(customer==='confirm'&&(data.commandId!==key||data.companyId!==actor.companyId||!['active','blocked'].includes(String(data.status))))return fail(`${code}_RESPONSE_INVALID`,503);
  return data;
 }
 if(operations){const data=reply.body.data;
  if(method==='GET'&&typeof operations==='object'&&'id'in operations){if(!validFieldOperationsDetailRead(data,actor.companyId,operations.id))return fail(`${code}_RESPONSE_INVALID`,503);return data;}
  if(method==='GET'){if(!validFieldOperationsWorkspace(data,actor.companyId)||data.actorId!==actor.userId)return fail(`${code}_RESPONSE_INVALID`,503);return data;}
  if(!body||!key||!object(data))return fail(`${code}_RESPONSE_INVALID`,503);let input:unknown;try{input=JSON.parse(body);}catch{return fail(`${code}_RESPONSE_INVALID`,503);}
  if(object(input)&&input.entity!=='policy'&&['assign','prioritize'].includes(String(input.action))){if(typeof input.id!=='string'||data.id!==input.id||!Number.isSafeInteger(input.expectedVersion)||data.version!==Number(input.expectedVersion)+1||!['open','acknowledged','in_progress','escalated'].includes(String(data.status)))return fail(`${code}_RESPONSE_INVALID`,503);}
  else if(!matchesOperationsReceipt(body,key,data))return fail(`${code}_RESPONSE_INVALID`,503);
  return data;
 }
 if(supportId!==undefined){const data=reply.body.data;if(!object(data)||!object(data.row)||data.row.id!==supportId||data.row.company_id!==actor.companyId||!['field_service','sales_order','general','pos_sale','pms_record','asset'].includes(String(data.row.source_kind))||typeof data.row.title!=='string'||typeof data.row.issue!=='string'||typeof data.row.status!=='string'||!Number.isSafeInteger(data.row.version)||Number(data.row.version)<1||!Array.isArray(data.history)||data.managedInFieldService!==(data.row.source_kind==='field_service')||data.canWork!==false||data.canManage!==false||data.canReport!==false||data.canConfirm!==false)return fail('NATIVE_FIELD_SUPPORT_RESPONSE_INVALID',503);return data;}
 if(method==='GET'){if(!validFieldCollaborationReadModel(reply.body.data))return fail(`${code}_RESPONSE_INVALID`,503);return reply.body.data;}
 const result=reply.body.data;if(!body||!key||!matchesCollaborationReceipt(body,key,result))return fail(`${code}_RESPONSE_INVALID`,503);
 return result;
}

export function nativeFieldCollaboration(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,method:'GET'|'POST',body?:string,key?:string){return nativeFieldRecordRead(incoming,slug,actor,method,body,key);}
/** Field entry; this does not replace the generic Support workspace. */
export function nativeFieldSupportRead(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,id:string){return nativeFieldRecordRead(incoming,slug,actor,'GET',undefined,undefined,id);}

export type NativeFieldSupportRead={row:{id:string;title:string;issue:string;status:string;version:number};history:unknown[];managedInFieldService:boolean;canWork:false;canManage:false;canReport:false;canConfirm:false};
export async function requiredNativeFieldSupportRead(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,id:string):Promise<NativeFieldSupportRead>{
 return await nativeFieldSupportRead(incoming,slug,actor,id) as NativeFieldSupportRead;
}

export function nativeFieldOperations(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,method:'GET'|'POST',body?:string,key?:string){return nativeFieldRecordRead(incoming,slug,actor,method,body,key,undefined,true);}

export function nativeFieldOperationsDetail(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,id:string){return nativeFieldRecordRead(incoming,slug,actor,'GET',undefined,undefined,undefined,{id});}

export function nativeServiceCustomerPreparation(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined,route:'read'|'create'|'confirm',body?:string,key?:string){return nativeFieldRecordRead(incoming,slug,actor,route==='read'?'GET':'POST',body,key,undefined,{customer:route});}
export async function requiredNativeServiceCustomerPreparation(incoming:Pick<Headers,'get'>,slug:string,actor:ReserveContext|undefined):Promise<ServiceCustomerPreparation>{
 const value=await nativeServiceCustomerPreparation(incoming,slug,actor,'read');
 if(!actor||!validServiceCustomerPreparation(value,actor.companyId))return fail('NATIVE_SERVICE_CUSTOMER_RESPONSE_INVALID',503);
 return value;
}
