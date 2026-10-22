import 'server-only';
import {productHostForHost} from '@erpx/feature-runtime/lib/product-hosts';
import {currentSession} from '@erpx/server-identity_access/public/authorization';
import {withCurrentPrivateSessionRead} from '@erpx/server-identity_access/public/private-session-read';
import {holdingBindingBySlug,holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {ApiError} from '@erpx/server-runtime/public/http';
import {legalPortalActor} from '@erpx/server-legal/domain/portal';
import {portalDetail} from '@erpx/server-legal/adapters/legal/portal';

/** Normal verified external identity only. The original detail reader owns its
 * source and view audit; this Control lease precedes that Holding snapshot and
 * rechecks inside its final continuation rather than after the audit commits. */
export async function legalPortalDetailRequest(holdingSlug:string,shareId:string){
 const {session,requestHeaders}=await currentSession(),actor=legalPortalActor(session.user,session.session.id,(status,code,message)=>new ApiError(status,code,message));
 const image=()=>JSON.stringify({session,headers:[...requestHeaders.entries()]}),captured=image();
 return withCurrentPrivateSessionRead({userId:actor.userId,email:actor.email,sessionId:actor.sessionId,audience:productHostForHost(requestHeaders.get('host'))},async assertSession=>{
  const assertCurrent=async()=>{if(image()!==captured)throw new ApiError(401,'SESSION_REVOKED','The shared Legal detail sign-in changed.');await assertSession();};
  const holding=await holdingBindingBySlug(holdingSlug),resources=await holdingResources(holding.id);
  return portalDetail(resources.write,actor,shareId,assertCurrent);
 });
}
