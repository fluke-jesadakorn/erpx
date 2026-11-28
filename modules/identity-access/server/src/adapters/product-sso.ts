import 'server-only';
import {nativeAccountHttp} from './native-account-http';
export {productSessionCookie,ssoOrigins} from './product-sso-context';
export async function productSsoRequest(request:Request,action:string){
 if(!['start','authorize','callback'].includes(action)||new URL(request.url).pathname!==`/sso/${action}`)return new Response(null,{status:404});
 return nativeAccountHttp(request);
}
// Existing Legal/SSO consumer tests retain this named HTTP port. It accepts
// only sign-out and delegates every decision/effect to the native endpoint.
export function productSignOut(request:Request){
 if(new URL(request.url).pathname!=='/api/auth/sign-out')return Promise.resolve(new Response(null,{status:404}));
 return nativeAccountHttp(request);
}
