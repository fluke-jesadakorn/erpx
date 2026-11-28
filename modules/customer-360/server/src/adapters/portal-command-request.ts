import {ssoAudienceForHost} from '@erpx/platform-runtime-ui/lib/product-hosts';
import 'server-only';
import {currentSession} from '@erpx/identity-access-server/public/authorization';
import {withCurrentPrivateSessionRead} from '@erpx/identity-access-server/public/private-session-read';
import {nativeAcknowledgePortalSupportResponse} from '@erpx/customer-authority-server/public/native-portal-support';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {holdingBindingBySlug,holdingResources} from '../../../../../platform/runtime/server/src/public/runtime';
import type {PostgresDatabase} from '@erpx/platform-runtime-server/public/runtime/postgres';
import {createCustomerPortalSessionApplication} from '@erpx/customer-360-server/application/portal-session';
import {customerPortalCommand,customerPortalProjection} from '@erpx/customer-360-server/adapters/portal';

/** Only normal request authentication can construct this Control-before-Holding
 * composition. Each original command owns its transaction and repeats current
 * request authority after all deferred effects, before that Holding commits. */
export async function customerPortalCommandRequest(holdingSlug:string,raw:unknown,key:string){
 const {session,requestHeaders}=await currentSession(),application=createCustomerPortalSessionApplication({error:(status,code,message)=>new ApiError(status,code,message)}),user=application.verify(session.user),image=()=>JSON.stringify({session,headers:[...requestHeaders.entries()]}),captured=image();
 return withCurrentPrivateSessionRead({userId:user.userId,email:user.email,sessionId:session.session.id,audience:ssoAudienceForHost(requestHeaders.get('host'))},async assertSession=>{
  const assertCurrent=async()=>{if(image()!==captured)throw new ApiError(401,'SESSION_REVOKED','The verified customer sign-in changed.');await assertSession();};
  const holding=await holdingBindingBySlug(holdingSlug),resources=await holdingResources(holding.id),actor=application.actor(user,holding);
  await customerPortalProjection(resources.write,actor);
  await assertCurrent();
  const requestId=raw&&typeof raw==='object'&&'action' in raw&&raw.action==='acknowledge_support'&&'requestId' in raw&&typeof raw.requestId==='string'?raw.requestId:null;
  return requestId?nativeAcknowledgePortalSupportResponse(actor,requestId,raw,key,requestHeaders):customerPortalCommand(resources.write as PostgresDatabase,actor,raw,key,assertCurrent);
 });
}
