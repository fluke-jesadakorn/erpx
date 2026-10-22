import 'server-only';
import {currentSession} from '@erpx/server-identity_access/public/authorization';
import {withCurrentPrivateSessionPreferences} from '@erpx/server-identity_access/public/private-session-preferences';
import {readSessionQuickAccessPreferences,saveSessionQuickAccessPreferences} from '@erpx/server-work_projection/public/quick-access-session-preferences';
import {customerPortalQuickAccess} from '@erpx/server-work_projection/public/portal-quick-access';
import {holdingBindingBySlug} from '../../../../packages/server-runtime/src/public/runtime';
import {ApiError} from '@erpx/server-runtime/public/http';
import {productHostForHost} from '@erpx/feature-runtime/lib/product-hosts';
import {createCustomerPortalSessionApplication} from '@erpx/server-customer_360/application/portal-session';
import {customerPortalProjectionRequest} from '@erpx/server-customer_360/adapters/portal-projection-request';

type Change={pinnedIds:string[]|null;expectedVersion:number;requestKey:string};
export async function customerPortalQuickAccessRequest(holdingSlug:string,change?:Change){
 const input=change?{pinnedIds:change.pinnedIds===null?null:[...change.pinnedIds],expectedVersion:change.expectedVersion,requestKey:change.requestKey}:undefined;
 const {session,requestHeaders}=await currentSession(),application=createCustomerPortalSessionApplication({error:(status,code,message)=>new ApiError(status,code,message)}),user=application.verify(session.user),binding=await holdingBindingBySlug(holdingSlug),captured=JSON.stringify({session,headers:[...requestHeaders.entries()]}),availableIds=input?customerPortalQuickAccess(holdingSlug,(await customerPortalProjectionRequest(holdingSlug,user)).items).map(item=>item.id):[];
 return withCurrentPrivateSessionPreferences({userId:user.userId,email:user.email,sessionId:session.session.id,audience:productHostForHost(requestHeaders.get('host'))},async database=>{
  if(JSON.stringify({session,headers:[...requestHeaders.entries()]})!==captured)throw new ApiError(401,'SESSION_REVOKED','The verified customer account changed.');
  return input?saveSessionQuickAccessPreferences(database,{...input,userId:user.userId,surface:'customer_portal',contextKey:binding.id,availableIds}):readSessionQuickAccessPreferences(database,user.userId,'customer_portal',binding.id);
 },()=>{if(JSON.stringify({session,headers:[...requestHeaders.entries()]})!==captured)throw new ApiError(401,'SESSION_REVOKED','The verified customer account changed.');});
}
