import 'server-only';
import {currentSession} from '../../identity_access/public/authorization';
import {withCurrentPrivateSessionRead} from '../../identity_access/public/private-session-read';
import {createCustomerPortalSessionApplication} from '../../customer_360/public/verified-portal-account';
import {holdingBindingBySlug,holdingResources} from '../../runtime/public/runtime';
import {ApiError} from '../../runtime/public/http';
import {productHostForHost} from '@/lib/runtime/product-hosts';
import {supportTicketPortalProjection,acknowledgeSupportTicketPortalResponse,supportTicketPortalPreview} from './ticket-portal-customer';
/** The original verified private account/session boundary precedes Holding
 * access. This request never calls the Sales customer document projection. */
async function request<Result>(holdingSlug:string,run:(context:{resources:Awaited<ReturnType<typeof holdingResources>>;actor:ReturnType<ReturnType<typeof createCustomerPortalSessionApplication>['actor']>;assertCurrent:()=>Promise<void>})=>Promise<Result>){const {session,requestHeaders}=await currentSession().catch(error=>{if(error instanceof Error&&error.message==='UNAUTHENTICATED')throw new ApiError(401,'UNAUTHENTICATED','Sign in to continue.');throw error;}),application=createCustomerPortalSessionApplication({error:(status,code,message)=>new ApiError(status,code,message)}),user=application.verify(session.user),snapshot=()=>JSON.stringify({session,headers:[...requestHeaders.entries()]}),captured=snapshot();return withCurrentPrivateSessionRead({userId:user.userId,email:user.email,sessionId:session.session.id,audience:productHostForHost(requestHeaders.get('host'))},async assertSession=>{const assertCurrent=async()=>{if(snapshot()!==captured)throw new ApiError(401,'SESSION_REVOKED','The verified customer sign-in changed.');await assertSession();},holding=await holdingBindingBySlug(holdingSlug),resources=await holdingResources(holding.id),actor=application.actor(user,holding);return run({resources,actor,assertCurrent});});}
export function supportTicketPortalProjectionRequest(holdingSlug:string){return request(holdingSlug,({resources,actor,assertCurrent})=>supportTicketPortalProjection(resources.write,actor,assertCurrent));}
export function supportTicketPortalCommandRequest(holdingSlug:string,bindingId:string,raw:unknown,key:string){return request(holdingSlug,({resources,actor,assertCurrent})=>acknowledgeSupportTicketPortalResponse(resources.write,actor,bindingId,raw,key,assertCurrent));}
export function supportTicketPortalPreviewRequest(holdingSlug:string,bindingId:string,linkId:string){return request(holdingSlug,({resources,actor,assertCurrent})=>supportTicketPortalPreview(resources.write,resources.files,actor,bindingId,linkId,assertCurrent));}
