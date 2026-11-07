import 'server-only';
import {cache} from 'react';
import {headers as requestHeaders} from 'next/headers';
import {currentSession} from '@erpx/server-identity_access/adapters/authorization';
import {listEmergencyAccess} from '@erpx/server-identity_access/adapters/iam-recovery';
import {nativeAccountSecurityRead} from '@erpx/server-identity_access/adapters/native-account-security';

/** Native Account security is the only source for the signed-in security seed.
 * A missing or failed native runtime surfaces as an error; there is no retained
 * database projection or per-request fallback. Emergency access remains its
 * own retained Identity-owned read until that workflow is migrated. */
export const currentAccountSecurity=cache(async()=>{
 const {session}=await currentSession(),actor={userId:session.user.id,sessionId:session.session.id};
 const [security,emergency]=await Promise.all([
  nativeAccountSecurityRead(await requestHeaders()),
  listEmergencyAccess(actor),
 ]);
 return {security,emergency};
});
