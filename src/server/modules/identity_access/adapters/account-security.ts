import 'server-only';
import {cache} from 'react';
import {configuredProductOrigin,productHostForHost} from '@/lib/runtime/product-hosts';
import {accountSecurityInput} from '@/lib/identity_access/account-security';
import {runtimeEnv} from '../../runtime/public/runtime';
import {isTransactionalSqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError,errorResponse} from '../../runtime/public/http';
import {currentSession} from './authorization';
import {sessionFromRequest} from './auth';
import {accountSessionSecurity} from './account-session-security';
import {listEmergencyAccess} from './iam-recovery';
import {requestPublicOrigin} from '../../manage_erpx/public/public-origin';

async function service(actor:{userId:string;sessionId:string}){
 const owner=(await runtimeEnv()).CONTROL_DB;
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'ACCOUNT_TRANSACTION_REQUIRED','Account security requires its Control transaction.');
 return accountSessionSecurity(owner,actor);
}
export const currentAccountSecurity=cache(async()=>{
 const {session}=await currentSession(),actor={userId:session.user.id,sessionId:session.session.id};
 const [security,emergency]=await Promise.all([(await service(actor)).read(),listEmergencyAccess(actor)]);
 return {security,emergency};
});
/** Host-bound personal API. Inputs contain no selected role, user, Company,
 * session ID, token or credential; normal central authentication is required. */
export async function accountSecurityRequest(request:Request){
 const headers={'Cache-Control':'private, no-store','Referrer-Policy':'no-referrer','X-Content-Type-Options':'nosniff'};
 try{
  const origin=configuredProductOrigin('account');if(!origin||productHostForHost(request.headers.get('host'))!=='account')throw new ApiError(404,'NOT_FOUND','Not found.');
  if(requestPublicOrigin(request)!==origin)throw new ApiError(404,'NOT_FOUND','Not found.');
  if(!['GET','POST'].includes(request.method))return new Response('Method not allowed',{status:405,headers:{...headers,Allow:'GET, POST'}});
  if(request.method==='POST'&&request.headers.get('origin')!==origin)throw new ApiError(403,'ORIGIN_FORBIDDEN','Use ERPX Account to manage your sign-ins.');
  const session=await sessionFromRequest(request);if(!session)throw new ApiError(401,'SIGN_IN_REQUIRED','Sign in to manage your sessions.');
  const security=await service({userId:session.user.id,sessionId:session.session.id});
  if(request.method==='GET'){if(new URL(request.url).searchParams.size)throw new ApiError(422,'ACCOUNT_SCOPE_NOT_ALLOWED','Account security uses only your signed-in account.');return Response.json({data:await security.read()},{headers});}
  if(request.headers.get('content-type')?.split(';')[0].trim().toLowerCase()!=='application/json')throw new ApiError(415,'JSON_REQUIRED','Submit this security request as JSON.');
  const text=await request.text();if(new TextEncoder().encode(text).length>8192)throw new ApiError(413,'REQUEST_TOO_LARGE','This request is too large.');
  let raw:unknown;try{raw=JSON.parse(text);}catch{throw new ApiError(422,'INVALID_JSON','Submit a valid security request.');}
  const input=accountSecurityInput.parse(raw),key=request.headers.get('Idempotency-Key')??'';if(!/^[A-Za-z0-9:_-]{8,128}$/.test(key))throw new ApiError(422,'IDEMPOTENCY_KEY_REQUIRED','Keep the original security request key when retrying.');
  return Response.json({data:await security.revoke(input,key)},{headers});
 }catch(error){const response=errorResponse(error);for(const [key,value]of Object.entries(headers))response.headers.set(key,value);return response;}
}
