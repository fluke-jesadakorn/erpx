import 'server-only';
import {productHostForHost} from '@/lib/runtime/product-hosts';
import {currentSession} from '../../identity_access/public/authorization';
import {withCurrentPrivateSessionRead} from '../../identity_access/public/private-session-read';
import {holdingBindingBySlug,holdingResources} from '../../runtime/public/runtime';
import {PostgresDatabase} from '../../runtime/public/runtime/postgres';
import {ApiError} from '../../runtime/public/http';
import {legalPortalActor} from '../domain/portal';
import {portalCommand} from './legal/portal';

/** Accept, comment, envelope view and counsel submission retain the original
 * complete Legal command. Current Control authority is its final continuation. */
export async function legalPortalCommandRequest(holdingSlug:string,raw:unknown,key:string){
 const {session,requestHeaders}=await currentSession(),actor=legalPortalActor(session.user,session.session.id,(status,code,message)=>new ApiError(status,code,message));
 const image=()=>JSON.stringify({session,headers:[...requestHeaders.entries()]}),captured=image();
 return withCurrentPrivateSessionRead({userId:actor.userId,email:actor.email,sessionId:actor.sessionId,audience:productHostForHost(requestHeaders.get('host'))},async assertSession=>{
  const assertCurrent=async()=>{if(image()!==captured)throw new ApiError(401,'SESSION_REVOKED','The shared Legal command sign-in changed.');await assertSession();};
  const binding=await holdingBindingBySlug(holdingSlug),resources=await holdingResources(binding.id);
  if(!(resources.write instanceof PostgresDatabase))throw new ApiError(503,'LEGAL_PORTAL_TRANSACTION_REQUIRED','The shared Legal command requires its original Holding owner.');
  return portalCommand(resources.write,actor,raw,key,assertCurrent);
 });
}
