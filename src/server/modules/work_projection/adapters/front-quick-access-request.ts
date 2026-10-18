import 'server-only';
import {currentSession} from '../../identity_access/public/authorization';
import type {WorkContext} from '../../identity_access/public/authorization';
import {withCurrentSelfAccountPreferences} from '../../identity_access/public/private-session-preferences';
import {productHostForHost} from '@/lib/runtime/product-hosts';
import {frontAppForHost,type FrontHostApp} from '@/lib/shared/front-host';
import {ApiError} from '../../runtime/public/http';
import {frontCompanyChoices,frontCompanyItems,frontActionItems} from './front-quick-access';
import {readSessionQuickAccessPreferences,saveSessionQuickAccessPreferences} from './quick-access-preferences';

type Change={pinnedIds:string[]|null;expectedVersion:number;requestKey:string};
/** Company choices are an earlier owner-authorized catalogue. Only the current
 * account's picker preference commits here, never a Front role or transaction. */
export async function frontQuickAccessRequest(request:Request,app:FrontHostApp,readChange?:()=>Promise<Change>){
 if(frontAppForHost(request.headers.get('host'))!==app)throw new ApiError(404,'NOT_FOUND','This Front picker is unavailable.');
 const {session,requestHeaders}=await currentSession(),image=()=>JSON.stringify({session,headers:[...requestHeaders.entries()],url:request.url,app}),captured=image(),change=readChange?await readChange():undefined,
  input=change?{pinnedIds:change.pinnedIds===null?null:[...change.pinnedIds],expectedVersion:change.expectedVersion,requestKey:change.requestKey}:undefined,
  availableIds=frontCompanyItems(await frontCompanyChoices(app)).map(item=>item.id);
 const assertContext=()=>{if(image()!==captured)throw new ApiError(401,'SESSION_REVOKED','The current Front account changed.');};
 return withCurrentSelfAccountPreferences({userId:session.user.id,sessionId:session.session.id,audience:productHostForHost(requestHeaders.get('host'))},async database=>{
  assertContext();return input?saveSessionQuickAccessPreferences(database,{...input,userId:session.user.id,surface:'front-picker',contextKey:app,availableIds}):readSessionQuickAccessPreferences(database,session.user.id,'front-picker',app);
 },assertContext);
}

/** The original Front actor is refreshed after the bounded command body. This
 * is an earlier Holding authority observation, not a cross-database IAM lease;
 * retained preference pins never authorize a later Company business action. */
export async function frontCompanyQuickAccessRequest(request:Request,app:FrontHostApp,initialActor:WorkContext,refreshActor:()=>Promise<WorkContext>,readChange?:()=>Promise<Change>){
 const {session,requestHeaders}=await currentSession(),actorScope=(actor:WorkContext)=>JSON.stringify({userId:actor.userId,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,assignmentId:actor.assignmentId,role:actor.role,authority:actor.authority,iam:actor.iam}),scope=actorScope(initialActor),
  image=()=>JSON.stringify({session,headers:[...requestHeaders.entries()],url:request.url,app,scope:actorScope(initialActor)}),captured=image(),change=readChange?await readChange():undefined,
  input=change?{pinnedIds:change.pinnedIds===null?null:[...change.pinnedIds],expectedVersion:change.expectedVersion,requestKey:change.requestKey}:undefined,
  actor=readChange?await refreshActor():initialActor;
 if(actor.userId!==session.user.id||actorScope(actor)!==scope)throw new ApiError(409,'FRONT_CONTEXT_CHANGED','The original Front role or workspace changed. Reload before saving your shortcuts.');
 const availableIds=frontActionItems(app,actor,'').map(item=>item.id),assertContext=()=>{if(image()!==captured)throw new ApiError(401,'SESSION_REVOKED','The current Front account changed.');};
 return withCurrentSelfAccountPreferences({userId:session.user.id,sessionId:session.session.id,audience:productHostForHost(requestHeaders.get('host'))},async database=>{
  assertContext();return input?saveSessionQuickAccessPreferences(database,{...input,userId:actor.userId,surface:'front-'+app,contextKey:actor.companyId,availableIds}):readSessionQuickAccessPreferences(database,actor.userId,'front-'+app,actor.companyId);
 },assertContext);
}
