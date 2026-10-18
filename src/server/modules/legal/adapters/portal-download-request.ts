import 'server-only';
import {productHostForHost} from '@/lib/runtime/product-hosts';
import {currentSession} from '../../identity_access/public/authorization';
import {withCurrentPrivateSessionRead} from '../../identity_access/public/private-session-read';
import {holdingBindingBySlug,holdingResources} from '../../runtime/public/runtime';
import {ApiError} from '../../runtime/public/http';
import {legalPortalActor} from '../domain/portal';
import {portalDownload} from './legal/portal';

/** Only normal host-specific authentication constructs this request. Control
 * authority is leased before the fresh Holding read; its final assertion runs
 * inside that read after buffered bytes and deferred effects, before audits
 * commit. This is ordered access, not a cross-database atomic transaction. */
export async function legalPortalDownloadRequest(holdingSlug:string,shareId:string){
 const {session,requestHeaders}=await currentSession(),actor=legalPortalActor(session.user,session.session.id,(status,code,message)=>new ApiError(status,code,message));
 const image=()=>JSON.stringify({session,headers:[...requestHeaders.entries()]}),captured=image();
 return withCurrentPrivateSessionRead({userId:actor.userId,email:actor.email,sessionId:actor.sessionId,audience:productHostForHost(requestHeaders.get('host'))},async assertSession=>{
  const assertCurrent=async()=>{if(image()!==captured)throw new ApiError(401,'SESSION_REVOKED','The shared Legal document sign-in changed.');await assertSession();};
  const holding=await holdingBindingBySlug(holdingSlug),resources=await holdingResources(holding.id);
  return portalDownload(resources.write,resources.files,actor,shareId,assertCurrent);
 });
}
