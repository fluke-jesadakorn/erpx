import 'server-only';
import {headers} from 'next/headers';
import type {FinanceFieldJournalFlowResponse,FinanceFieldJournalFlowCommandResponse} from '@erpx/contracts/finance-journal-read';
import type {FinancePurchaseControls} from '@erpx/contracts/finance-purchase-controls';
import type {FlowView} from '@erpx/feature-shared/lib/flow';
import {nativeFinancePurchaseControls} from './native-finance-presentation';
import {nativeFinanceConfiguration,type NativeJournalPageContext} from './native-journal-page';
import {createNativeFieldJournalFlowRead,validateNativeJournalWorkspaceResponse,NativeFinanceReadError,type NativeFinanceReadTransport,type NativeJournalReadInput} from './native-journal-detail';
import {nativeFinanceCommandHttp,nativeFinanceReplyBytes} from './native-journal-post';

type Workspace={context:FinanceFieldJournalFlowResponse['context'];flow:FlowView;workspace:NonNullable<FinanceFieldJournalFlowResponse['workspace']>;access:{sourceOnly:boolean;canPost:boolean;purchaseControls:FinancePurchaseControls}};
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0&&Buffer.byteLength(v)<=200;
const integer=(v:unknown,min=0):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=min;
const nullable=(v:unknown)=>v===null||text(v);
const invalid=():never=>{throw new NativeFinanceReadError('NATIVE_FINANCE_RESPONSE_INVALID');};
const sameSet=(a:readonly string[],b:readonly string[])=>a.length===new Set(a).size&&b.length===new Set(b).size&&a.length===b.length&&a.every(v=>b.includes(v));
const closed=(status:number,code:string)=>Response.json({error:{code,message:'The current Field journal workflow is unavailable. Retain the original request for retry.'}},{status,headers:{'Cache-Control':'private, no-store'}});
function flow(value:unknown,id:string,version:unknown,state:unknown):FlowView{
 const person=(p:unknown)=>object(p)&&text(p.userId)&&typeof p.name==='string'&&nullable(p.teamId)&&typeof p.teamName==='string';
 const step=(s:unknown)=>object(s)&&s.key==='draft'&&typeof s.action==='string'&&s.role==='finance_approver'&&Array.isArray(s.allowedRoles)&&s.allowedRoles.every(text)&&typeof s.teamName==='string'&&(s.primary===null||person(s.primary))&&Array.isArray(s.candidates)&&s.candidates.length<=500&&s.candidates.every(person)&&['current','upcoming','recorded'].includes(String(s.status))&&(s.blocker===null||typeof s.blocker==='string');
 if(!object(value)||value.kind!=='journal'||value.recordId!==id||value.recordVersion!==version||value.state!==state||!integer(value.version)||!integer(value.defaultsRevision)||!['draft','in_review','posted','reversed'].includes(String(value.state))||typeof value.canAct!=='boolean'||typeof value.canManage!=='boolean'||!(value.blocker===null||typeof value.blocker==='string')||!Array.isArray(value.steps)||value.steps.length!==1||!step(value.steps[0])||!(value.current===null||step(value.current))||value.next!==null||!Array.isArray(value.history)||value.history.length>10000||!value.history.every(h=>object(h)&&text(h.action)&&typeof h.actorName==='string'&&integer(h.at)&&typeof h.reason==='string'&&h.step==='draft'))return invalid();
 const first=value.steps[0] as Record<string,unknown>,pool=first.candidates as Record<string,unknown>[];
 if(new Set(pool.map(p=>p.userId)).size!==pool.length||first.primary!==null&&!pool.some(p=>JSON.stringify(p)===JSON.stringify(first.primary))||value.current!==null&&JSON.stringify(value.current)!==JSON.stringify(first))return invalid();
 if(['posted','reversed'].includes(String(state))&&(value.current!==null||value.canAct!==false||value.blocker!==null))return invalid();
 return value as unknown as FlowView;
}
function validate(value:unknown,holding:{holdingId:string},input:NativeJournalReadInput):FinanceFieldJournalFlowResponse{
 if(!object(value)||Object.keys(value).sort().join(',')!=='access,context,data,runtime,workspace'||!['rust','legacy'].includes(String(value.runtime))||!object(value.context))return invalid();
 const c=value.context;
 if(Object.keys(c).sort().join(',')!=='assignmentId,assignmentScopeKind,assignmentVersion,branchId,branchSlug,companyId,departmentId,departmentSlug,expiresAt,holdingId,modules,permissions,revision,role,userId'||!text(c.userId)||c.holdingId!==holding.holdingId||!text(c.companyId)||!text(c.assignmentId)||!integer(c.assignmentVersion,1)||!integer(c.revision)||!integer(c.expiresAt,1)||!text(c.role)||!['holding','branch','department'].includes(String(c.assignmentScopeKind))||![c.branchId,c.departmentId,c.branchSlug,c.departmentSlug].every(nullable)||c.branchSlug!==(input.branchSlug??null)||c.departmentSlug!==(input.departmentSlug??null)||!Array.isArray(c.modules)||!c.modules.every(text)||!c.modules.includes('finance')||!Array.isArray(c.permissions)||!c.permissions.every(text)||!c.permissions.includes('finance.read')||!c.permissions.includes('module.finance.read'))return invalid();
 if(value.runtime==='legacy'){
  if(value.data!==null||value.workspace!==null||value.access!==null)return invalid();
 }else{
  const data=validateNativeJournalWorkspaceResponse({data:value.workspace,context:c},holding,input).data;
  if(data.journal.source_type!=='field_service_parts')return invalid();
  flow(value.data,input.journalId,data.journal.version,data.journal.status);
  if(!object(value.access)||Object.keys(value.access).sort().join(',')!=='canPost,purchaseControls,sourceOnly'||typeof value.access.canPost!=='boolean'||typeof value.access.sourceOnly!=='boolean')return invalid();
  nativeFinancePurchaseControls(value.access.purchaseControls);
  if(value.access.canPost&&object(value.data)&&value.data.canAct!==true)return invalid();
 }
 return value as unknown as FinanceFieldJournalFlowResponse;
}
export function createNativeFieldJournalFlowReader(raw:string,transport?:NativeFinanceReadTransport){
 const read=createNativeFieldJournalFlowRead(nativeFinanceConfiguration(raw),transport,validate);
 return async(actor:NativeJournalPageContext,incoming:Pick<Headers,'get'>,holdingSlug:string,id:string,signal?:AbortSignal):Promise<Workspace|undefined>=>{
  if(!actor.authority||!actor.iam||actor.holdingSlug!==holdingSlug)throw new NativeFinanceReadError('NATIVE_FINANCE_CONTEXT_CHANGED');
  const branchSlug=actor.scope.kind==='holding'?undefined:actor.scope.branchSlug,departmentSlug=actor.scope.kind==='department'?actor.scope.departmentSlug:undefined;
  const result=await read(incoming,{holdingSlug,journalId:id,branchSlug,departmentSlug,expected:{companyId:actor.companyId,assignmentId:actor.assignmentId,revision:actor.authority.revision,assignmentVersion:actor.authority.assignmentVersion}},signal);
  const expected={userId:actor.userId,holdingId:actor.holdingId,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,branchSlug:branchSlug??null,departmentSlug:departmentSlug??null,assignmentId:actor.assignmentId,assignmentVersion:actor.authority.assignmentVersion,revision:actor.authority.revision,role:actor.role,assignmentScopeKind:actor.assignmentScope};
  if(Object.entries(expected).some(([key,value])=>result.context[key as keyof typeof result.context]!==value)||!sameSet(actor.modules,result.context.modules)||!sameSet(actor.iam.permissions,result.context.permissions))throw new NativeFinanceReadError('NATIVE_FINANCE_CONTEXT_CHANGED');
  if(result.runtime==='legacy')return undefined;
  if(!result.workspace||!result.access)return invalid();
  return {context:result.context,flow:flow(result.data,id,result.workspace.journal.version,result.workspace.journal.status),workspace:result.workspace,access:{sourceOnly:result.access.sourceOnly,canPost:result.access.canPost,purchaseControls:nativeFinancePurchaseControls(result.access.purchaseControls)}};
 };
}
export async function nativeFieldJournalFlowWorkspace(actor:NativeJournalPageContext,holdingSlug:string,id:string){
 const raw=process.env.ERPX_NATIVE_FINANCE_SERVICE_INVOICE_CONFIG;
 if(raw===undefined)throw new NativeFinanceReadError('NATIVE_FINANCE_CONFIGURATION_REQUIRED');
 return createNativeFieldJournalFlowReader(raw)(actor,await headers(),holdingSlug,id);
}
function denial(error:unknown):Response{
 const status=error instanceof NativeFinanceReadError&&[401,403,404,409,422].includes(error.status??0)?error.status!:error instanceof NativeFinanceReadError&&error.code==='NATIVE_FINANCE_CONTEXT_CHANGED'?409:503;
 return closed(status,error instanceof NativeFinanceReadError?error.code:'NATIVE_FINANCE_UNAVAILABLE');
}
export async function nativeFieldJournalFlowReadback(request:Request,actor:NativeJournalPageContext,holdingSlug:string,id:string):Promise<Response|undefined>{
 try{
  const raw=process.env.ERPX_NATIVE_FINANCE_SERVICE_INVOICE_CONFIG;if(raw===undefined)throw new NativeFinanceReadError('NATIVE_FINANCE_CONFIGURATION_REQUIRED');
  const source=await createNativeFieldJournalFlowReader(raw)(actor,request.headers,holdingSlug,id,request.signal);
  return source?Response.json({data:source.flow},{headers:{'Cache-Control':'private, no-store'}}):undefined;
 }catch(error){return denial(error);}
}
export function createNativeFieldJournalFlowCommands(raw:string,readTransport?:NativeFinanceReadTransport,writeTransport:typeof nativeFinanceCommandHttp=nativeFinanceCommandHttp){
 const read=createNativeFieldJournalFlowReader(raw,readTransport),settings=nativeFinanceConfiguration(raw);
 return async(request:Request,actor:NativeJournalPageContext,holdingSlug:string,id:string):Promise<Response|undefined>=>{
  let acknowledged:FinanceFieldJournalFlowCommandResponse|undefined;
  const uncertain=()=>Response.json({error:{code:'NATIVE_FINANCE_READBACK_UNCERTAIN',message:'The responsibility command was acknowledged. Retain its original key and payload while refreshing.'},...(acknowledged?{command:acknowledged.data}:{})},{status:503,headers:{'Cache-Control':'private, no-store'}});
  let timer:ReturnType<typeof setTimeout>|undefined;const controller=new AbortController(),cancel=()=>controller.abort();
  try{
   const source=await read(actor,request.headers,holdingSlug,id,request.signal);if(!source)return undefined;
   const origin=settings.holdings.find(h=>h.holdingSlug===holdingSlug&&new URL(h.origin).host===request.headers.get('host'))?.origin;
   if(!origin||request.headers.get('origin')!==null&&request.headers.get('origin')!==origin)return closed(403,'ORIGIN_FORBIDDEN');
   const expected={'x-erpx-context-company':actor.companyId,'x-erpx-context-assignment':actor.assignmentId,'x-erpx-context-revision':String(actor.authority!.revision),'x-erpx-context-assignment-version':String(actor.authority!.assignmentVersion)};
   if(Object.entries(expected).some(([key,value])=>request.headers.get(key)!==value))return closed(409,'WORK_CONTEXT_CHANGED');
   request.signal.addEventListener('abort',cancel,{once:true});if(request.signal.aborted)cancel();timer=setTimeout(cancel,settings.timeoutMs);
   const body=await nativeFinanceReplyBytes(request.body,2*1024*1024),forward=new Headers({host:request.headers.get('host')!,accept:'application/json',...expected});
   for(const name of ['cookie','origin','content-type','idempotency-key','x-erpx-workspace-branch','x-erpx-workspace-department']){const value=request.headers.get(name);if(value!==null)forward.set(name,value);}
   const url=new URL(`/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/journals/${encodeURIComponent(id)}/flow`,settings.privateBaseUrl);
   const reply=await writeTransport(url,forward,body,controller.signal);
   if(reply.redirected||reply.status>=300&&reply.status<400||reply.headers.get('content-type')?.split(';')[0]?.trim()!=='application/json')return closed(502,'NATIVE_FINANCE_RESPONSE_INVALID');
   const value:unknown=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(await nativeFinanceReplyBytes(reply.body,2*1024*1024)));
   if(reply.status!==200){
    if(!object(value)||!object(value.error)||!text(value.error.code)||typeof value.error.message!=='string')return closed(502,'NATIVE_FINANCE_RESPONSE_INVALID');
    return Response.json(value,{status:reply.status,headers:{'Cache-Control':'private, no-store'}});
   }
   if(!object(value)||Object.keys(value).join(',')!=='data'||!object(value.data)||Object.keys(value.data).sort().join(',')!=='id,version'||value.data.id!==id||!integer(value.data.version,2))return closed(502,'NATIVE_FINANCE_RESPONSE_INVALID');
   const acknowledgement=value as unknown as FinanceFieldJournalFlowCommandResponse;acknowledged=acknowledgement;
   const current=await read(actor,request.headers,holdingSlug,id,request.signal);if(!current)return uncertain();
   return Response.json({data:current.flow,command:acknowledgement.data},{headers:{'Cache-Control':'private, no-store','X-ERPX-Native-Flow':'field-journal'}});
  }catch(error){return acknowledged?uncertain():denial(error);}
  finally{controller.abort();if(timer)clearTimeout(timer);request.signal.removeEventListener('abort',cancel);}
 };
}
export async function nativeFieldJournalFlowCommand(request:Request,actor:NativeJournalPageContext,holdingSlug:string,id:string){
 const raw=process.env.ERPX_NATIVE_FINANCE_SERVICE_INVOICE_CONFIG;if(raw===undefined)return closed(503,'NATIVE_FINANCE_CONFIGURATION_REQUIRED');
 return createNativeFieldJournalFlowCommands(raw)(request,actor,holdingSlug,id);
}

export async function nativeFieldJournalWorkspaceReadback(request:Request,actor:NativeJournalPageContext,holdingSlug:string,id:string):Promise<Response|undefined>{
 try{
  const raw=process.env.ERPX_NATIVE_FINANCE_SERVICE_INVOICE_CONFIG;if(raw===undefined)throw new NativeFinanceReadError('NATIVE_FINANCE_CONFIGURATION_REQUIRED');
  const source=await createNativeFieldJournalFlowReader(raw)(actor,request.headers,holdingSlug,id,request.signal);
  return source?Response.json({data:source.workspace,context:source.context},{headers:{'Cache-Control':'private, no-store'}}):undefined;
 }catch(error){return denial(error);}
}
/** An absent native journal action may never select a legacy Field writer. */
export async function nativeFieldJournalLegacyUnavailable(request:Request,actor:NativeJournalPageContext,holdingSlug:string,id:string):Promise<Response|undefined>{
 try{
  const raw=process.env.ERPX_NATIVE_FINANCE_SERVICE_INVOICE_CONFIG;if(raw===undefined)throw new NativeFinanceReadError('NATIVE_FINANCE_CONFIGURATION_REQUIRED');
  const source=await createNativeFieldJournalFlowReader(raw)(actor,request.headers,holdingSlug,id,request.signal);
  return source?closed(503,'NATIVE_FINANCE_CONFIGURATION_REQUIRED'):undefined;
 }catch(error){return denial(error);}
}
