import 'server-only';
import {createHash} from 'node:crypto';
import type {SqlDatabase,TransactionalSqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {assertAccountPreferenceSession} from '@erpx/server-identity_access/adapters/account-preference-session';
import {loadIamAuthority,iamRole} from '@erpx/server-identity_access/adapters/iam-store';
import {iamDb} from '@erpx/server-identity_access/adapters/iam-db';
import type {ProviderContextSeed,ProviderContextSelection} from '@erpx/feature-identity_access/lib/provider-context';

type Actor={userId:string;sessionId:string;selectedCookie:string|null;audience?:string;remember?:boolean};
type ContextRow={user_id:string;assignment_id:string;revision:number;updated_at:number};
const digest=(value:unknown)=>createHash('sha256').update(JSON.stringify(value)).digest('hex');
export const providerContextAccountKey=(userId:string,sessionId:string)=>digest(['provider-context-account',userId,sessionId]);
const changed=()=>new ApiError(409,'PROVIDER_CONTEXT_CHANGED','Your account or Platform role changed. Reload Administration before choosing a role.');
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
 if(await database.prepare(`SELECT 1 FROM iam_revoked_sessions revocation WHERE revocation.company_id='platform' AND revocation.user_id=? AND revocation.session_id IN(?,(SELECT parent_session_id FROM sso_session_bindings WHERE session_id=?))`).bind(actor.userId,actor.sessionId,actor.sessionId).first())throw new ApiError(401,'SESSION_REVOKED','Sign in again to Administration.');
}
async function snapshot(database:SqlDatabase,actor:Actor):Promise<ProviderContextSeed>{
 await sessionCurrent(database,actor);
 const rows=(await database.prepare(`SELECT assignment.id,assignment.role,assignment.version FROM platform_assignments assignment WHERE assignment.user_id=? AND iam_assignment_active(assignment.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(assignment.user_id,'platform',floor(extract(epoch FROM clock_timestamp())*1000)::bigint) ORDER BY assignment.valid_until,assignment.id`).bind(actor.userId).all<{id:string;role:string;version:number}>()).results;
 const choices:ProviderContextSeed['choices']=[];
 for(const assignment of rows){
  const role=await iamRole(database,'platform',assignment.role);if(!role?.scopes.includes('platform'))continue;
  const authority=await loadIamAuthority(database,'platform',actor.userId,assignment.role,actor.sessionId);
  choices.push({id:assignment.id,label:role.name,version:digest([assignment,role,authority.identityVersion,authority.principalId,authority.freshUntil,authority.emergencyUntil])});
 }
 if(!choices.length)throw new ApiError(403,'PLATFORM_AUTHORITY_REQUIRED','A current assigned Platform role is required to open this provider context.');
 const selected=await database.prepare('SELECT user_id,assignment_id,revision,updated_at FROM admin_provider_contexts WHERE session_id=?').bind(actor.sessionId).first<ContextRow>();
 if(selected&&selected.user_id!==actor.userId)throw changed();
 const preference=selected?null:await database.prepare('SELECT platform_assignment_id FROM account_workspace_preferences WHERE user_id=?').bind(actor.userId).first<{platform_assignment_id:string|null}>();
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
   const receipt=await iamDb(database).prepare('SELECT company_id,actor_id,request_hash,result FROM iam_receipts WHERE id=?').bind(key).first<{company_id:string;actor_id:string;request_hash:string;result:ProviderContextSeed}>();
   if(receipt){
    if(receipt.company_id!=='platform'||receipt.actor_id!==actor.userId||receipt.request_hash!==requestHash)throw new ApiError(409,'IDEMPOTENCY_CONFLICT','This request key belongs to another captured selection.');
    if(receipt.result.contextKey!==before.contextKey||receipt.result.selectedId!==input.assignmentId||before.choices.find(choice=>choice.id===input.assignmentId)?.version!==input.choiceVersion)throw changed();
    await database.prepare('SET CONSTRAINTS ALL IMMEDIATE').run();if((await snapshot(database,actor)).contextKey!==before.contextKey)throw changed();return before;
   }
   if(input.expectedContext!==before.contextKey||input.expectedRevision!==before.revision)throw changed();
   const choice=before.choices.find(row=>row.id===input.assignmentId);if(!choice||choice.version!==input.choiceVersion)throw changed();
   const updated=before.revision===0?await database.prepare('INSERT INTO admin_provider_contexts(session_id,user_id,assignment_id,revision,updated_at) VALUES(?,?,?,1,?) ON CONFLICT(session_id) DO NOTHING RETURNING revision').bind(actor.sessionId,actor.userId,choice.id,Date.now()).first():await database.prepare('UPDATE admin_provider_contexts SET assignment_id=?,revision=revision+1,updated_at=? WHERE session_id=? AND user_id=? AND revision=? RETURNING revision').bind(choice.id,Date.now(),actor.sessionId,actor.userId,before.revision).first();
   if(!updated)throw changed();
   if(actor.remember)await database.prepare(`INSERT INTO account_workspace_preferences(user_id,platform_assignment_id,updated_at) VALUES(?,?,?) ON CONFLICT(user_id) DO UPDATE SET platform_assignment_id=excluded.platform_assignment_id,updated_at=excluded.updated_at`).bind(actor.userId,choice.id,Date.now()).run();
   const after=await snapshot(database,actor);
   if(after.selectedId!==choice.id||after.revision!==before.revision+1||after.choices.find(row=>row.id===choice.id)?.version!==choice.version)throw changed();
   await database.batch([database.prepare('INSERT INTO iam_receipts(id,company_id,actor_id,request_hash,result,created_at) VALUES(?,\'platform\',?,?,?::jsonb,?)').bind(key,actor.userId,requestHash,JSON.stringify(after),Date.now()),database.prepare('INSERT INTO iam_audit(id,company_id,actor_id,assignment_id,action,target_id,evidence,created_at) VALUES(?,\'platform\',?,?,\'provider.context.selected\',?,?::jsonb,?)').bind(`provider-context:${key}`,actor.userId,choice.id,choice.id,JSON.stringify({previousAssignmentId:before.selectedId,revision:after.revision}),Date.now())]);
   await database.prepare('SET CONSTRAINTS ALL IMMEDIATE').run();if((await snapshot(database,actor)).contextKey!==after.contextKey)throw changed();return after;
  }),
 };
}
