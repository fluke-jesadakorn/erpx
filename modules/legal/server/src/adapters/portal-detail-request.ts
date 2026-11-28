import {ssoAudienceForHost} from '@erpx/platform-runtime-ui/lib/product-hosts';
import 'server-only';
import {productHostForHost} from '@erpx/platform-runtime-ui/lib/product-hosts';
import {currentSession} from '@erpx/identity-access-server/public/authorization';
import {withCurrentPrivateSessionRead} from '@erpx/identity-access-server/public/private-session-read';
import {holdingBindingBySlug,holdingResources} from '../../../../../platform/runtime/server/src/public/runtime';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {legalPortalActor} from '@erpx/legal-server/domain/portal';
import {portalDetail} from '@erpx/legal-server/adapters/legal/portal';

/** Normal verified external identity only. The original detail reader owns its
 * source and view audit; this Control lease precedes that Holding snapshot and
 * rechecks inside its final continuation rather than after the audit commits. */
export async function legalPortalDetailRequest(holdingSlug:string,shareId:string){
 const {session,requestHeaders}=await currentSession(),actor=legalPortalActor(session.user,session.session.id,(status,code,message)=>new ApiError(status,code,message));
 const image=()=>JSON.stringify({session,headers:[...requestHeaders.entries()]}),captured=image();
 return withCurrentPrivateSessionRead({userId:actor.userId,email:actor.email,sessionId:actor.sessionId,audience:ssoAudienceForHost(requestHeaders.get('host'))},async assertSession=>{
  const assertCurrent=async()=>{if(image()!==captured)throw new ApiError(401,'SESSION_REVOKED','The shared Legal detail sign-in changed.');await assertSession();};
  const holding=await holdingBindingBySlug(holdingSlug),resources=await holdingResources(holding.id);
  return portalDetail(resources.write,actor,shareId,assertCurrent);
 });
}
