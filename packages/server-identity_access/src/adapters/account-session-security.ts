import 'server-only';
import {createHash} from 'node:crypto';
import type {SqlDatabase,TransactionalSqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {assertAccountPreferenceSession} from '@erpx/server-identity_access/adapters/account-preference-session';
import {revokeProductSessionFamily} from '@erpx/server-identity_access/adapters/product-session-revocation';
import {bindingIsCurrent} from '@erpx/server-identity_access/domain/product-sso';
import {iamDb} from '@erpx/server-identity_access/adapters/iam-db';
import type {AccountSecurityInput,AccountSecuritySeed} from '@erpx/feature-identity_access/lib/account-security';

type Actor={userId:string;sessionId:string};
type Session={id:string;user_id:string;created_at:string;expires_at:string;user_agent:string|null;image:string};
type Binding={session_id:string;parent_session_id:string|null;audience:string;environment:string;authenticated_at:number;absolute_expires_at:number;image:string};
type MfaSession={session_id:string;user_id:string;verified_at:number;image:string};
type Image={sessions:Session[];bindings:Binding[];mfa:{enabled:boolean;image:string}|null;verifications:MfaSession[];user:string|null};
const digest=(value:unknown)=>createHash('sha256').update(JSON.stringify(value)).digest('hex');
const changed=()=>new ApiError(409,'ACCOUNT_SECURITY_CHANGED','Your sign-ins or authenticator changed. Reload and review the current sessions.');
const accountKey=(actor:Actor)=>digest(['account-security',actor.userId,actor.sessionId]);
const familyHandle=(actor:Actor,parent:string)=>digest(['account-family',actor.userId,actor.sessionId,parent]);
/** The same session-first order as Provider context: no data snapshot precedes
 * the session writer lease, including when a normal family deletion is waiting. */
async function leases(database:SqlDatabase){
 await database.prepare('LOCK TABLE session IN EXCLUSIVE MODE').run();
 await database.prepare('LOCK TABLE iam_receipts,iam_audit IN SHARE ROW EXCLUSIVE MODE').run();
 await database.prepare('LOCK TABLE sso_session_bindings,iam_mfa,iam_mfa_sessions,"user" IN SHARE MODE').run();
}
async function source(database:SqlDatabase,actor:Actor):Promise<Image>{
 const sessions=(await database.prepare('SELECT id,"userId" user_id,"createdAt"::text created_at,"expiresAt"::text expires_at,"userAgent" user_agent,to_jsonb(s)::text image FROM session s ORDER BY id').all<Session>()).results;
 const bindings=(await database.prepare('SELECT session_id,parent_session_id,audience,environment,authenticated_at,absolute_expires_at,to_jsonb(b)::text image FROM sso_session_bindings b ORDER BY session_id').all<Binding>()).results;
 const mfa=await database.prepare('SELECT enabled,to_jsonb(m)::text image FROM iam_mfa m WHERE user_id=?').bind(actor.userId).first<{enabled:boolean;image:string}>(),verifications=(await database.prepare('SELECT session_id,user_id,verified_at,to_jsonb(v)::text image FROM iam_mfa_sessions v WHERE user_id=? ORDER BY session_id').bind(actor.userId).all<MfaSession>()).results,user=await database.prepare('SELECT to_jsonb(u)::text image FROM "user" u WHERE id=?').bind(actor.userId).first<string>('image');
 return {sessions,bindings,mfa,verifications,user};
}
function freshUntil(image:Image,actor:Actor){const verification=image.verifications.find(row=>row.session_id===actor.sessionId&&row.user_id===actor.userId);return image.mfa?.enabled&&verification?Number(verification.verified_at)+300000:null;}
function seed(image:Image,actor:Actor):AccountSecuritySeed{
 const now=Date.now(),own=image.sessions.filter(row=>row.user_id===actor.userId),families:AccountSecuritySeed['families']=[];
 for(const row of own){const binding=image.bindings.find(item=>item.session_id===row.id);if(!binding||binding.parent_session_id||!bindingIsCurrent({sessionId:binding.session_id,parentSessionId:null,audience:binding.audience,environment:binding.environment,authenticatedAt:Number(binding.authenticated_at),absoluteExpiresAt:Number(binding.absolute_expires_at)},'account','development',now)||Date.parse(row.expires_at)<=now)continue;
  families.push({handle:familyHandle(actor,row.id),current:row.id===actor.sessionId,createdAt:Number(binding.authenticated_at),expiresAt:Math.min(Date.parse(row.expires_at),Number(binding.absolute_expires_at)),userAgent:row.user_agent?.slice(0,500)??null,audiences:[...new Set(image.bindings.filter(item=>item.parent_session_id===row.id&&item.environment==='development'&&own.some(session=>session.id===item.session_id)).map(item=>item.audience))].sort()});
 }
 return {accountKey:accountKey(actor),revision:digest({actor,sessions:own,bindings:image.bindings.filter(binding=>own.some(row=>row.id===binding.session_id||row.id===binding.parent_session_id)),mfa:image.mfa,verifications:image.verifications,user:image.user}),mfaEnabled:!!image.mfa?.enabled,mfaFreshUntil:freshUntil(image,actor),families,legacySessions:own.filter(row=>!image.bindings.some(binding=>binding.session_id===row.id)&&Date.parse(row.expires_at)>now).length};
}
/** A closed current-Account unit, never a Company/Platform management actor.
 * Only display flags and opaque handles leave this source boundary. */
export function accountSessionSecurity(owner:TransactionalSqlDatabase,actor:Actor){
 return {
  read:()=>owner.transaction(async database=>{await leases(database);await assertAccountPreferenceSession(database,{...actor,audience:'account'});return seed(await source(database,actor),actor);}),
  revoke:(input:AccountSecurityInput,key:string)=>owner.transaction(async database=>{
   await leases(database);await assertAccountPreferenceSession(database,{...actor,audience:'account'});const before=await source(database,actor),initial=seed(before,actor),fresh=freshUntil(before,actor);
   if(!fresh||fresh<=Date.now())throw new ApiError(403,'FRESH_MFA_REQUIRED','Verify your authenticator before revoking sessions.');
   if(input.accountKey!==initial.accountKey)throw changed();
   const hash=digest(['account.sessions.revoke',actor,input]),receipt=await iamDb(database).prepare('SELECT company_id,actor_id,request_hash,result FROM iam_receipts WHERE id=?').bind(key).first<{company_id:string;actor_id:string;request_hash:string;result:{revoked:true;count:number;reauthenticate:boolean}}>();
   if(receipt){if(receipt.company_id!=='account'||receipt.actor_id!==actor.userId||receipt.request_hash!==hash)throw new ApiError(409,'IDEMPOTENCY_CONFLICT','This key belongs to another captured security request.');await database.prepare('SET CONSTRAINTS ALL IMMEDIATE').run();await assertAccountPreferenceSession(database,{...actor,audience:'account'});if(JSON.stringify(await source(database,actor))!==JSON.stringify(before)||fresh<=Date.now())throw changed();return receipt.result;}
   if(input.expectedRevision!==initial.revision)throw changed();
   const own=before.sessions.filter(row=>row.user_id===actor.userId);let ids:string[];
   if(input.scope==='family'){
    const parent=own.find(row=>familyHandle(actor,row.id)===input.family&&initial.families.some(item=>item.handle===input.family));if(!parent)throw new ApiError(404,'ACCOUNT_FAMILY_NOT_FOUND','This sign-in is no longer available.');
    ids=before.sessions.filter(row=>row.id===parent.id||before.bindings.some(binding=>binding.session_id===row.id&&binding.parent_session_id===parent.id&&binding.environment==='development')).map(row=>row.id);
    if(ids.some(id=>!own.some(row=>row.id===id)))throw changed();
    await revokeProductSessionFamily(database,parent.id,'development');
   }else{ids=own.map(row=>row.id);await database.prepare('DELETE FROM session WHERE "userId"=?').bind(actor.userId).run();}
   const expected={...before,sessions:before.sessions.filter(row=>!ids.includes(row.id)),verifications:before.verifications.filter(row=>!ids.includes(row.session_id))},result={revoked:true as const,count:ids.length,reauthenticate:ids.includes(actor.sessionId)},instant=Date.now();
   await database.batch([database.prepare("INSERT INTO iam_receipts(id,company_id,actor_id,request_hash,result,created_at) VALUES(?,'account',?,?,?::jsonb,?)").bind(key,actor.userId,hash,JSON.stringify(result),instant),database.prepare("INSERT INTO iam_audit(id,company_id,actor_id,assignment_id,action,target_id,evidence,created_at) VALUES(?,'account',?,'self','account.sessions.revoked',?,?::jsonb,?)").bind(`account-security:${key}`,actor.userId,input.scope,JSON.stringify({reason:input.reason,scope:input.scope,count:ids.length,currentFamily:result.reauthenticate}),instant)]);
   await database.prepare('SET CONSTRAINTS ALL IMMEDIATE').run();
   if(JSON.stringify(await source(database,actor))!==JSON.stringify(expected)||fresh<=Date.now())throw changed();
   if(!result.reauthenticate)await assertAccountPreferenceSession(database,{...actor,audience:'account'});
   return result;
  }),
 };
}
