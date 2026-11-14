import {ssoAudienceForHost} from '@erpx/feature-runtime/lib/product-hosts';
import 'server-only';
import {productHostForHost} from '@erpx/feature-runtime/lib/product-hosts';
import {currentSession} from '@erpx/server-identity_access/public/authorization';
import {withCurrentPrivateSessionRead} from '@erpx/server-identity_access/public/private-session-read';
import {holdingBindingBySlug,holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {ApiError} from '@erpx/server-runtime/public/http';
import {legalPortalActor} from '@erpx/server-legal/domain/portal';
import {portalDownload} from '@erpx/server-legal/adapters/legal/portal';

/** Only normal host-specific authentication constructs this request. Control
 * authority is leased before the fresh Holding read; its final assertion runs
 * inside that read after buffered bytes and deferred effects, before audits
 * commit. This is ordered access, not a cross-database atomic transaction. */
export async function legalPortalDownloadRequest(holdingSlug:string,shareId:string){
 const {session,requestHeaders}=await currentSession(),actor=legalPortalActor(session.user,session.session.id,(status,code,message)=>new ApiError(status,code,message));
 const image=()=>JSON.stringify({session,headers:[...requestHeaders.entries()]}),captured=image();
 return withCurrentPrivateSessionRead({userId:actor.userId,email:actor.email,sessionId:actor.sessionId,audience:ssoAudienceForHost(requestHeaders.get('host'))},async assertSession=>{
  const assertCurrent=async()=>{if(image()!==captured)throw new ApiError(401,'SESSION_REVOKED','The shared Legal document sign-in changed.');await assertSession();};
  const holding=await holdingBindingBySlug(holdingSlug),resources=await holdingResources(holding.id);
  return portalDownload(resources.write,resources.files,actor,shareId,assertCurrent);
 });
}
