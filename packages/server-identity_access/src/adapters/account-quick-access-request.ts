import 'server-only';
import {currentSession} from '@erpx/server-identity_access/adapters/authorization';
import {withCurrentSelfAccountPreferences} from '@erpx/server-identity_access/adapters/private-session-preferences';
import {accountQuickAccessItems,type AccountQuickAccessArea} from '@erpx/server-identity_access/domain/account-quick-access';
import {readSessionQuickAccessPreferences,saveSessionQuickAccessPreferences} from '@erpx/server-work_projection/public/quick-access-session-preferences';
import {workRoleOptions} from '@erpx/server-work_projection/public/work-role-options';
import {workspaceChoices} from '@erpx/server-organization/public/workspace-choices';
import {safeReturnPath} from '@erpx/feature-shared/lib/access-problem';
import {parseHoldingWorkspace,workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {productHostForHost} from '@erpx/feature-runtime/lib/product-hosts';
import {ApiError} from '@erpx/server-runtime/public/http';

type Change={pinnedIds:string[]|null;expectedVersion:number;requestKey:string};
/** Catalogue visibility is an earlier authorized owner read. This unit commits
 * only self-account preference state in Control, never a Company role choice. */
export async function accountQuickAccessRequest(request:Request,area:AccountQuickAccessArea,change?:Change){
 const input=change?{pinnedIds:change.pinnedIds===null?null:[...change.pinnedIds],expectedVersion:change.expectedVersion,requestKey:change.requestKey}:undefined;
 const {session,requestHeaders}=await currentSession(),captured=JSON.stringify({session,headers:[...requestHeaders.entries()],url:request.url}),choices=await workspaceChoices();
 let surface=`account-${area}`,contextKey='account',items=accountQuickAccessItems(area,{hasWorkspace:choices.length>0});
 if(area==='work-role'){
  const from=safeReturnPath(new URL(request.url).searchParams.get('from'));
  if(!from)throw new ApiError(422,'INVALID_RETURN_PATH','Choose a valid workspace page.');
  const parts=new URL(from,'https://erpx.invalid').pathname.split('/').filter(Boolean);
  if(parts[0]!=='holdings'||!parts[1])throw new ApiError(422,'INVALID_RETURN_PATH','Choose a holding workspace page.');
  const parsed=parseHoldingWorkspace(parts[1],parts.slice(2));if(!parsed)throw new ApiError(422,'INVALID_RETURN_PATH','Choose a valid workspace page.');
  const {access,options}=await workRoleOptions(parsed.scope);if(access.userId!==session.user.id)throw new ApiError(401,'SESSION_REVOKED','The current personal account changed.');
  surface='account-work-role';contextKey=workspaceBase(parsed.scope);items=accountQuickAccessItems(area,{hasWorkspace:choices.length>0,hasRoles:options.length>0});
 }
 const assertContext=()=>{if(JSON.stringify({session,headers:[...requestHeaders.entries()],url:request.url})!==captured)throw new ApiError(401,'SESSION_REVOKED','The current personal account changed.');};
 return withCurrentSelfAccountPreferences({userId:session.user.id,sessionId:session.session.id,audience:productHostForHost(requestHeaders.get('host'))},async database=>{
  assertContext();return input?saveSessionQuickAccessPreferences(database,{...input,userId:session.user.id,surface,contextKey,availableIds:items.map(item=>item.id)}):readSessionQuickAccessPreferences(database,session.user.id,surface,contextKey);
 },assertContext);
}
