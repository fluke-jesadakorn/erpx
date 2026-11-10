import 'server-only';
import {ApiError} from '@erpx/server-runtime/public/http';
import { createNativeSessionClient, NativeSessionReadError, type NativeSessionClientConfiguration, type NativeSessionRead } from '@erpx/native-sdk/session';
import type { IdentitySession } from '@erpx/server-identity_access/domain/authorization';

export function nativeAccountConfiguration(raw: string): NativeSessionClientConfiguration {
  let value: unknown;
  try { value = JSON.parse(raw); } catch { throw new NativeSessionReadError('NATIVE_SESSION_CONFIGURATION_INVALID'); }
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new NativeSessionReadError('NATIVE_SESSION_CONFIGURATION_INVALID');
  const record = value as Record<string, unknown>;
  if (typeof record.privateBaseUrl !== 'string' || typeof record.accountOrigin !== 'string' || typeof record.timeoutMs !== 'number' ||
      !Array.isArray(record.holdingOrigins) || !record.holdingOrigins.every((origin: unknown) => typeof origin === 'string')) {
    throw new NativeSessionReadError('NATIVE_SESSION_CONFIGURATION_INVALID');
  }
  const loopback = process.env.NODE_ENV === 'development' && (process.env.NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE === 'loopback' || process.env.ERPX_NATIVE_ORIGIN_PROFILE === 'loopback');
  const profile = loopback ? 'loopback' : process.env.NODE_ENV === 'development' && process.env.NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE === 'container-localhost' ? 'contributor' : 'public';
  return {...record,originProfile:profile} as NativeSessionClientConfiguration;
}

type NativeIdentitySession = IdentitySession & {
  session: { userId: string; createdAt: Date; updatedAt: Date };
  user: { emailVerified: boolean; image: string | null };
};

/** Native session identity only. Grants and selected Work remain separate
 * current Rust checks; an absent native runtime is explicitly unavailable. */
export function createIdentitySessionAuthenticator(
  client:(configuration:NativeSessionClientConfiguration)=>NativeSessionRead=createNativeSessionClient,
){
 return async(origin:string|undefined,headers:Headers):Promise<NativeIdentitySession|null>=>{
  const raw=process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG;
  if(raw===undefined)throw new ApiError(503,'NATIVE_SESSION_CONFIGURATION_REQUIRED','The native Account session is not configured.');
  // Declared legacy hosts still have installed Better Auth cookies until their
  // consumers move. Only the real legacy cookie name is accepted here; native
  // and copied native cookies always take the native path.
  const legacy=/(?:^|;\s*)(?:(?:__Secure-)?better-auth|erpx-[a-z0-9_-]+)\.session_token/.test(headers.get('cookie')??'');
  const allowed=[process.env.BETTER_AUTH_URL,process.env.ERPX_PUBLIC_ORIGIN,process.env.ERPX_FRONT_POS_ORIGIN,process.env.ERPX_FRONT_PMS_ORIGIN].filter((value):value is string=>!!value);
  if(legacy&&origin&&allowed.includes(origin)){
   const {createAuth}=await import('./auth');
   const session=await (await createAuth(origin)).api.getSession({headers});
   if(!session)return null;
   return {session:{id:session.session.id,userId:session.user.id,expiresAt:session.session.expiresAt,createdAt:session.session.createdAt,updatedAt:session.session.updatedAt},user:{id:session.user.id,name:session.user.name,email:session.user.email,emailVerified:Boolean(session.user.emailVerified),image:session.user.image??null}};
  }
  const result=await client(nativeAccountConfiguration(raw))(headers);
  if(result===null)return null;
  return {
   session:{id:result.session.id,userId:result.session.userId,expiresAt:new Date(result.session.expiresAt),createdAt:new Date(result.session.createdAt),updatedAt:new Date(result.session.updatedAt)},
   user:{id:result.user.id,name:result.user.name,email:result.user.email,emailVerified:result.user.emailVerified,image:result.user.image},
  };
 };
}

export function sessionFromRequest(request:Request){return createIdentitySessionAuthenticator()(undefined,request.headers);}
