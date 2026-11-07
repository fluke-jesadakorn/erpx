import 'server-only';
import {accountSecuritySeedSchema,type AccountSecuritySeed} from '@erpx/feature-identity_access/lib/account-security';
import {ApiError} from '@erpx/server-runtime/public/http';
import {nativeAccountHttp} from './native-account-http';

type Send=(request:Request)=>Promise<Response>;
type IncomingHeaders=Pick<Headers,'get'>;
const MAXIMUM_BODY_BYTES=64*1024;

function invalid(message='The Account security response did not match its contract.'):never{
 throw new ApiError(503,'NATIVE_ACCOUNT_SECURITY_RESPONSE_INVALID',message);
}
function failure(status:number,body:unknown):never{
 const record=body&&typeof body==='object'&&!Array.isArray(body)?body as Record<string,unknown>:undefined;
 const error=record?.error&&typeof record.error==='object'&&!Array.isArray(record.error)?record.error as Record<string,unknown>:undefined;
 const code=typeof error?.code==='string'&&/^[A-Z0-9_]{3,80}$/.test(error.code)?error.code:'NATIVE_ACCOUNT_SECURITY_UNAVAILABLE';
 const message=typeof error?.message==='string'&&error.message.length<=1000?error.message:'The Account security request could not be completed.';
 throw new ApiError(status,code,message);
}

/** Read-only native projection for the Account security page. A missing,
 * malformed or failed native runtime is an explicit error; there is no retained
 * database projection or per-request fallback. */
export function createNativeAccountSecurityReader(send:Send=nativeAccountHttp){
 return async(incoming:IncomingHeaders):Promise<AccountSecuritySeed>=>{
  const host=incoming.get('host');
  if(!host)throw new ApiError(503,'NATIVE_ACCOUNT_SECURITY_AUTHORITY_REQUIRED','The native Account authority is not configured.');
  const cookie=incoming.get('cookie');
  const headers=new Headers({host,accept:'application/json','cache-control':'no-cache'});
  if(cookie)headers.set('cookie',cookie);
  const response=await send(new Request(`https://${host}/api/v1/account/security`,{method:'GET',headers,cache:'no-store',redirect:'error'}));
  const text=await response.text();
  if(new TextEncoder().encode(text).length>MAXIMUM_BODY_BYTES)return invalid();
  let body:unknown;
  try{body=JSON.parse(text);}catch{return invalid();}
  if(!response.ok)return failure(response.status,body);
  const data=body&&typeof body==='object'&&!Array.isArray(body)?(body as Record<string,unknown>).data:undefined;
  const parsed=accountSecuritySeedSchema.safeParse(data);
  if(!parsed.success)return invalid();
  return parsed.data;
 };
}
export const nativeAccountSecurityRead=createNativeAccountSecurityReader();
