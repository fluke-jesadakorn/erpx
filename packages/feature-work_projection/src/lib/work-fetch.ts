import {holdingFetch} from '@erpx/contracts/holding-fetch';
import { announceWorkChange } from "@erpx/feature-work_projection/lib/work-refresh";
import { contextGeneration, contextIsSwitching } from "@erpx/feature-work_projection/lib/context-switch";
export type WorkCommandCompletion = { form: HTMLFormElement; formEditVersion: string | null };
/** Sends the context rendered with this page, never a freshly fetched role. */
export function workFetch(input:RequestInfo|URL,init?:RequestInit,completion?:WorkCommandCompletion,acknowledgement:"automatic"|"caller"="automatic"):Promise<Response>{
 const url=new URL(input instanceof Request?input.url:String(input),typeof window==='undefined'?'https://erpx.invalid':window.location.origin);
 if(typeof document==='undefined'||url.origin!==window.location.origin||(!url.pathname.startsWith('/api/v1/holdings/')&&!url.pathname.startsWith('/api/v1/platform/')&&!url.pathname.startsWith('/api/v1/front/')))return holdingFetch(input,init);
 if(url.pathname.startsWith('/api/v1/holdings/')&&(url.pathname.endsWith('/assignments/active')||url.pathname.endsWith('/companies/active')))return holdingFetch(input,init);
 const generation=contextGeneration();
 const node=document.querySelector<HTMLElement>('[data-work-revision]');
 const stamp = node ? [node.dataset.workUser,node.dataset.workRevision,node.dataset.workCompany,node.dataset.workAssignment].join(':') : null;
 const headers=new Headers(init?.headers??(input instanceof Request?input.headers:undefined));
 if(node){headers.set('x-erpx-context-revision',node.dataset.workRevision!);headers.set('x-erpx-context-company',node.dataset.workCompany!);headers.set('x-erpx-context-assignment',node.dataset.workAssignment!);}
 const platformNode=document.querySelector<HTMLElement>('[data-platform-assignment]');
 if(url.pathname.startsWith('/api/v1/platform/')&&platformNode)headers.set('x-erpx-platform-assignment',platformNode.dataset.platformAssignment!);
 const method=(init?.method??(input instanceof Request?input.method:'GET')).toUpperCase();
 const recordForm=completion ? completion.form : document.activeElement?.closest?.('form')??null;
 const formEditVersion=completion ? completion.formEditVersion : recordForm?.getAttribute?.('data-erpx-edit-version')??null;
 const cleanup=method==='DELETE' && (url.pathname.endsWith('/push')||url.pathname.endsWith('/push/session'));
 const selecting=url.pathname==='/api/v1/platform/assignments/active';
 if(contextIsSwitching()&&!['GET','HEAD','OPTIONS'].includes(method)&&!cleanup&&!selecting)return Promise.resolve(Response.json({error:{code:'WORK_CONTEXT_CHANGED',message:'Account and permissions are updating. Wait before submitting.'}},{status:409}));
 const frontNode=document.querySelector<HTMLElement>('[data-front-app]');
  if(frontNode&&url.pathname.startsWith('/api/v1/front/')){
  headers.set('x-erpx-front-location',frontNode.dataset.frontLocation??'');
  if(!['GET','HEAD','OPTIONS'].includes(method)&&!navigator.onLine)return Promise.resolve(Response.json({error:{code:'FRONT_APP_OFFLINE',message:'Reconnect before submitting this storefront action.'}},{status:503}));
 }
 if(node?.dataset.workStale==='true'&&!['GET','HEAD','OPTIONS'].includes(method))return Promise.resolve(Response.json({error:{code:'WORK_CONTEXT_CHANGED',message:'Your role changed in another tab. Reload and review before submitting.'}},{status:409}));
 return holdingFetch(input,{...init,headers}).then(async response=>{
 if(response.status===409){const body=await response.clone().json().catch(()=>null);if(["WORK_CONTEXT_CHANGED","WORK_ROLE_REQUIRED","WORK_ROLE_UNAVAILABLE"].includes(body?.error?.code))window.dispatchEvent(new Event('erpx:work-context-changed'));}
 if(generation!==contextGeneration() || url.pathname.startsWith('/api/v1/platform/') && platformNode && (!platformNode.isConnected || headers.get('x-erpx-platform-assignment')!==platformNode.dataset.platformAssignment) || node && (!node.isConnected || node.dataset.workStale==='true' || stamp !== [node.dataset.workUser,node.dataset.workRevision,node.dataset.workCompany,node.dataset.workAssignment].join(':')))return Response.json({error:{code:'WORK_CONTEXT_CHANGED',message:'Your work role changed. Reload and review.'}},{status:409});
 if(acknowledgement==="automatic" && response.ok && !["GET","HEAD","OPTIONS"].includes(method) && !url.pathname.endsWith("/navigation")) {
  window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{form:recordForm,formEditVersion}}));
  announceWorkChange();
 }
 return response;
 });
}
