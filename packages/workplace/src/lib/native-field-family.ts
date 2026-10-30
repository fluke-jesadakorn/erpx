import 'server-only';
import {Agent,request as httpRequest} from 'node:http';
import {createHash} from 'node:crypto';
import type {FieldIntakeSet,FieldIntakeLink,FieldFileStatusData,FieldFileReservationData,FieldFileUploadData} from '@erpx/contracts/field-service-attachment-intake';
import {contributorExactOrigin} from '@erpx/contracts/holding-hosts';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {ApiError} from '@erpx/server-runtime/public/http';
import {fieldFilesRoute,type FieldFilesRoute} from '@erpx/native-sdk/field-service-files-routes';

type Target={origin:string;holdingSlug:string;holdingId:string;fieldFamily?:boolean};
type Config={privateBaseUrl:string;timeoutMs:number;holdings:Target[]};
type Input={route:FieldFilesRoute;body?:Uint8Array;contentType?:string;key?:string};
type Reply={status:number;headers:Headers;body:Uint8Array};
type Transport=(url:URL,method:string,headers:Headers,body:Uint8Array|undefined,timeout:number,limit:number)=>Promise<Reply>;
const fail=(status:number,code:string):never=>{throw new ApiError(status,code,'Field visit files are unavailable.');};
const object=(x:unknown):x is Record<string,unknown>=>!!x&&typeof x==='object'&&!Array.isArray(x);
const text=(x:unknown):x is string=>typeof x==='string'&&x.length>0;
const exact=(v:Record<string,unknown>,keys:readonly string[])=>Object.keys(v).length===keys.length&&keys.every(k=>Object.hasOwn(v,k));
const integer=(v:unknown,min=0):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=min;
const nullableString=(v:unknown)=>v===null||typeof v==='string';
const nullableInt=(v:unknown)=>v===null||integer(v);
const oneOf=(v:unknown,values:readonly string[])=>typeof v==='string'&&values.includes(v);
const sources=['legacy_unscanned','quarantined','scanning','available','scan_error','malware_rejected','destroyed'] as const;
const states=['draft','bound','withdrawn','expired'] as const;
const purposes=['site_photo','technician_report','signed_job_sheet','exception_evidence'] as const;
const components=['record','action','line','event','message','publication'] as const;
function validLink(v:unknown):v is FieldIntakeLink{return object(v)&&exact(v,['id','sourceId','filename','mediaType','bytes','sourceState','linkState','caption','previewAvailable','createdAt','expiresAt','boundAt'])&&text(v.id)&&nullableString(v.sourceId)&&text(v.filename)&&text(v.mediaType)&&(v.bytes===null||integer(v.bytes,1))&&(v.sourceState===null||oneOf(v.sourceState,sources))&&oneOf(v.linkState,states)&&nullableString(v.caption)&&typeof v.previewAvailable==='boolean'&&integer(v.createdAt)&&integer(v.expiresAt,1)&&nullableInt(v.boundAt);}
function validSet(v:unknown):v is FieldIntakeSet{return object(v)&&exact(v,['id','targetType','targetId','componentKind','componentRef','purposeKey','expectedTargetVersion','policyVersion','classification','requiredCount','maxFiles','state','createdAt','updatedAt','links'])&&text(v.id)&&v.targetType==='field_service_visit'&&text(v.targetId)&&oneOf(v.componentKind,components)&&nullableString(v.componentRef)&&oneOf(v.purposeKey,purposes)&&integer(v.expectedTargetVersion,1)&&integer(v.policyVersion,1)&&oneOf(v.classification,['public','internal','confidential','restricted'])&&integer(v.requiredCount)&&integer(v.maxFiles,1)&&v.maxFiles<=100&&v.requiredCount<=v.maxFiles&&oneOf(v.state,states)&&integer(v.createdAt)&&integer(v.updatedAt)&&Array.isArray(v.links)&&v.links.length<=100&&v.links.every(validLink);}
function validStatus(v:unknown,id:string):v is FieldFileStatusData {if(!object(v)||!exact(v,['linkId','setId','sourceId','sourceState','linkState','scan','previewAvailable','bindable','expiresAt','boundAt','withdrawnAt'])||v.linkId!==id||!text(v.setId)||!nullableString(v.sourceId)||!(v.sourceState===null||oneOf(v.sourceState,sources))||!oneOf(v.linkState,states)||!object(v.scan)||!exact(v.scan,['attempts','lastResult','lastError','signature','definitionUpdatedAt','scannedAt']))return false;const scan=v.scan;return integer(scan.attempts)&&nullableString(scan.lastResult)&&nullableString(scan.lastError)&&nullableString(scan.signature)&&nullableInt(scan.definitionUpdatedAt)&&nullableInt(scan.scannedAt)&&typeof v.previewAvailable==='boolean'&&typeof v.bindable==='boolean'&&integer(v.expiresAt,1)&&nullableInt(v.boundAt)&&nullableInt(v.withdrawnAt);}
function validReservation(v:unknown):v is FieldFileReservationData{return object(v)&&exact(v,['uploadIntentId','attachmentLinkId','uploadUrl','expiresAt'])&&text(v.uploadIntentId)&&text(v.attachmentLinkId)&&text(v.uploadUrl)&&integer(v.expiresAt,1);}
function validUpload(v:unknown,id:string):v is FieldFileUploadData{return object(v)&&exact(v,['uploadIntentId','attachmentLinkId','sourceId','state','status'])&&v.uploadIntentId===id&&text(v.attachmentLinkId)&&text(v.sourceId)&&oneOf(v.state,sources)&&v.status==='scan_pending';}
function safeDisposition(value:string,expected:'inline'|'attachment'):boolean {
 const prefix=`${expected}; filename*=UTF-8''`;if(!value.startsWith(prefix))return false;
 const encoded=value.slice(prefix.length);if(!encoded||encoded.length>2303||!/^(?:[A-Za-z0-9!#$&+.^_`|~-]|%[0-9A-Fa-f]{2})+$/.test(encoded))return false;
 try{const decoded=decodeURIComponent(encoded);return decoded.length>0&&decoded.length<=(expected==='inline'?263:255)&&!/[\x00-\x1f\x7f]/.test(decoded);}catch{return false;}
}
const reserved=new Set([3000,3001,3100,3101,3103,5432,5433,5435,8080]);
function config(raw:string):Config {
 if(Buffer.byteLength(raw)>2*1024*1024)return fail(503,'NATIVE_ATTACHMENT_CONFIGURATION_INVALID');let x:unknown;try{x=JSON.parse(raw);}catch{return fail(503,'NATIVE_ATTACHMENT_CONFIGURATION_INVALID');}
 if(!object(x)||!text(x.privateBaseUrl)||!Number.isSafeInteger(x.timeoutMs)||Number(x.timeoutMs)<250||Number(x.timeoutMs)>30000||!Array.isArray(x.holdings)||!x.holdings.length||x.holdings.length>64)return fail(503,'NATIVE_ATTACHMENT_CONFIGURATION_INVALID');
 let base:URL;try{base=new URL(x.privateBaseUrl);}catch{return fail(503,'NATIVE_ATTACHMENT_CONFIGURATION_INVALID');}const port=Number(base.port);
 if(base.protocol!=='http:'||!['127.0.0.1','[::1]'].includes(base.hostname)||!port||reserved.has(port)||port>=3200&&port<=3299||base.origin!==x.privateBaseUrl||base.username||base.password)return fail(503,'NATIVE_ATTACHMENT_CONFIGURATION_INVALID');
 const slugs=new Set<string>(),hosts=new Set<string>();for(const h of x.holdings){if(!object(h)||!text(h.origin)||!text(h.holdingId)||!text(h.holdingSlug)||!/^[a-z0-9_-]{1,128}$/.test(h.holdingSlug)||h.fieldFamily!==undefined&&typeof h.fieldFamily!=='boolean')return fail(503,'NATIVE_ATTACHMENT_CONFIGURATION_INVALID');let origin:URL;try{origin=new URL(h.origin);}catch{return fail(503,'NATIVE_ATTACHMENT_CONFIGURATION_INVALID');}if(origin.protocol!=='https:'||origin.origin!==h.origin||origin.username||origin.password||(!(process.env.NODE_ENV==='development'&&process.env.NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE==='container-localhost'&&contributorExactOrigin(origin.host,'contributor')===h.origin)&&(!!origin.port||!origin.hostname.includes('.')||!origin.hostname.split('.').every(p=>/^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(p))))||slugs.has(h.holdingSlug)||hosts.has(origin.host))return fail(503,'NATIVE_ATTACHMENT_CONFIGURATION_INVALID');slugs.add(h.holdingSlug);hosts.add(origin.host);}
 return x as Config;
}
const direct:Transport=(url,method,headers,body,timeout,limit)=>new Promise((resolve,reject)=>{const controller=new AbortController(),timer=setTimeout(()=>controller.abort(),timeout),agent=new Agent({keepAlive:false,maxSockets:1,proxyEnv:{} as import('node:http').ProxyEnv});const req=httpRequest(url,{method,headers:Object.fromEntries(headers),agent,signal:controller.signal,maxHeaderSize:8192},res=>{const chunks:Buffer[]=[];let size=0;res.on('data',(chunk:Buffer)=>{size+=chunk.length;if(size>limit){res.destroy(Error('response limit'));return;}chunks.push(chunk);});res.on('error',reject);res.on('end',()=>{const h=new Headers();for(let i=0;i<res.rawHeaders.length;i+=2)h.append(res.rawHeaders[i]!,res.rawHeaders[i+1]!);resolve({status:res.statusCode??502,headers:h,body:Buffer.concat(chunks)});});});req.on('error',reject);req.on('close',()=>{clearTimeout(timer);agent.destroy();});req.end(body);});
export function createFieldFamilyBridge(raw:string,transport:Transport=direct){const settings=config(raw);return async(request:Request,actor:WorkContext,input:Input):Promise<Response>=>{
 const h=settings.holdings.find(x=>x.holdingSlug===actor.holdingSlug);if(!h?.fieldFamily)return fail(503,'NATIVE_FIELD_FILES_NOT_CONFIGURED');
 if(request.headers.get('host')!==new URL(h.origin).host||actor.holdingId!==h.holdingId||!actor.authority||!actor.iam)return fail(403,'NATIVE_ATTACHMENT_AUTHORITY_INVALID');
 const captured={'x-erpx-context-company':actor.companyId,'x-erpx-context-assignment':actor.assignmentId,'x-erpx-context-revision':String(actor.authority.revision),'x-erpx-context-assignment-version':String(actor.authority.assignmentVersion)};
 if(Object.entries(captured).some(([name,value])=>request.headers.get(name)!==value))return fail(409,'WORK_CONTEXT_CHANGED');
 const headers=new Headers({host:new URL(h.origin).host,accept:input.route.kind==='content'?'*/*':'application/json',...captured});
 for(const name of ['cookie','origin']){const value=request.headers.get(name);if(value){if(Buffer.byteLength(value)>8192)return fail(503,'NATIVE_ATTACHMENT_HEADER_LIMIT');headers.set(name,value);}}
 if(actor.scope.kind!=='holding'&&actor.scope.branchSlug)headers.set('x-erpx-workspace-branch',actor.scope.branchSlug);if(actor.scope.kind==='department')headers.set('x-erpx-workspace-department',actor.scope.departmentSlug);
 if(input.key)headers.set('idempotency-key',input.key);if(input.contentType)headers.set('content-type',input.contentType);
 const route=fieldFilesRoute(actor.holdingSlug,input.route),url=new URL(route.path,settings.privateBaseUrl),limit=input.route.kind==='content'?10*1024*1024:2*1024*1024;
 if(input.body&&input.body.byteLength>(input.route.kind==='upload'?10*1024*1024:2*1024*1024))return fail(413,'NATIVE_ATTACHMENT_BODY_LIMIT');
 let result:Reply;try{result=await transport(url,route.method,headers,input.body,settings.timeoutMs,limit);}catch{return fail(503,'NATIVE_ATTACHMENT_UNAVAILABLE');}
 if(result.status>=300&&result.status<400)return fail(503,'NATIVE_ATTACHMENT_RESPONSE_INVALID');
 if(result.status>=400){let v:unknown;try{v=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(result.body));}catch{return fail(503,'NATIVE_ATTACHMENT_RESPONSE_INVALID');}if(!object(v)||!object(v.error)||!text(v.error.code)||!text(v.error.message))return fail(503,'NATIVE_ATTACHMENT_RESPONSE_INVALID');return Response.json(v,{status:result.status,headers:{'Cache-Control':'private, no-store'}});}
 if(input.route.kind==='content'){
  const type=result.headers.get('content-type'),disposition=result.headers.get('content-disposition'),checksum=result.headers.get('x-erpx-checksum-sha256');
  const allowed=input.route.variant==='preview'?['image/jpeg','image/webp']:['image/jpeg','image/png','image/webp','image/heic','image/heif','application/pdf'];
  const expected=input.route.variant==='preview'?'inline':'attachment';
  if(result.status!==200||!type||!allowed.includes(type)||!disposition||!safeDisposition(disposition,expected)||!checksum||!/^[a-f0-9]{64}$/.test(checksum)||result.body.length===0||result.body.length>limit||createHash('sha256').update(result.body).digest('hex')!==checksum)return fail(503,'NATIVE_ATTACHMENT_RESPONSE_INVALID');
  const out=new Headers({'Content-Type':type,'Content-Disposition':disposition,'Cache-Control':'private, no-store','X-Content-Type-Options':'nosniff','Accept-Ranges':'none'});if(input.route.variant==='preview')out.set('Content-Security-Policy',"sandbox; default-src 'none'");return new Response(result.body as BodyInit,{headers:out});
 }
 if(result.headers.get('content-type')?.split(';')[0]?.trim()!=='application/json')return fail(503,'NATIVE_ATTACHMENT_RESPONSE_INVALID');let v:unknown;try{v=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(result.body));}catch{return fail(503,'NATIVE_ATTACHMENT_RESPONSE_INVALID');}if(!object(v)||!exact(v,['data']))return fail(503,'NATIVE_ATTACHMENT_RESPONSE_INVALID');
 if(input.route.kind==='list'||input.route.kind==='create'){
  let expected:Record<string,unknown>;try{const query=input.route.kind==='list'?new URLSearchParams(input.route.query):null;const parsed=query?Object.fromEntries(['targetId','purposeKey','componentKind','expectedTargetVersion'].map(key=>[key,query.get(key)])):JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(input.body));if(!object(parsed))throw Error();expected=parsed;}catch{return fail(503,'NATIVE_ATTACHMENT_RESPONSE_INVALID');}
  const version=expected.expectedTargetVersion?Number(expected.expectedTargetVersion):0;const componentRef=typeof expected.componentRef==='string'?expected.componentRef.trim()||null:null;const matches=(d:FieldIntakeSet)=>d.targetId===expected.targetId&&(!expected.purposeKey||d.purposeKey===expected.purposeKey)&&(!expected.componentKind||d.componentKind===expected.componentKind)&&(!version||Number.isNaN(version)||d.expectedTargetVersion===version)&&(input.route.kind!=='create'||d.componentRef===componentRef);
  if(input.route.kind==='list'?(result.status!==200||!Array.isArray(v.data)||v.data.length>100||!v.data.every(d=>validSet(d)&&matches(d))):(result.status!==201||!validSet(v.data)||!matches(v.data)))return fail(503,'NATIVE_ATTACHMENT_RESPONSE_INVALID');
 }
 if(input.route.kind==='status'&&(result.status!==200||!validStatus(v.data,input.route.linkId)))return fail(503,'NATIVE_ATTACHMENT_RESPONSE_INVALID');
 if(input.route.kind==='reserve'){const d=v.data;if(result.status!==201||!validReservation(d))return fail(503,'NATIVE_ATTACHMENT_RESPONSE_INVALID');const expected=`/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/field-service/uploads/${encodeURIComponent(d.uploadIntentId)}/content`;if(d.uploadUrl!==expected)return fail(503,'NATIVE_ATTACHMENT_RESPONSE_INVALID');d.uploadUrl=`/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/uploads/${encodeURIComponent(d.uploadIntentId)}/content`;}
 if(input.route.kind==='upload'&&(result.status!==202||!validUpload(v.data,input.route.intentId)))return fail(503,'NATIVE_ATTACHMENT_RESPONSE_INVALID');
 return Response.json(v,{status:result.status,headers:{'Cache-Control':'private, no-store'}});
 };}
export function nativeFieldFamily(request:Request,actor:WorkContext,input:Input){const raw=process.env.ERPX_NATIVE_FIELD_ATTACHMENT_CONFIG;if(!raw||process.env.NODE_ENV==='production')return fail(503,'NATIVE_FIELD_FILES_NOT_CONFIGURED');return createFieldFamilyBridge(raw)(request,actor,input);}

/** Existing bind/withdraw contracts retain their context-bearing native replies. */
export async function nativeFieldFamilyCommand(request:Request,actor:WorkContext,kind:'bind'|'withdraw',id:string,command:{key:string;body:string;expectedVersion?:number}){
 const raw=process.env.ERPX_NATIVE_FIELD_ATTACHMENT_CONFIG;if(!raw||process.env.NODE_ENV==='production')return fail(503,'NATIVE_FIELD_FILES_NOT_CONFIGURED');
 const value=config(raw),target=value.holdings.find(h=>h.holdingSlug===actor.holdingSlug);
 if(!target?.fieldFamily)return fail(503,'NATIVE_FIELD_FILES_NOT_CONFIGURED');
 if(request.headers.get('host')!==new URL(target.origin).host||actor.holdingId!==target.holdingId||!actor.authority||!actor.iam)return fail(403,'NATIVE_ATTACHMENT_AUTHORITY_INVALID');
 const expected={'x-erpx-context-company':actor.companyId,'x-erpx-context-assignment':actor.assignmentId,'x-erpx-context-revision':String(actor.authority.revision),'x-erpx-context-assignment-version':String(actor.authority.assignmentVersion)};
 if(Object.entries(expected).some(([key,v])=>request.headers.get(key)!==v))return fail(409,'WORK_CONTEXT_CHANGED');
 if(kind==='bind'){
  if(!Number.isSafeInteger(command.expectedVersion)||Number(command.expectedVersion)<1)return fail(422,'VALIDATION_FAILED');
  const {createNativeFieldBindBridge}=await import('@erpx/server-files/public/attachments/native-field-bind');
  const configured=JSON.stringify({...value,holdings:value.holdings.map(h=>({...h,setIds:h===target?[id]:[]}))});
  const result=await createNativeFieldBindBridge(configured)(request.headers,actor,id,{key:command.key,body:command.body,expectedVersion:command.expectedVersion!});
  if(!result)return fail(503,'NATIVE_ATTACHMENT_UNAVAILABLE');return Response.json({data:result},{headers:{'Cache-Control':'private, no-store'}});
 }
 const {createNativeFieldAttachmentBridge}=await import('@erpx/server-files/public/attachments/native-field-attachment');
 const configured=JSON.stringify({...value,holdings:value.holdings.map(h=>({...h,linkIds:h===target?[id]:[]}))});
 const result=await createNativeFieldAttachmentBridge(configured)(request.headers,actor,id,{key:command.key,body:command.body});
 if(!result)return fail(503,'NATIVE_ATTACHMENT_UNAVAILABLE');return Response.json({data:result},{headers:{'Cache-Control':'private, no-store'}});
}
