import 'server-only';
import {createHash} from 'node:crypto';
import {productHostForHost} from '@/lib/runtime/product-hosts';
import {currentSession} from '../../identity_access/public/authorization';
import {withCurrentPrivateSessionRead} from '../../identity_access/public/private-session-read';
import {holdingBindingBySlug,holdingResources} from '../../runtime/public/runtime';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {readQuickAccessPreferences} from '../../work_projection/public/quick-access-preferences';
import {legalPortalActor,type PortalActor} from '../domain/portal';
import {portalSnapshot} from './legal/portal';

/** Server-only presentation data. Control identity and preference reads precede
 * the complete Holding list/detail unit; its final assertion precedes audits. */
async function withPortalSession<Result>(holdingSlug:string,read:(input:{database:SqlDatabase;actor:PortalActor;holdingId:string;assertCurrent:()=>Promise<void>})=>Promise<Result>){
 const {session,requestHeaders}=await currentSession(),actor=legalPortalActor(session.user,session.session.id,(status,code,message)=>new ApiError(status,code,message));
 const image=()=>JSON.stringify({session,headers:[...requestHeaders.entries()]}),captured=image();
 return withCurrentPrivateSessionRead({userId:actor.userId,email:actor.email,sessionId:actor.sessionId,audience:productHostForHost(requestHeaders.get('host'))},async assertSession=>{
  const assertCurrent=async()=>{if(image()!==captured)throw new ApiError(401,'SESSION_REVOKED','The shared Legal sign-in changed.');await assertSession();};
  const binding=await holdingBindingBySlug(holdingSlug),resources=await holdingResources(binding.id);
  return read({database:resources.write,actor,holdingId:binding.id,assertCurrent});
 });
}
export async function legalPortalPresentationSnapshot(holdingSlug:string,selectedShareId?:string){return withPortalSession(holdingSlug,async({database,actor,holdingId,assertCurrent})=>{
 const preferences=await readQuickAccessPreferences(actor.userId,'legal_portal',holdingId),snapshot=await portalSnapshot(database,actor,selectedShareId,assertCurrent);
 return {...snapshot,preferences,commandScope:createHash('sha256').update(JSON.stringify([holdingId,actor.userId,actor.sessionId])).digest('hex')};
});}
export async function legalPortalListRequest(holdingSlug:string){return withPortalSession(holdingSlug,async({database,actor,assertCurrent})=>(await portalSnapshot(database,actor,undefined,assertCurrent)).initial);}
