import {safeProductReturnPath} from '@erpx/server-identity_access/domain/product-sso';
export type PublicSurfaceHost='booking'|'portal'|'legal-portal'|'forms';
const identifier='[A-Za-z0-9_-]+';
export function legalEnrollmentEntryTarget(value:string|null|undefined):string|null{
 if(!value||value.length>8500||/[\\\s\u0000-\u001f\u007f]/u.test(value))return null;
 let url:URL;try{url=new URL(value,'https://account.invalid');decodeURIComponent(value);}catch{return null;}
 if(value.split(/[?#]/,1)[0]!=='/legal-portal/enroll'||url.pathname!=='/legal-portal/enroll'||url.search||url.origin!=='https://account.invalid')return null;
 if(!url.hash)return '/legal-portal/enroll';
 const fragment=new URLSearchParams(url.hash.slice(1));
 if(fragment.getAll('token').length!==1||[...fragment.keys()].some(key=>key!=='token'))return null;
 const token=fragment.get('token');if(!token||token.length>8000||!/^[A-Za-z0-9_.-]+$/.test(token))return null;
 return '/legal-portal/enroll#'+new URLSearchParams({token});
}
export function publicSurfaceTarget(value:string|null|undefined):{host:PublicSurfaceHost;target:string}|null{
 if(!value||value.length>2000||!value.startsWith('/')||value.startsWith('//')||/[\\\s\u0000-\u001f\u007f]/u.test(value))return null;
 let decoded:string,url:URL;try{decoded=decodeURIComponent(value);url=new URL(value,'https://public.invalid');}catch{return null;}
 if(decoded.startsWith('//')||/[\\\u0000-\u001f\u007f]/u.test(decoded)||url.origin!=='https://public.invalid'||value.split(/[?#]/,1)[0]!==url.pathname)return null;
 let host:PublicSurfaceHost;
 if(new RegExp(`^/book/${identifier}(?:/${identifier})?$`).test(url.pathname))host='booking';
 else if(new RegExp(`^/customer-portal/${identifier}(?:/tickets)?$`).test(url.pathname))host='portal';
 else if(new RegExp(`^/legal-portal/${identifier}$`).test(url.pathname)&&!['enroll','continue'].includes(url.pathname.split('/')[2]))host='legal-portal';
 else if(new RegExp(`^/forms/${identifier}/${identifier}$`).test(url.pathname))host='forms';
 else return null;
 for(const key of ['code','state','token','session_token','_rsc'])url.searchParams.delete(key);
 return {host,target:url.pathname+url.search+url.hash};
}
export function publicSurfaceLaunchPath(value:string|null|undefined){
 const target=publicSurfaceTarget(value);if(!target||!['portal','legal-portal'].includes(target.host))return null;
 const next=safeProductReturnPath('/public/launch?'+new URLSearchParams({next:target.target}));return next==='/'?null:next;
}
/** Only actual verified invitation continuation identifiers survive login. No
 * token is moved from its invitation fragment into a return query. */
export function legalEnrollmentReturnPath(value:string|null|undefined):string|null{
 if(!value||value.length>2000||!value.startsWith('/legal-portal/continue?')||/[\\\s\u0000-\u001f\u007f]/u.test(value))return null;
 let url:URL;try{url=new URL(value,'https://account.invalid');decodeURIComponent(value);}catch{return null;}
 if(url.pathname!=='/legal-portal/continue'||url.hash||url.searchParams.getAll('holding').length!==1||url.searchParams.getAll('share').length!==1||[...url.searchParams.keys()].some(key=>key!=='holding'&&key!=='share'))return null;
 const holding=url.searchParams.get('holding')!,share=url.searchParams.get('share')!;
 if(!/^[A-Za-z0-9_-]{1,100}$/.test(holding)||!/^[A-Za-z0-9_-]{1,200}$/.test(share))return null;
 return '/legal-portal/continue?'+new URLSearchParams({holding,share});
}
