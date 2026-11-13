import 'server-only';
import {cache} from 'react';
import {headers as requestHeaders} from 'next/headers';
import {currentSession} from '@erpx/server-identity_access/adapters/authorization';
import {nativeAccountSecurityRead} from '@erpx/server-identity_access/adapters/native-account-security';
import {nativeEmergencyAccess} from '@erpx/server-identity_access/adapters/native-recovery';

/** Native Account security and native temporary access are the only sources
 * for the signed-in security seed. A missing or failed native runtime surfaces
 * as an error; there is no retained database projection or fallback. */
export const currentAccountSecurity=cache(async()=>{
 await currentSession();
 const headers=await requestHeaders();
 const forwarded=new Headers();
 for(const name of ['host','cookie','origin']){
  const value=headers.get(name);
  if(value)forwarded.set(name,value);
 }
 const emergencyRequest=new Request('https://account.invalid/api/v1/account/iam/emergency/active',{headers:forwarded});
 const [security,emergency]=await Promise.all([
  nativeAccountSecurityRead(headers),
  nativeEmergencyAccess.active(emergencyRequest),
 ]);
 return {security,emergency};
});
