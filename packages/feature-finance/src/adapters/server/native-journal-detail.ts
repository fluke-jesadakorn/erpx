import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import type { FinanceJournalReadResponse, FinanceJournalWorkspaceReadResponse, FinanceLedgerValue } from '@erpx/contracts/finance-journal-read';

export type NativeFinanceReadConfiguration=Readonly<{privateBaseUrl:string;holdings:readonly Readonly<{origin:string;holdingSlug:string;holdingId:string}>[];timeoutMs:number}>;
export type NativeJournalReadInput=Readonly<{holdingSlug:string;journalId:string;branchSlug?:string;departmentSlug?:string;expected?:Readonly<{companyId:string;assignmentId:string;revision:number;assignmentVersion:number}>}>;
export type NativeFinanceReadTransport=(url:URL,headers:Headers,signal:AbortSignal)=>Promise<Response>;
type NativeJournalRead<T>=(incoming:Pick<Headers,'get'>,input:NativeJournalReadInput,signal?:AbortSignal)=>Promise<T>;
export type NativeJournalDetailRead=NativeJournalRead<FinanceJournalReadResponse>;
export type NativeJournalWorkspaceRead=NativeJournalRead<FinanceJournalWorkspaceReadResponse>;
export class NativeFinanceReadError extends Error{
 readonly code:string;readonly status?:number;
 constructor(code:string,status?:number){super('The native Finance detail read could not be completed.');this.name='NativeFinanceReadError';this.code=code;this.status=status;}
}
const failure=(code:string,status?:number):never=>{throw new NativeFinanceReadError(code,status);};
function validText(value:unknown,max=200):value is string{return typeof value==='string'&&value.length>0&&new TextEncoder().encode(value).length<=max;}
function integer(value:unknown,min=0):value is number{return typeof value==='number'&&Number.isSafeInteger(value)&&value>=min;}
function record(value:unknown):value is Record<string,unknown>{return value!==null&&typeof value==='object'&&!Array.isArray(value);}
function exact(value:unknown,keys:readonly string[]):value is Record<string,unknown>{return record(value)&&Object.keys(value).length===keys.length&&keys.every(k=>Object.hasOwn(value,k));}
function ledger(value:unknown):value is Record<string,FinanceLedgerValue>{return record(value)&&Object.values(value).every(v=>v===null||typeof v==='string'||typeof v==='number'&&Number.isSafeInteger(v));}
function texts(value:unknown):value is string[]{return Array.isArray(value)&&value.length<=4096&&value.every(v=>validText(v,256));}
function response(value:unknown,holding:{holdingId:string},input:NativeJournalReadInput):FinanceJournalReadResponse{
 if(!exact(value,['data','context'])||!exact(value.data,['journal','lines','history'])||!ledger(value.data.journal)||!Array.isArray(value.data.lines)||!value.data.lines.every(ledger)||!Array.isArray(value.data.history)||!value.data.history.every(ledger))return failure('NATIVE_FINANCE_RESPONSE_INVALID');
 if(value.data.lines.length+value.data.history.length+1>10000)return failure('NATIVE_FINANCE_RESPONSE_INVALID');
 const c=value.context,j=value.data.journal;
 if(!exact(c,['userId','holdingId','companyId','branchId','departmentId','branchSlug','departmentSlug','assignmentId','assignmentVersion','revision','role','assignmentScopeKind','modules','permissions','expiresAt'])
  ||!validText(c.userId)||c.holdingId!==holding.holdingId||!validText(c.companyId)||c.companyId!==j.company_id||!validText(c.assignmentId)||!integer(c.assignmentVersion,1)||!integer(c.revision)||!integer(c.expiresAt,1)||!validText(c.role)||!['holding','branch','department'].includes(String(c.assignmentScopeKind))||!texts(c.modules)||!c.modules.includes('finance')||!texts(c.permissions)||!c.permissions.includes('finance.read')||!c.permissions.includes('module.finance.read')
  ||!['branchId','departmentId','branchSlug','departmentSlug'].every(k=>c[k]===null||validText(c[k],128))||c.branchSlug!==(input.branchSlug??null)||c.departmentSlug!==(input.departmentSlug??null)
  ||j.id!==input.journalId||!validText(j.chart_id)||!validText(j.journal_number)||!validText(j.entry_date,32)||!integer(j.version,1)||!['draft','in_review','posted','reversed'].includes(String(j.status))||typeof j.created_by_name!=='string'||typeof j.posted_by_name!=='string'
  ||value.data.lines.some(l=>l.journal_id!==j.id||!validText(l.id)||!validText(l.account_id)||typeof l.code!=='string'||typeof l.name!=='string'||!integer(l.ordinal)||!integer(l.debit_minor)||!integer(l.credit_minor))
  ||value.data.history.some(h=>h.journal_id!==j.id||h.company_id!==c.companyId||!integer(h.journal_version,1)||typeof h.snapshot_json!=='string'))return failure('NATIVE_FINANCE_RESPONSE_INVALID');
 if(input.expected&&(c.companyId!==input.expected.companyId||c.assignmentId!==input.expected.assignmentId||c.revision!==input.expected.revision||c.assignmentVersion!==input.expected.assignmentVersion))return failure('NATIVE_FINANCE_CONTEXT_CHANGED');
 return value as unknown as FinanceJournalReadResponse;
}
function workspaceResponse(value:unknown,holding:{holdingId:string},input:NativeJournalReadInput):FinanceJournalWorkspaceReadResponse {
 if(!exact(value,['data','context'])||!exact(value.data,['journal','lines','history','chart','accounts','periods','dimensions','dimensionValues']))return failure('NATIVE_FINANCE_RESPONSE_INVALID');
 const d=value.data;
 const chart=d.chart;
 const common=response({data:{journal:d.journal,lines:d.lines,history:d.history},context:value.context},holding,input);
 if(!ledger(chart)||chart.id!==common.data.journal.chart_id||chart.company_id!==common.context.companyId||typeof chart.name!=='string'||typeof chart.base_currency!=='string'
  ||!Array.isArray(d.accounts)||!d.accounts.every(ledger)||!Array.isArray(d.periods)||!d.periods.every(ledger)||!Array.isArray(d.dimensions)||!d.dimensions.every(ledger)||!Array.isArray(d.dimensionValues)||!d.dimensionValues.every(ledger)
  ||1+common.data.lines.length+common.data.history.length+1+d.accounts.length+d.periods.length+d.dimensions.length+d.dimensionValues.length>10000
  ||d.accounts.some(a=>!validText(a.id)||a.company_id!==common.context.companyId||a.chart_id!==chart.id||typeof a.code!=='string'||typeof a.name!=='string'||!['active','inactive'].includes(String(a.status)))
  ||d.periods.some(p=>!validText(p.id)||p.company_id!==common.context.companyId||typeof p.start_date!=='string'||typeof p.end_date!=='string'||!['open','closing','closed'].includes(String(p.state)))
  ||d.dimensions.some(v=>!exact(v,['id','code','name','required'])||!validText(v.id)||typeof v.code!=='string'||typeof v.name!=='string'||!integer(v.required)||![0,1].includes(v.required))
  ||d.dimensionValues.some(v=>!exact(v,['id','dimension_id','code','name'])||!validText(v.id)||!validText(v.dimension_id)||typeof v.code!=='string'||typeof v.name!=='string'))return failure('NATIVE_FINANCE_RESPONSE_INVALID');
 return value as unknown as FinanceJournalWorkspaceReadResponse;
}
const reserved=new Set([3000,3001,3100,3101,3103,5432,5433,5435,8080]);
export function createNativeJournalDetailClient(configuration:NativeFinanceReadConfiguration,transport:NativeFinanceReadTransport=directHttp):NativeJournalDetailRead {
 return createRead(configuration,transport,response,'journals','');
}
export function createNativeJournalWorkspaceClient(configuration:NativeFinanceReadConfiguration,transport:NativeFinanceReadTransport=directHttp):NativeJournalWorkspaceRead {
 return createRead(configuration,transport,workspaceResponse,'journals','/workspace');
}
export function createNativeFinanceWorkspaceRead<T>(configuration:NativeFinanceReadConfiguration,transport:NativeFinanceReadTransport|undefined,validate:(value:unknown,holding:{holdingId:string},input:NativeJournalReadInput)=>T):NativeJournalRead<T>{
 return createRead(configuration,transport??directHttp,validate,'invoices','/workspace');
}
function createRead<T>(configuration:NativeFinanceReadConfiguration,transport:NativeFinanceReadTransport,validate:(value:unknown,holding:{holdingId:string},input:NativeJournalReadInput)=>T,kind:'journals'|'invoices',suffix:''|'/workspace'):NativeJournalRead<T>{
 let base:URL;try{base=new URL(configuration.privateBaseUrl);}catch{return failure('NATIVE_FINANCE_CONFIGURATION_INVALID');}
 const port=Number(base.port);
 if(base.protocol!=='http:'||!['127.0.0.1','[::1]'].includes(base.hostname)||!base.port||reserved.has(port)||port>=3200&&port<=3299||base.username||base.password||configuration.privateBaseUrl!==base.origin||!integer(configuration.timeoutMs,250)||configuration.timeoutMs>30000||configuration.holdings.length<1||configuration.holdings.length>64)return failure('NATIVE_FINANCE_CONFIGURATION_INVALID');
 const holdings=new Map<string,{holdingId:string;holdingSlug:string}>();
 for(const h of configuration.holdings){let origin:URL;try{origin=new URL(h.origin);}catch{return failure('NATIVE_FINANCE_CONFIGURATION_INVALID');}
  if(origin.protocol!=='https:'||h.origin!==origin.origin||origin.port||origin.username||origin.password||!origin.hostname.includes('.')||!origin.hostname.split('.').every(p=>/^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(p))||!validText(h.holdingId)||!validText(h.holdingSlug,128)||!/^[a-z0-9_-]+$/.test(h.holdingSlug)||holdings.has(origin.host))return failure('NATIVE_FINANCE_CONFIGURATION_INVALID');
  holdings.set(origin.host,{holdingId:h.holdingId,holdingSlug:h.holdingSlug});
 }
 const timeout=configuration.timeoutMs;
 return async(incoming,requested,signal)=>{
  const input={...requested,expected:requested.expected?{...requested.expected}:undefined};
  const host=incoming.get('host'),cookie=incoming.get('cookie'),holding=host?holdings.get(host):undefined;
  if(!holding||holding.holdingSlug!==input.holdingSlug)return failure('NATIVE_FINANCE_AUTHORITY_INVALID');
  if(!validText(input.journalId)||input.journalId.includes('/')||input.journalId.includes('\\'))return failure('NATIVE_FINANCE_REQUEST_INVALID');
  const headers=new Headers({host:host!,accept:'application/json','cache-control':'no-cache'});
  if(cookie){if(new TextEncoder().encode(cookie).length>8192)return failure('NATIVE_FINANCE_COOKIE_LIMIT');headers.set('cookie',cookie);}
  for(const [key,slug]of [['x-erpx-workspace-branch',input.branchSlug],['x-erpx-workspace-department',input.departmentSlug]] as const){if(slug!==undefined){if(!validText(slug,128)||!/^[a-z0-9_-]+$/.test(slug))return failure('NATIVE_FINANCE_REQUEST_INVALID');headers.set(key,slug);}}
  if(input.expected){const e=input.expected;if(!validText(e.companyId)||!validText(e.assignmentId)||!integer(e.revision)||!integer(e.assignmentVersion,1))return failure('NATIVE_FINANCE_REQUEST_INVALID');headers.set('x-erpx-context-company',e.companyId);headers.set('x-erpx-context-assignment',e.assignmentId);headers.set('x-erpx-context-revision',String(e.revision));headers.set('x-erpx-context-assignment-version',String(e.assignmentVersion));}
  const url=new URL(`/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/finance/${kind}/${encodeURIComponent(input.journalId)}${suffix}`,base);
  const controller=new AbortController();const cancel=()=>controller.abort();signal?.addEventListener('abort',cancel,{once:true});if(signal?.aborted)controller.abort();const timer=setTimeout(cancel,timeout);
  try{
   const result=await transport(url,headers,controller.signal);
   if(result.redirected||result.status!==200){void result.body?.cancel().catch(()=>undefined);return failure('NATIVE_FINANCE_UNAVAILABLE',result.status);}
   if(result.headers.get('content-type')?.split(';')[0]?.trim()!=='application/json'||!result.body){void result.body?.cancel().catch(()=>undefined);return failure('NATIVE_FINANCE_RESPONSE_INVALID');}
   const reader=result.body.getReader();const chunks:Uint8Array[]=[];let bytes=0;const abortBody=()=>{void reader.cancel().catch(()=>undefined);};controller.signal.addEventListener('abort',abortBody,{once:true});
   try{for(;;){if(controller.signal.aborted){await reader.cancel();return failure('NATIVE_FINANCE_ABORTED');}const next=await reader.read();if(controller.signal.aborted)return failure('NATIVE_FINANCE_ABORTED');if(next.done)break;bytes+=next.value.byteLength;if(bytes>2*1024*1024){await reader.cancel();return failure('NATIVE_FINANCE_RESPONSE_LIMIT');}chunks.push(next.value);}}
   finally{controller.signal.removeEventListener('abort',abortBody);reader.releaseLock();}
   const data=new Uint8Array(bytes);let offset=0;for(const chunk of chunks){data.set(chunk,offset);offset+=chunk.length;}
   let parsed:unknown;try{parsed=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(data));}catch{return failure('NATIVE_FINANCE_RESPONSE_INVALID');}
   return validate(parsed,holding,input);
  }catch(error){const aborted=controller.signal.aborted;controller.abort();if(error instanceof NativeFinanceReadError)throw error;return failure(aborted?'NATIVE_FINANCE_ABORTED':'NATIVE_FINANCE_UNAVAILABLE');}
  finally{clearTimeout(timer);signal?.removeEventListener('abort',cancel);}
 };
}
/** Fresh bounded GET socket; bypasses patched fetch/HMR and all ambient proxies. */
function directHttp(url:URL,headers:Headers,signal:AbortSignal):Promise<Response>{
 return new Promise((resolve,reject)=>{const agent=new Agent({keepAlive:false,maxSockets:1,maxTotalSockets:1,proxyEnv:{} as import('node:http').ProxyEnv});const outgoing=request(url,{method:'GET',agent,headers:Object.fromEntries(headers),signal,maxHeaderSize:8192},incoming=>{incoming.once('close',()=>agent.destroy());try{const status=incoming.statusCode??502;if(status!==200){incoming.destroy();resolve(new Response(null,{status:status>=200&&status<=599?status:502}));return;}const values=new Headers();for(let i=0;i<incoming.rawHeaders.length;i+=2)values.append(incoming.rawHeaders[i]!,incoming.rawHeaders[i+1]!);resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>,{status,headers:values}));}catch(error){incoming.destroy();agent.destroy();reject(error);}});outgoing.once('error',error=>{agent.destroy();reject(error);});outgoing.end();});
}
