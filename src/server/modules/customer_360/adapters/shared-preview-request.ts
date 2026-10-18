import "server-only";
import {currentSession} from "../../identity_access/public/authorization";
import {withCurrentPrivateSessionRead} from "../../identity_access/public/private-session-read";
import {ApiError} from "../../runtime/public/http";
import {holdingBindingBySlug,holdingResources} from "../../runtime/public/runtime";
import {productHostForHost} from "@/lib/runtime/product-hosts";
import {createCustomerPortalSessionApplication} from "../application/portal-session";
import {customerPortalSharedPreview} from "./portal-attachments";

/** Only the actual normal request session can construct this external preview.
 * No caller-supplied customer identity or session metadata is accepted. */
export async function customerPortalSharedPreviewRequest(holdingSlug:string,linkId:string){
 const {session,requestHeaders}=await currentSession(),application=createCustomerPortalSessionApplication({error:(status,code,message)=>new ApiError(status,code,message)}),user=application.verify(session.user);
 const image=()=>JSON.stringify({session,headers:[...requestHeaders.entries()]}),captured=image();
 return withCurrentPrivateSessionRead({userId:user.userId,email:user.email,sessionId:session.session.id,audience:productHostForHost(requestHeaders.get("host"))},async assertSession=>{
  const assertCurrent=async()=>{if(image()!==captured)throw new ApiError(401,"SESSION_REVOKED","The private document sign-in changed.");await assertSession();};
  const holding=await holdingBindingBySlug(holdingSlug),resources=await holdingResources(holding.id),actor=application.actor(user,holding);
  return customerPortalSharedPreview(resources.write,resources.files,actor,linkId,assertCurrent);
 });
}
