import {nativeAccountHttp} from '@erpx/server-identity_access/public/native-account-http';
import {createAuth} from '@erpx/server-identity_access/public/auth';
export const dynamic='force-dynamic';
// Rust owns endpoint admission, sessions, cookies, redirects and all writes.
// These two declared installed consumer endpoints remain on the original
// Better Auth handler until their native commands are migrated; a copied or
// native-only cookie therefore stays unauthenticated instead of being upgraded.
const legacy=new Set(['/api/auth/list-sessions','/api/auth/update-user']);
async function handler(request:Request){
 const url=new URL(request.url);
 if(!legacy.has(url.pathname))return nativeAccountHttp(request);
 const cookie=request.headers.get('cookie')??'';
 if(!/(?:^|;\s*)(?:(?:__Secure-)?better-auth|erpx-[a-z0-9_-]+)\.session_token/.test(cookie))
  return Response.json({error:{code:'UNAUTHORIZED',message:'Sign in to continue.'}},{status:401,headers:{'cache-control':'private, no-store'}});
 const origin=request.headers.get('origin')??`https://${request.headers.get('host')??url.host}`;
 return (await createAuth(origin)).handler(request);
}
export const GET=handler;
export const POST=handler;
