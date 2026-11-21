import {ssoAudienceForHost} from '@erpx/feature-runtime/lib/product-hosts';
import "server-only";
import {currentSession} from "@erpx/server-identity_access/public/authorization";
import {withCurrentPrivateSessionRead} from "@erpx/server-identity_access/public/private-session-read";
import {ApiError} from "@erpx/server-runtime/public/http";
import {holdingBindingBySlug,holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {productHostForHost} from "@erpx/feature-runtime/lib/product-hosts";
import {createCustomerPortalSessionApplication} from "@erpx/server-customer_360/application/portal-session";
import {customerPortalSharedPreview} from "@erpx/server-customer_360/adapters/portal-attachments";

/** Only the actual normal request session can construct this external preview.
 * No caller-supplied customer identity or session metadata is accepted. */
export async function customerPortalSharedPreviewRequest(holdingSlug:string,linkId:string){
 const {session,requestHeaders}=await currentSession(),application=createCustomerPortalSessionApplication({error:(status,code,message)=>new ApiError(status,code,message)}),user=application.verify(session.user);
 const image=()=>JSON.stringify({session,headers:[...requestHeaders.entries()]}),captured=image();
 return withCurrentPrivateSessionRead({userId:user.userId,email:user.email,sessionId:session.session.id,audience:ssoAudienceForHost(requestHeaders.get("host"))},async assertSession=>{
  const assertCurrent=async()=>{if(image()!==captured)throw new ApiError(401,"SESSION_REVOKED","The private document sign-in changed.");await assertSession();};
  const holding=await holdingBindingBySlug(holdingSlug),resources=await holdingResources(holding.id),actor=application.actor(user,holding);
  return customerPortalSharedPreview(resources.write,resources.files,actor,linkId,assertCurrent);
 });
}
