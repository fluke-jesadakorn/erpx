import 'server-only';
import {currentSession} from '../../identity_access/public/authorization';
import {withCurrentPrivateSessionPreferences} from '../../identity_access/public/private-session-preferences';
import {readSessionQuickAccessPreferences,saveSessionQuickAccessPreferences} from '../../work_projection/public/quick-access-session-preferences';
import {legalPortalQuickAccess} from '../../work_projection/public/portal-quick-access';
import {holdingBindingBySlug} from '../../runtime/public/runtime';
import {ApiError} from '../../runtime/public/http';
import {productHostForHost} from '@/lib/runtime/product-hosts';
import {legalPortalActor} from '../domain/portal';
import {legalPortalListRequest} from './portal-snapshot-request';

type Change={pinnedIds:string[]|null;expectedVersion:number;requestKey:string};
/** Available document IDs come from an earlier authorized Holding read. Only
 * personal preference state commits in Control; pins never authorize a share. */
export async function legalPortalQuickAccessRequest(holdingSlug:string,change?:Change){
 const input=change?{pinnedIds:change.pinnedIds===null?null:[...change.pinnedIds],expectedVersion:change.expectedVersion,requestKey:change.requestKey}:undefined;
 const {session,requestHeaders}=await currentSession(),actor=legalPortalActor(session.user,session.session.id,(status,code,message)=>new ApiError(status,code,message)),binding=await holdingBindingBySlug(holdingSlug),captured=JSON.stringify({session,headers:[...requestHeaders.entries()]}),availableIds=input?legalPortalQuickAccess(holdingSlug,await legalPortalListRequest(holdingSlug)).map(item=>item.id):[];
 const assertContext=()=>{if(JSON.stringify({session,headers:[...requestHeaders.entries()]})!==captured)throw new ApiError(401,'SESSION_REVOKED','The verified shared Legal account changed.');};
 return withCurrentPrivateSessionPreferences({userId:actor.userId,email:actor.email,sessionId:actor.sessionId,audience:productHostForHost(requestHeaders.get('host'))},async database=>{
  assertContext();
  return input?saveSessionQuickAccessPreferences(database,{...input,userId:actor.userId,surface:'legal_portal',contextKey:binding.id,availableIds}):readSessionQuickAccessPreferences(database,actor.userId,'legal_portal',binding.id);
 },assertContext);
}
