import {ssoAudienceForHost} from '@erpx/platform-runtime-ui/lib/product-hosts';
import 'server-only';
import {currentSession} from '@erpx/identity-access-server/public/authorization';
import {withCurrentPrivateSessionRead} from '@erpx/identity-access-server/public/private-session-read';
import {holdingBindingBySlug,holdingResources} from '../../../../../platform/runtime/server/src/public/runtime';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {productHostForHost} from '@erpx/platform-runtime-ui/lib/product-hosts';
import {createCustomerPortalSessionApplication} from '@erpx/customer-360-server/application/portal-session';
import {customerPortalRequestProjection} from '@erpx/customer-360-server/adapters/portal';

/** Expected presentation identity only restricts the original request; it is
 * never a caller-selected account or a replacement for normal authentication. */
export async function customerPortalProjectionRequest(holdingSlug:string,expectedActor?:{userId:string;email:string}){
 const {session,requestHeaders}=await currentSession(),application=createCustomerPortalSessionApplication({error:(status,code,message)=>new ApiError(status,code,message)}),user=application.verify(session.user),image=()=>JSON.stringify({session,headers:[...requestHeaders.entries()]}),captured=image();
 if(expectedActor&&(expectedActor.userId!==user.userId||expectedActor.email.toLowerCase()!==user.email))throw new ApiError(401,'SESSION_REVOKED','The verified customer account changed.');
 return withCurrentPrivateSessionRead({userId:user.userId,email:user.email,sessionId:session.session.id,audience:ssoAudienceForHost(requestHeaders.get('host'))},async assertSession=>{
  const assertCurrent=async()=>{if(image()!==captured)throw new ApiError(401,'SESSION_REVOKED','The verified customer sign-in changed.');await assertSession();},holding=await holdingBindingBySlug(holdingSlug),resources=await holdingResources(holding.id),actor=application.actor(user,holding);
  return customerPortalRequestProjection(resources.write,actor,assertCurrent);
 });
}
