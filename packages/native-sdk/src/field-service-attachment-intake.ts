import 'server-only';
import {Agent,request} from 'node:http';
import {Readable} from 'node:stream';
import type {FieldIntakeCreateResponse,FieldIntakeListResponse,FieldIntakeErrorResponse} from '@erpx/contracts/field-service-attachment-intake';

/** This private transport forwards the caller's raw body and authorization headers. */
export type NativeFieldIntakeRequest={baseUrl:string;slug:string;route:{kind:'list';query?:string}|{kind:'create'};headers:Headers;timeoutMs:number;body?:string};
export type NativeFieldIntakeRawReply={status:number;body:unknown};
export type NativeFieldIntakeReply={status:number;body:FieldIntakeListResponse|FieldIntakeCreateResponse|FieldIntakeErrorResponse};
export type NativeFieldIntakeTransportInput=NativeFieldIntakeRequest&{method:'GET'|'POST';suffix:string};
export type NativeFieldIntakeTransport=(input:NativeFieldIntakeTransportInput)=>Promise<NativeFieldIntakeRawReply>;
const reserved=new Set([3000,3001,3100,3101,3103,5432,5433,5435,8080]);
const invalid=()=>Error('NATIVE_FIELD_INTAKE_RESPONSE_INVALID');
function object(v:unknown):v is Record<string,unknown>{return v!==null&&typeof v==='object'&&!Array.isArray(v);}
function string(v:unknown):v is string{return typeof v==='string';}
function nullableString(v:unknown):boolean{return v===null||string(v);}
function oneOf(v:unknown,options:readonly string[]):boolean{return string(v)&&options.includes(v);}
const components=['record','action','line','event','message','publication'];const states=['draft','bound','withdrawn','expired'];const sources=['legacy_unscanned','quarantined','scanning','available','scan_error','malware_rejected','destroyed'];const classifications=['public','internal','confidential','restricted'];
function integer(v:unknown):v is number{return typeof v==='number'&&Number.isSafeInteger(v);}
function nullableInteger(v:unknown):boolean{return v===null||integer(v);}
function link(v:unknown):boolean{return object(v)&&string(v.id)&&nullableString(v.sourceId)&&string(v.filename)&&string(v.mediaType)&&nullableInteger(v.bytes)&&(v.sourceState===null||oneOf(v.sourceState,sources))&&oneOf(v.linkState,states)&&nullableString(v.caption)&&typeof v.previewAvailable==='boolean'&&integer(v.createdAt)&&integer(v.expiresAt)&&nullableInteger(v.boundAt);}
function set(v:unknown):boolean{return object(v)&&string(v.id)&&v.targetType==='field_service_visit'&&string(v.targetId)&&oneOf(v.componentKind,components)&&nullableString(v.componentRef)&&string(v.purposeKey)&&integer(v.expectedTargetVersion)&&integer(v.policyVersion)&&oneOf(v.classification,classifications)&&integer(v.requiredCount)&&integer(v.maxFiles)&&oneOf(v.state,states)&&integer(v.createdAt)&&integer(v.updatedAt)&&Array.isArray(v.links)&&v.links.every(link);}
function finite(v:unknown):boolean{if(typeof v==='number')return Number.isFinite(v);if(Array.isArray(v))return v.every(finite);if(object(v))return Object.values(v).every(finite);return true;}
function checked(status:number,body:unknown,kind:'list'|'create'):NativeFieldIntakeReply {
 if(!object(body)||!finite(body))throw invalid();
 if(status===200&&kind==='list'&&Array.isArray(body.data)&&body.data.every(set))return {status,body:body as FieldIntakeListResponse};
 if(status===201&&kind==='create'&&set(body.data))return {status,body:body as FieldIntakeCreateResponse};
 if(status>=400&&status<=599&&object(body.error)&&string(body.error.code)&&string(body.error.message))return {status,body:body as FieldIntakeErrorResponse};
 throw invalid();
}
export async function nativeFieldIntakeRequest(input:NativeFieldIntakeRequest,transport:NativeFieldIntakeTransport=nodeTransport):Promise<NativeFieldIntakeReply>{
 const route=input.route;let suffix:string,method:'GET'|'POST';
 switch(route.kind){case 'list':suffix=route.query??'';method='GET';if(suffix&&(!suffix.startsWith('?')||/[#\u0000-\u001f\u007f]/.test(suffix))||new TextEncoder().encode(suffix).length>2*1024*1024)throw Error('NATIVE_FIELD_INTAKE_REQUEST_INVALID');break;case 'create':suffix='';method='POST';break;default:throw Error('NATIVE_FIELD_INTAKE_REQUEST_INVALID');}
 if(method==='GET'&&input.body!==undefined||method==='POST'&&input.body===undefined)throw Error('NATIVE_FIELD_INTAKE_REQUEST_INVALID');
 const reply=await transport({...input,method,suffix});return checked(reply.status,reply.body,route.kind);
}
async function nodeTransport(input:NativeFieldIntakeTransportInput):Promise<NativeFieldIntakeRawReply>{
 let origin:URL;try{origin=new URL(input.baseUrl);}catch{throw Error('NATIVE_FIELD_INTAKE_CONFIGURATION_INVALID');}
 const port=Number(origin.port);
 if(origin.protocol!=='http:'||!['127.0.0.1','[::1]'].includes(origin.hostname)||!origin.port||reserved.has(port)||port>=3200&&port<=3299||origin.username||origin.password||origin.origin!==input.baseUrl||!Number.isSafeInteger(input.timeoutMs)||input.timeoutMs<250||input.timeoutMs>30000||!/^[a-z0-9_-]{1,128}$/.test(input.slug))throw Error('NATIVE_FIELD_INTAKE_CONFIGURATION_INVALID');
 if(input.method==='GET'&&(input.body!==undefined)||input.body!==undefined&&new TextEncoder().encode(input.body).length>2*1024*1024)throw Error('NATIVE_FIELD_INTAKE_REQUEST_INVALID');
 const url=new URL(`/api/v1/holdings/${encodeURIComponent(input.slug)}/field-service/attachment-sets${input.suffix}`,origin),controller=new AbortController(),timer=setTimeout(()=>controller.abort(),input.timeoutMs);
 try{
  const response=await new Promise<Response>((resolve,reject)=>{const agent=new Agent({keepAlive:false,maxSockets:1,maxTotalSockets:1,proxyEnv:{} as import('node:http').ProxyEnv}),req=request(url,{method:input.method,headers:Object.fromEntries(input.headers),agent,signal:controller.signal,maxHeaderSize:8192},incoming=>{incoming.once('close',()=>agent.destroy());try{const headers=new Headers();for(let i=0;i<incoming.rawHeaders.length;i+=2)headers.append(incoming.rawHeaders[i]!,incoming.rawHeaders[i+1]!);resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>,{status:incoming.statusCode??502,headers}));}catch(error){incoming.destroy();agent.destroy();reject(error);}});req.once('error',error=>{agent.destroy();reject(error);});req.end(input.body);});
  if(response.headers.get('content-type')?.split(';')[0]?.trim()!=='application/json'||!response.body)throw Error('NATIVE_FIELD_INTAKE_RESPONSE_INVALID');
  const reader=response.body.getReader(),chunks:Uint8Array[]=[];let size=0;const cancel=()=>{void reader.cancel().catch(()=>undefined);};controller.signal.addEventListener('abort',cancel,{once:true});try{for(;;){const part=await reader.read();if(controller.signal.aborted)throw Error('NATIVE_FIELD_INTAKE_TIMEOUT');if(part.done)break;size+=part.value.length;if(size>2*1024*1024){await reader.cancel();throw Error('NATIVE_FIELD_INTAKE_RESPONSE_LIMIT');}chunks.push(part.value);}}finally{controller.signal.removeEventListener('abort',cancel);reader.releaseLock();}
  const bytes=new Uint8Array(size);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.length;}
  let body:unknown;try{body=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));}catch{throw Error('NATIVE_FIELD_INTAKE_RESPONSE_INVALID');}
  return {status:response.status,body};
 }finally{clearTimeout(timer);}
}
