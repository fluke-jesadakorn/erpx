import 'server-only';
import {Agent,request} from 'node:http';
import {Readable} from 'node:stream';

/** Private transport only; selection and authorization remain the caller's responsibility. */
export type NativePartsSetupRequest={baseUrl:string;slug:string;method:'GET'|'POST';existing?:boolean;headers:Headers;body?:string;timeoutMs:number};
export type NativePartsSetupReply={status:number;body:unknown};
const reserved=new Set([3000,3001,3100,3101,3103,5432,5433,5435,8080]);
export type NativePartsPrerequisiteOperation='read'|'chart'|'account'|'period'|'rate';
export type NativePartsPrerequisiteRequest=Omit<NativePartsSetupRequest,'method'|'existing'>&{operation:NativePartsPrerequisiteOperation};
const prerequisitePaths=new Map<NativePartsPrerequisiteOperation,string>([['read','/prerequisites'],['chart','/charts'],['account','/accounts'],['period','/periods'],['rate','/rates']]);
export async function nativePartsPrerequisiteRequest(input:NativePartsPrerequisiteRequest):Promise<NativePartsSetupReply>{
 const suffix=prerequisitePaths.get(input.operation);
 if(suffix===undefined)throw Error('NATIVE_PARTS_SETUP_REQUEST_INVALID');
 return nativePartsTransport({...input,method:input.operation==='read'?'GET':'POST'},suffix);
}
export type NativePartsStockOperation='read'|'products'|'warehouses';
export type NativePartsStockRequest=Omit<NativePartsSetupRequest,'method'|'existing'>&{operation:NativePartsStockOperation};
const stockPaths=new Map<NativePartsStockOperation,string>([['read','/stock'],['products','/masters/products'],['warehouses','/masters/warehouses']]);
export async function nativePartsStockRequest(input:NativePartsStockRequest):Promise<NativePartsSetupReply>{
 const suffix=stockPaths.get(input.operation);if(suffix===undefined)throw Error('NATIVE_PARTS_SETUP_REQUEST_INVALID');
 return nativePartsTransport({...input,method:input.operation==='read'?'GET':'POST'},suffix);
}
export async function nativePartsSetupRequest(input:NativePartsSetupRequest):Promise<NativePartsSetupReply>{
 return nativePartsTransport(input,input.existing?'/existing':'');
}
async function nativePartsTransport(input:NativePartsSetupRequest,suffix:string,resource:"parts-setup"|"collaboration"|"support"|"operations"|"customer-preparation"|"work-roles"|"inbox-candidates"|"inbox"|"executive"|"executive-evidence"|"commercial"|"invoice-payment-evidence"="parts-setup"):Promise<NativePartsSetupReply>{
 let origin:URL;try{origin=new URL(input.baseUrl);}catch{throw Error('NATIVE_PARTS_SETUP_CONFIGURATION_INVALID');}
 const port=Number(origin.port);
 if(origin.protocol!=='http:'||!['127.0.0.1','[::1]'].includes(origin.hostname)||!origin.port||reserved.has(port)||port>=3200&&port<=3299||origin.username||origin.password||origin.origin!==input.baseUrl||!Number.isSafeInteger(input.timeoutMs)||input.timeoutMs<250||input.timeoutMs>30000||!/^[a-z0-9_-]{1,128}$/.test(input.slug))throw Error('NATIVE_PARTS_SETUP_CONFIGURATION_INVALID');
 if(input.method==='GET'&&(input.existing||input.body!==undefined)||input.body!==undefined&&(resource==='commercial'?input.body.length>200000:new TextEncoder().encode(input.body).length>65536))throw Error('NATIVE_PARTS_SETUP_REQUEST_INVALID');
 const url=new URL(resource==='commercial'?`/api/v1/platform/commercial${suffix}`:resource==='invoice-payment-evidence'?`/api/v1/holdings/${encodeURIComponent(input.slug)}/finance/invoices${suffix}`:`/api/v1/holdings/${encodeURIComponent(input.slug)}/field-service/${resource}${suffix}`,origin),controller=new AbortController(),timer=setTimeout(()=>controller.abort(),input.timeoutMs);
 try{
  const response=await new Promise<Response>((resolve,reject)=>{const agent=new Agent({keepAlive:false,maxSockets:1,maxTotalSockets:1,proxyEnv:{} as import('node:http').ProxyEnv}),req=request(url,{method:input.method,headers:Object.fromEntries(input.headers),agent,signal:controller.signal,maxHeaderSize:8192},incoming=>{incoming.once('close',()=>agent.destroy());try{const headers=new Headers();for(let i=0;i<incoming.rawHeaders.length;i+=2)headers.append(incoming.rawHeaders[i]!,incoming.rawHeaders[i+1]!);resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>,{status:incoming.statusCode??502,headers}));}catch(error){incoming.destroy();agent.destroy();reject(error);}});req.once('error',error=>{agent.destroy();reject(error);});req.end(input.body);});
  if(response.headers.get('content-type')?.split(';')[0]?.trim()!=='application/json'||!response.body)throw Error('NATIVE_PARTS_SETUP_RESPONSE_INVALID');
  const reader=response.body.getReader(),chunks:Uint8Array[]=[];let size=0;const cancel=()=>{void reader.cancel().catch(()=>undefined);};controller.signal.addEventListener('abort',cancel,{once:true});try{for(;;){const part=await reader.read();if(controller.signal.aborted)throw Error('NATIVE_PARTS_SETUP_TIMEOUT');if(part.done)break;size+=part.value.length;if(size>2*1024*1024){await reader.cancel();throw Error('NATIVE_PARTS_SETUP_RESPONSE_LIMIT');}chunks.push(part.value);}}finally{controller.signal.removeEventListener('abort',cancel);reader.releaseLock();}
  const bytes=new Uint8Array(size);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.length;}
  let body:unknown;try{body=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));}catch{throw Error('NATIVE_PARTS_SETUP_RESPONSE_INVALID');}
  return {status:response.status,body};
 }finally{clearTimeout(timer);}
}

/** Field Collaboration shares the private transport and its bounded lifecycle. */
export async function nativeFieldCollaborationRequest(input:NativePartsSetupRequest):Promise<NativePartsSetupReply>{return nativePartsTransport(input,"","collaboration");}

/** Field-managed Support read only. No generic Support writer is exposed. */
export async function nativeFieldSupportRequest(input:Omit<NativePartsSetupRequest,'method'|'existing'|'body'>&{id:string}):Promise<NativePartsSetupReply>{
 if(!input.id||input.id==='.'||input.id==='..'||input.id.length>256||/[\u0000-\u001f\u007f]/.test(input.id))throw Error('NATIVE_FIELD_SUPPORT_REQUEST_INVALID');
 return nativePartsTransport({...input,method:'GET'},`/${encodeURIComponent(input.id)}`,"support");
}

export async function nativeFieldOperationsRequest(input:NativePartsSetupRequest&{id?:string}):Promise<NativePartsSetupReply>{
 if(input.id!==undefined&&(input.method!=='GET'||!input.id||input.id==='.'||input.id==='..'||input.id.length>180||/[\u0000-\u001f\u007f]/.test(input.id)))throw Error('NATIVE_FIELD_OPERATIONS_REQUEST_INVALID');
 return nativePartsTransport(input,input.id===undefined?'':`/${encodeURIComponent(input.id)}`,"operations");
}

/** Customer create/confirmation share one immutable body and command key. */
export async function nativeServiceCustomerPreparationRequest(input:Omit<NativePartsSetupRequest,'existing'>&{confirmation?:boolean}):Promise<NativePartsSetupReply>{
 if(input.confirmation&&input.method!=='POST')throw Error('NATIVE_SERVICE_CUSTOMER_REQUEST_INVALID');
 return nativePartsTransport(input,input.confirmation?'/confirmations':'','customer-preparation');
}

/** Account-only role bootstrap; business authorization is never inferred from this read. */
export async function nativeWorkRolesRequest(input:NativePartsSetupRequest&{mode?:'current'|'initialize'|'access'|'context'}):Promise<NativePartsSetupReply>{
 if(input.mode!==undefined&&(input.method!=='GET'||!['current','initialize','access','context'].includes(input.mode)))throw Error('NATIVE_WORK_ROLE_REQUEST_INVALID');
 return nativePartsTransport(input,input.mode?`/${input.mode}`:'','work-roles');
}

/** Company discovery authenticates the Holding account before a Company is selected. */
export async function nativeAccountCompaniesRequest(input:Omit<NativePartsSetupRequest,'method'|'existing'|'body'>):Promise<NativePartsSetupReply>{return nativePartsTransport({...input,method:'GET'},'/companies','work-roles');}

export async function nativeAccountPreferencesRequest(input:NativePartsSetupRequest&{workspace?:boolean}):Promise<NativePartsSetupReply>{if(input.workspace&&input.method!=='POST')throw Error('NATIVE_WORKSPACE_PREFERENCE_REQUEST_INVALID');return nativePartsTransport(input,input.workspace?'/workspace-preference':'/preferences','work-roles');}

export type NativeFieldInboxQuery={projection?:'inbox'|'staff-list'|'staff-page';reverse?:boolean;all:boolean;category?:string;search:string;before?:{at:number;id:string;key?:string};limit:number};
export async function nativeFieldInboxCandidatesRequest(input:Omit<NativePartsSetupRequest,'method'|'body'|'existing'>&{query:NativeFieldInboxQuery}):Promise<NativePartsSetupReply>{
 const q=input.query;if(!Number.isSafeInteger(q.limit)||q.limit<1||q.limit>101||q.search.length>200||q.before&&(!Number.isSafeInteger(q.before.at)||!q.before.id||new TextEncoder().encode(q.before.id).length>512))throw Error('NATIVE_FIELD_INBOX_QUERY_INVALID');
 const params=new URLSearchParams({all:String(q.all),search:q.search,limit:String(q.limit)});if(q.projection)params.set('projection',q.projection);if(q.reverse)params.set('reverse','true');if(q.before?.key!==undefined)params.set('beforeKey',q.before.key);if(q.category)params.set('category',q.category);if(q.before){params.set('beforeAt',String(q.before.at));params.set('beforeId',q.before.id);}
 return nativePartsTransport({...input,method:'GET'},'?'+params.toString(),'inbox-candidates');
}

/** Bounded delivery-state command; the native owner resolves source ownership. */
export async function nativeFieldInboxStateRequest(input:Omit<NativePartsSetupRequest,'method'|'existing'>&{id:string}):Promise<NativePartsSetupReply>{
 if(!input.id||input.id==='.'||input.id==='..'||new TextEncoder().encode(input.id).length>512||/[\u0000-\u001f\u007f]/.test(input.id)||input.body===undefined||new TextEncoder().encode(input.body).length>8192)throw Error('NATIVE_FIELD_INBOX_STATE_INVALID');
 return nativePartsTransport({...input,method:'POST'},`/${encodeURIComponent(input.id)}/state`,'inbox');
}

export type NativeFieldExecutiveQuery={accessOnly?:boolean;metric:'open-service-orders'|'service-billing-releases'|'field-visit-completion';breakdown:string;fromAt:number;toAt:number;query?:string;beforeAt?:number;beforeId?:string;evidenceLimit?:number};
export async function nativeFieldExecutiveRequest(input:Omit<NativePartsSetupRequest,'method'|'existing'|'body'>&{query:NativeFieldExecutiveQuery}):Promise<NativePartsSetupReply>{
 const q=input.query,params=new URLSearchParams({metric:q.metric,breakdown:q.breakdown,fromAt:String(q.fromAt),toAt:String(q.toAt)});
 if(q.accessOnly!==undefined)params.set('accessOnly',String(q.accessOnly));if(q.query!==undefined)params.set('query',q.query);if(q.beforeAt!==undefined)params.set('beforeAt',String(q.beforeAt));if(q.beforeId!==undefined)params.set('beforeId',q.beforeId);if(q.evidenceLimit!==undefined)params.set('evidenceLimit',String(q.evidenceLimit));
 return nativePartsTransport({...input,method:'GET'},'?'+params.toString(),'executive');
}

/** Finite native Service Commercial entrypoints share the bounded private
 * transport. Raw reviewed payload/key are never reconstructed or retried here. */
export type NativeServicePackageRequest=Omit<NativePartsSetupRequest,'method'|'existing'>&{operation:'companies'|'overview'|'preview'|'apply';holdingId?:string;companyId?:string};
export async function nativeServicePackageRequest(input:NativeServicePackageRequest):Promise<NativePartsSetupReply>{
 let suffix:string,method:'GET'|'POST';
 switch(input.operation){
  case 'companies':suffix='';method='GET';break;
  case 'overview':
   if(typeof input.holdingId!=='string'||!input.holdingId||input.holdingId.length>100||typeof input.companyId!=='string'||!input.companyId||input.companyId.length>100)throw Error('NATIVE_PARTS_SETUP_REQUEST_INVALID');
   suffix='/packages?'+new URLSearchParams({holdingId:input.holdingId,companyId:input.companyId});method='GET';break;
  case 'preview':suffix='/packages/preview';method='POST';break;
  case 'apply':suffix='/packages/apply';method='POST';break;
  default:throw Error('NATIVE_PARTS_SETUP_REQUEST_INVALID');
 }
 return nativePartsTransport({...input,method},suffix,'commercial');
}

/** Only a native negative admission read; it never authorizes legacy writes. */
export async function nativeServiceInvoicePaymentEvidenceRequest(input:Omit<NativePartsSetupRequest,'method'|'existing'|'body'>&{id:string}):Promise<NativePartsSetupReply>{
 if(typeof input.id!=='string'||!input.id||input.id.length>180)throw Error('NATIVE_PARTS_SETUP_REQUEST_INVALID');
 return nativePartsTransport({...input,method:'GET'},`/${encodeURIComponent(input.id)}/workspace?purpose=payment-evidence`,'invoice-payment-evidence');
}

/** Search/open include a native, same-transaction evidence access audit. */
export async function nativeFieldEvidenceRequest(input:Omit<NativePartsSetupRequest,'method'|'existing'>&{action:'search'|'open'}):Promise<NativePartsSetupReply>{
 if(!['search','open'].includes(input.action)||input.body===undefined)throw Error('NATIVE_FIELD_EVIDENCE_REQUEST_INVALID');
 return nativePartsTransport({...input,method:'POST'},`/${input.action}`,'executive-evidence');
}
