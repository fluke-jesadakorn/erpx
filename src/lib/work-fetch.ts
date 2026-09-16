/** Sends the context rendered with this page, never a freshly fetched role. */
export function workFetch(input:RequestInfo|URL,init?:RequestInit):Promise<Response>{
 const url=new URL(input instanceof Request?input.url:String(input),typeof window==='undefined'?'https://erpx.invalid':window.location.origin);
 if(typeof document==='undefined'||url.origin!==window.location.origin||!url.pathname.startsWith('/api/v1/holdings/'))return fetch(input,init);
 if((url.pathname.endsWith('/assignments/active')||url.pathname.endsWith('/companies/active')))return fetch(input,init);
 const node=document.querySelector<HTMLElement>('[data-work-revision]');
 const headers=new Headers(init?.headers??(input instanceof Request?input.headers:undefined));
 if(node){headers.set('x-erpx-context-revision',node.dataset.workRevision!);headers.set('x-erpx-context-company',node.dataset.workCompany!);headers.set('x-erpx-context-assignment',node.dataset.workAssignment!);}
 const method=(init?.method??(input instanceof Request?input.method:'GET')).toUpperCase();
 if(node?.dataset.workStale==='true'&&!['GET','HEAD','OPTIONS'].includes(method))return Promise.resolve(Response.json({error:{code:'WORK_CONTEXT_CHANGED',message:'Your role changed in another tab. Reload and review before submitting.'}},{status:409}));
 return fetch(input,{...init,headers}).then(async response=>{
 if(response.status===409){const body=await response.clone().json().catch(()=>null);if(["WORK_CONTEXT_CHANGED","WORK_ROLE_REQUIRED","WORK_ROLE_UNAVAILABLE"].includes(body?.error?.code))window.dispatchEvent(new Event('erpx:work-context-changed'));}
 if(node?.dataset.workStale==='true')return Response.json({error:{code:'WORK_CONTEXT_CHANGED',message:'Your work role changed. Reload and review.'}},{status:409});
 return response;
 });
}
