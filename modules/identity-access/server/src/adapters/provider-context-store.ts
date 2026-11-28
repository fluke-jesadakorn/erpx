import 'server-only';
import { nativePrepare } from '@erpx/identity-access-server/public/native-identity-write';
import {createHash} from 'node:crypto';
import type {SqlDatabase,TransactionalSqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {assertAccountPreferenceSession} from '@erpx/identity-access-server/adapters/account-preference-session';
import {loadIamAuthority,iamRole} from '@erpx/identity-access-server/adapters/iam-store';
import {iamDb} from '@erpx/identity-access-server/adapters/iam-db';
import {planIdentityControlRead,PC_REVOKED_SESSION,PC_PLATFORM_ASSIGNMENTS,PC_PROVIDER_CONTEXT,PC_WORKSPACE_PREFERENCE,PC_CONTEXT_RECEIPT} from './native-control-read-plan';
import type {ProviderContextSeed,ProviderContextSelection} from '@erpx/identity-access-ui/lib/provider-context';

type Actor={userId:string;sessionId:string;selectedCookie:string|null;audience?:string;remember?:boolean};
type ContextRow={user_id:string;assignment_id:string;revision:number;updated_at:number};
const digest=(value:unknown)=>createHash('sha256').update(JSON.stringify(value)).digest('hex');
export const providerContextAccountKey=(userId:string,sessionId:string)=>digest(['provider-context-account',userId,sessionId]);
const changed=()=>new ApiError(409,'PROVIDER_CONTEXT_CHANGED','Your account or Platform role changed. Reload Administration before choosing a role.');
/** The native owner plans the exact statement; the caller binds and executes it
 * on its own already-open Control unit so the source leases still hold. */
async function controlRead(database:SqlDatabase,kind:string,values:readonly string[]){
 const plan=await planIdentityControlRead(kind,values);
 return database.prepare(plan.sql).bind(...plan.values);
}
/** These locks establish no data snapshot. The dedicated context relation is
 * serializes selectors. Session EXCLUSIVE precedes it because normal family
 * deletion locks session rows before cascading to this new context relation. */
async function leases(database:SqlDatabase){
 await database.prepare('LOCK TABLE session IN EXCLUSIVE MODE').run();
 await database.prepare('LOCK TABLE admin_provider_contexts,iam_receipts,iam_audit IN SHARE ROW EXCLUSIVE MODE').run();
 await database.prepare('LOCK TABLE "user",session,sso_session_bindings,platform_assignments,iam_roles,iam_role_revisions,iam_members,iam_identities,iam_connections,iam_revoked_sessions,iam_emergency_sessions,account_workspace_preferences IN SHARE MODE').run();
}
async function sessionCurrent(database:SqlDatabase,actor:Actor){
 await assertAccountPreferenceSession(database,{...actor,audience:actor.audience??'admin'});
 if(await (await controlRead(database,PC_REVOKED_SESSION,[actor.userId,actor.sessionId,actor.sessionId])).first())throw new ApiError(401,'SESSION_REVOKED','Sign in again to Administration.');
}
async function snapshot(database:SqlDatabase,actor:Actor):Promise<ProviderContextSeed>{
 await sessionCurrent(database,actor);
 const rows=(await (await controlRead(database,PC_PLATFORM_ASSIGNMENTS,[actor.userId])).all<{id:string;role:string;version:number}>()).results;
 const choices:ProviderContextSeed['choices']=[];
 for(const assignment of rows){
  const role=await iamRole(database,'platform',assignment.role);if(!role?.scopes.includes('platform'))continue;
  const authority=await loadIamAuthority(database,'platform',actor.userId,assignment.role,actor.sessionId);
  choices.push({id:assignment.id,label:role.name,version:digest([assignment,role,authority.identityVersion,authority.principalId,authority.freshUntil,authority.emergencyUntil])});
 }
 if(!choices.length)throw new ApiError(403,'PLATFORM_AUTHORITY_REQUIRED','A current assigned Platform role is required to open this provider context.');
 const selected=await (await controlRead(database,PC_PROVIDER_CONTEXT,[actor.sessionId])).first<ContextRow>();
 if(selected&&selected.user_id!==actor.userId)throw changed();
 const preference=selected?null:await (await controlRead(database,PC_WORKSPACE_PREFERENCE,[actor.userId])).first<{platform_assignment_id:string|null}>();
 const selectedId=[selected?.assignment_id,actor.selectedCookie,preference?.platform_assignment_id,choices[0].id].find(id=>choices.some(choice=>choice.id===id))!,revision=Number(selected?.revision??0),accountKey=providerContextAccountKey(actor.userId,actor.sessionId);
 return {accountKey,contextKey:digest(['provider-context',actor.userId,actor.sessionId,selected??null,selectedId,choices]),selectedId,revision,choices};
}
/** Closed complete Control unit. Inputs never create assignments, permissions,
 * MFA, Company grants or a product session. */
export function providerContextStore(owner:TransactionalSqlDatabase,actor:Actor){
 return {
  read:()=>owner.transaction(async database=>{await leases(database);return snapshot(database,actor);}),
  select:(input:ProviderContextSelection,key:string)=>owner.transaction(async database=>{
   await leases(database);const before=await snapshot(database,actor),requestHash=digest(['provider.context.select',actor.userId,actor.sessionId,input]);
   if(input.accountKey!==before.accountKey)throw changed();
   const receipt=await (await controlRead(iamDb(database),PC_CONTEXT_RECEIPT,[key])).first<{company_id:string;actor_id:string;request_hash:string;result:ProviderContextSeed}>();
   if(receipt){
    if(receipt.company_id!=='platform'||receipt.actor_id!==actor.userId||receipt.request_hash!==requestHash)throw new ApiError(409,'IDEMPOTENCY_CONFLICT','This request key belongs to another captured selection.');
    if(receipt.result.contextKey!==before.contextKey||receipt.result.selectedId!==input.assignmentId||before.choices.find(choice=>choice.id===input.assignmentId)?.version!==input.choiceVersion)throw changed();
    await database.prepare('SET CONSTRAINTS ALL IMMEDIATE').run();if((await snapshot(database,actor)).contextKey!==before.contextKey)throw changed();return before;
   }
   if(input.expectedContext!==before.contextKey||input.expectedRevision!==before.revision)throw changed();
   const choice=before.choices.find(row=>row.id===input.assignmentId);if(!choice||choice.version!==input.choiceVersion)throw changed();
   const updated=before.revision===0?await nativePrepare("w_09c11cc48fb3").bind(actor.sessionId,actor.userId,choice.id,Date.now()).first():await nativePrepare("w_ab9a4febec42").bind(choice.id,Date.now(),actor.sessionId,actor.userId,before.revision).first();
   if(!updated)throw changed();
   if(actor.remember)await nativePrepare("w_8bcbd26a42b8").bind(actor.userId,choice.id,Date.now()).run();
   const after=await snapshot(database,actor);
   if(after.selectedId!==choice.id||after.revision!==before.revision+1||after.choices.find(row=>row.id===choice.id)?.version!==choice.version)throw changed();
   await Promise.all([nativePrepare("w_2c7c80aef2a3").bind(key,actor.userId,requestHash,JSON.stringify(after),Date.now()),nativePrepare("w_2858784f6372").bind(`provider-context:${key}`,actor.userId,choice.id,choice.id,JSON.stringify({previousAssignmentId:before.selectedId,revision:after.revision}),Date.now())]);
   await database.prepare('SET CONSTRAINTS ALL IMMEDIATE').run();if((await snapshot(database,actor)).contextKey!==after.contextKey)throw changed();return after;
  }),
 };
}
