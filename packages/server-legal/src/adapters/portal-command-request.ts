import {ssoAudienceForHost} from '@erpx/feature-runtime/lib/product-hosts';
import 'server-only';
import {productHostForHost} from '@erpx/feature-runtime/lib/product-hosts';
import {currentSession} from '@erpx/server-identity_access/public/authorization';
import {withCurrentPrivateSessionRead} from '@erpx/server-identity_access/public/private-session-read';
import {holdingBindingBySlug,holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import {ApiError} from '@erpx/server-runtime/public/http';
import {legalPortalActor} from '@erpx/server-legal/domain/portal';
import {portalCommand} from '@erpx/server-legal/adapters/legal/portal';

/** Accept, comment, envelope view and counsel submission retain the original
 * complete Legal command. Current Control authority is its final continuation. */
export async function legalPortalCommandRequest(holdingSlug:string,raw:unknown,key:string){
 const {session,requestHeaders}=await currentSession(),actor=legalPortalActor(session.user,session.session.id,(status,code,message)=>new ApiError(status,code,message));
 const image=()=>JSON.stringify({session,headers:[...requestHeaders.entries()]}),captured=image();
 return withCurrentPrivateSessionRead({userId:actor.userId,email:actor.email,sessionId:actor.sessionId,audience:ssoAudienceForHost(requestHeaders.get('host'))},async assertSession=>{
  const assertCurrent=async()=>{if(image()!==captured)throw new ApiError(401,'SESSION_REVOKED','The shared Legal command sign-in changed.');await assertSession();};
  const binding=await holdingBindingBySlug(holdingSlug),resources=await holdingResources(binding.id);
  if(!(resources.write instanceof PostgresDatabase))throw new ApiError(503,'LEGAL_PORTAL_TRANSACTION_REQUIRED','The shared Legal command requires its original Holding owner.');
  return portalCommand(resources.write,actor,raw,key,assertCurrent);
 });
}
