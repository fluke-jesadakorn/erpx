export function safeProductReturnPath(value: string | null | undefined) {
  if (!value || value.length > 4096 || !value.startsWith("/") || value.startsWith("//") || /[\\\s\u0000-\u001f\u007f]/u.test(value)) return "/";
  let decoded: string;
  try { decoded = decodeURIComponent(value); } catch { return "/"; }
  if (decoded.startsWith("//") || /[\\\u0000-\u001f\u007f]/u.test(decoded)) return "/";
  const url = new URL(value, "https://product.invalid");
  if (url.origin !== "https://product.invalid" || /^\/(?:sso|api\/auth|login|product-entry)(?:\/|$)/.test(decodeURIComponent(url.pathname))) return "/";
  // Never carry callback credentials on to a clean application URL.
  for (const key of ["code", "state", "token", "session_token", "_rsc"]) url.searchParams.delete(key);
  return url.pathname + url.search;
}


import {isProductHostKey,productIds} from './product-registry.ts';
import {holdingHostForSlug} from './holding-hosts.ts';

/** Closed return from the existing normal Account/Directory login. */
export function deviceAccountReturn(value:string|null|undefined){
 if(!value||value.length>4096||!value.startsWith('/account/device-return?'))return null;
 try{const url=new URL(value,'https://account.invalid'),app=url.searchParams.get('app');
  const holding=url.searchParams.get('holding');
  if(url.pathname!=='/account/device-return'||url.hash||!app||!isProductHostKey(app)||!['apps','admin',...productIds].includes(app)||[...url.searchParams.keys()].some(key=>!['app','next','holding'].includes(key))||url.searchParams.getAll('app').length!==1||url.searchParams.getAll('next').length>1||url.searchParams.getAll('holding').length>1||holding&&!holdingHostForSlug(holding))return null;
  return '/account/device-return?'+new URLSearchParams({app,next:safeProductReturnPath(url.searchParams.get('next')),...holding?{holding}:{}});
 }catch{return null;}
}
/** Use a Company-provided Account link without forwarding its extra query. */
export function deviceDirectoryLogin(value:string,accountOrigin:string,app:string,next:string,holding?:string){
 try{
  const url=new URL(value),origin=new URL(accountOrigin).origin,connection=url.searchParams.get('connection');
  if(url.origin!==origin||url.username||url.password||url.pathname!=='/login'||url.hash||url.searchParams.getAll('connection').length!==1||!connection||!/^[a-f0-9-]{36}$/i.test(connection))return null;
  const target=deviceAccountReturn('/account/device-return?'+new URLSearchParams({app,next,...holding?{holding}:{}}));
  return target?origin+'/login?'+new URLSearchParams({connection,next:target}):null;
 }catch{return null;}
}
