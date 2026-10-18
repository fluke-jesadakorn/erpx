const selfPaths=new Set(['/my-access','/security','/my-profile']);
const credentials=['code','state','token','session_token','_rsc'];

/** Only explicit account-self pages may cross product hosts without a Company.
 * A Company profile, API, callback or arbitrary return is never a self alias. */
export function accountWorkspaceTarget(value:string|null|undefined):string|null {
 if(!value||value.length>2000||!value.startsWith('/')||value.startsWith('//')||/[\\\s\u0000-\u001f\u007f]/u.test(value))return null;
 let decoded:string,url:URL;
 try{decoded=decodeURIComponent(value);url=new URL(value,'https://account.invalid');}catch{return null;}
 if(decoded.startsWith('//')||/[\\\u0000-\u001f\u007f]/u.test(decoded)||url.origin!=='https://account.invalid'||!selfPaths.has(url.pathname)||value.split(/[?#]/,1)[0]!==url.pathname)return null;
 for(const key of credentials)url.searchParams.delete(key);
 return url.pathname+url.search+url.hash;
}
import {safeProductReturnPath} from './product-sso';

/** Login receives only the closed GET bridge, never an arbitrary Account path. */
export function accountWorkspaceLoginReturn(value:string|null|undefined):string|null {
 let target=accountWorkspaceTarget(value);
 if(!target&&value&&value.startsWith('/account/launch?')&&value.length<=4096){
  let url:URL;try{url=new URL(value,'https://account.invalid');decodeURIComponent(value);}catch{return null;}
  if(url.pathname!=='/account/launch'||url.hash||url.searchParams.getAll('next').length!==1||[...url.searchParams.keys()].some(key=>key!=='next'))return null;
  target=accountWorkspaceTarget(url.searchParams.get('next'));
 }
 if(!target)return null;
 const next=safeProductReturnPath('/account/launch?'+new URLSearchParams({next:target}));
 return next==='/'?null:next;
}
