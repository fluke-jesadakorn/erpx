import {ssoAudienceForHost} from '@erpx/platform-runtime-ui/lib/product-hosts';
import 'server-only';
import {productHostForHost} from '@erpx/platform-runtime-ui/lib/product-hosts';
import {currentSession} from '@erpx/identity-access-server/public/authorization';
import {withCurrentPrivateSessionRead} from '@erpx/identity-access-server/public/private-session-read';
import {holdingBindingBySlug,holdingResources} from '../../../../../platform/runtime/server/src/public/runtime';
import {PostgresDatabase} from '@erpx/platform-runtime-server/public/runtime/postgres';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {legalPortalActor} from '@erpx/legal-server/domain/portal';
import {portalCommand} from '@erpx/legal-server/adapters/legal/portal';

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
