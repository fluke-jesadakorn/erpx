import 'server-only';
import {headers} from 'next/headers';
import {NextResponse} from 'next/server';
import {z} from 'zod';
import {configuredProductOrigin,productHostForHost} from '@/lib/runtime/product-hosts';
import {requestPublicOrigin} from '../../manage_erpx/public/public-origin';
import {runtimeEnv} from '../../runtime/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError,errorResponse} from '../../runtime/public/http';
import {sessionFromRequest} from './auth';
import {currentSession} from './authorization';
import {cookieValue} from '../domain/authorization';
import {providerContextStore} from './provider-context-store';
import type {ProviderContextSeed} from '@/lib/identity_access/provider-context';

const responseHeaders={'Cache-Control':'private, no-store','Referrer-Policy':'no-referrer','X-Content-Type-Options':'nosniff'};
const selection=z.object({accountKey:z.string().regex(/^[a-f0-9]{64}$/),expectedContext:z.string().regex(/^[a-f0-9]{64}$/),expectedRevision:z.number().int().nonnegative().safe(),assignmentId:z.string().min(1).max(128),choiceVersion:z.string().regex(/^[a-f0-9]{64}$/)}).strict();
function requireAdmin(requestHeaders:Headers){if(!configuredProductOrigin('admin')||productHostForHost(requestHeaders.get('host'))!=='admin')throw new ApiError(404,'NOT_FOUND','Not found.');}
function owned(database:SqlDatabase){if(!isTransactionalSqlDatabase(database))throw new ApiError(503,'PROVIDER_CONTEXT_TRANSACTION_REQUIRED','Platform selection requires its complete fresh Control transaction.');return database;}
function readyError(error:unknown){let cause=error;for(let depth=0;depth<5&&cause&&typeof cause==='object';depth++){const item=cause as {code?:string;cause?:unknown};if(item.code==='42P01')return new ApiError(503,'PROVIDER_CONTEXT_NOT_READY','Administration context selection is not ready.');cause=item.cause;}return error;}
/** Minimal Server seed; no actor, modules, policy, session or grant snapshot. */
export async function currentProviderContext():Promise<ProviderContextSeed>{
 const requestHeaders=await headers();requireAdmin(requestHeaders);const {session}=await currentSession(),owner=(await runtimeEnv()).CONTROL_DB;
 try{return await providerContextStore(owned(owner),{userId:session.user.id,sessionId:session.session.id,selectedCookie:cookieValue(requestHeaders.get('cookie'),'erpx_assignment_platform')}).read();}catch(error){throw readyError(error);}
}
export async function providerContextRequest(request:Request){
 try{
  requireAdmin(request.headers);const origin=configuredProductOrigin('admin')!;
  if(requestPublicOrigin(request)!==origin)throw new ApiError(404,'NOT_FOUND','Not found.');
  if(!['GET','POST'].includes(request.method))return new Response('Method not allowed',{status:405,headers:{...responseHeaders,Allow:'GET, POST'}});
  if(request.method==='POST'&&request.headers.get('origin')!==origin)throw new ApiError(403,'ORIGIN_FORBIDDEN','Choose a Platform role from the current Administration application.');
  const session=await sessionFromRequest(request);if(!session)throw new ApiError(401,'SIGN_IN_REQUIRED','Sign in to Administration.');
  const store=providerContextStore(owned((await runtimeEnv()).CONTROL_DB),{userId:session.user.id,sessionId:session.session.id,selectedCookie:cookieValue(request.headers.get('cookie'),'erpx_assignment_platform')});
  if(request.method==='GET'){
   const seed=await store.read(),accountKey=new URL(request.url).searchParams.get('accountKey');if(accountKey!==null&&accountKey!==seed.accountKey)throw new ApiError(409,'PROVIDER_CONTEXT_CHANGED','Your account changed. Reload Administration.');return NextResponse.json({data:seed},{headers:responseHeaders});
  }
  if(request.headers.get('content-type')?.split(';')[0].trim().toLowerCase()!=='application/json')throw new ApiError(415,'JSON_REQUIRED','Submit a Platform selection as JSON.');
  const key=request.headers.get('Idempotency-Key');if(!key||!/^[A-Za-z0-9_.:-]{8,128}$/.test(key))throw new ApiError(422,'IDEMPOTENCY_KEY_REQUIRED','Use a captured request key for this selection.');
  const body=await request.text();if(new TextEncoder().encode(body).length>4096)throw new ApiError(413,'REQUEST_TOO_LARGE','This request is too large.');let raw:unknown;try{raw=JSON.parse(body);}catch{throw new ApiError(422,'INVALID_JSON','Submit a valid Platform selection.');}
  const parsed=selection.safeParse(raw);if(!parsed.success)throw new ApiError(422,'VALIDATION_FAILED','Choose the current assigned Platform role.');
  const seed=await store.select(parsed.data,key),response=NextResponse.json({data:seed},{headers:responseHeaders});response.cookies.set('erpx_assignment_platform',seed.selectedId,{httpOnly:true,sameSite:'lax',secure:true,path:'/',maxAge:12*60*60,priority:'high'});return response;
 }catch(error){const response=errorResponse(readyError(error));for(const [key,value]of Object.entries(responseHeaders))response.headers.set(key,value);return response;}
}
