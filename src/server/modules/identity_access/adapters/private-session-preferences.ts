import 'server-only';
import {runtimeEnv} from '../../runtime/public/runtime';
import {transactionDatabase,type PostgresDatabase} from '../../runtime/public/runtime/postgres';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {assertAccountPreferenceSession} from './account-preference-session';

type Actor={userId:string;sessionId:string;email:string;audience:string|null};
type SelfAccountActor=Pick<Actor,'userId'|'sessionId'|'audience'>;
type IdentityPolicy={kind:'verified-external';email:string}|{kind:'self-account'};
const phases=new WeakMap<SqlDatabase,string>();
export function assertPrivateSessionPreferenceUnit(database:SqlDatabase,userId:string){if(phases.get(database)!==userId)throw new ApiError(503,'SESSION_PREFERENCE_COMPOSITION_REQUIRED','Personal preferences require their current owned session transaction.');}

/** This writes only Control-owned personal preferences. Holding document
 * validation is an earlier independent read, never a distributed commit. */
async function currentSessionPreferences<Result>(actor:SelfAccountActor,identity:IdentityPolicy,run:(database:SqlDatabase)=>Promise<Result>,assertRequestContext:()=>void){
 const owner=(await runtimeEnv()).CONTROL_DB as PostgresDatabase;
 if(!owner.pool)throw new ApiError(503,'SESSION_PREFERENCE_TRANSACTION_REQUIRED','Personal preferences require their native session transaction.');
 const client=await owner.pool.connect(),context=JSON.stringify({actor,identity});let database:SqlDatabase|undefined;
 try{
  await client.query('BEGIN ISOLATION LEVEL READ COMMITTED');await client.query("SET LOCAL lock_timeout='5s'");
  await client.query('LOCK TABLE session IN EXCLUSIVE MODE');
  await client.query('LOCK TABLE sso_session_bindings IN SHARE MODE');
  await client.query('LOCK TABLE quick_access_preferences,quick_access_preference_receipts IN SHARE ROW EXCLUSIVE MODE');
  const transaction=transactionDatabase(client);database=transaction;
  const current=async()=>{assertRequestContext();if(JSON.stringify({actor,identity})!==context)throw new ApiError(401,'SESSION_REVOKED','The current personal account changed.');await assertAccountPreferenceSession(transaction,actor);if(identity.kind==='verified-external'){const user=await transaction.prepare('SELECT email,"emailVerified" verified FROM "user" WHERE id=?').bind(actor.userId).first<{email:string;verified:number}>();if(!user||!Number(user.verified)||user.email.toLowerCase()!==identity.email.toLowerCase())throw new ApiError(401,'SESSION_REVOKED','Sign in again to edit your personal shortcuts.');}};
  await current();phases.set(transaction,actor.userId);const result=await run(transaction);
  await client.query('SET CONSTRAINTS ALL IMMEDIATE');await current();await client.query('COMMIT');return result;
 }catch(error){await client.query('ROLLBACK').catch(()=>{});if(['55P03','40P01'].includes((error as {code?:string}).code??''))throw new ApiError(409,'SESSION_AUTHORITY_BUSY','Your sign-in is changing. Reload and try again.');throw error;
 }finally{if(database)phases.delete(database);client.release();}
}
/** Portal identity still requires the currently verified recipient email. */
export function withCurrentPrivateSessionPreferences<Result>(actor:Actor,run:(database:SqlDatabase)=>Promise<Result>,assertRequestContext:()=>void){return currentSessionPreferences(actor,{kind:'verified-external',email:actor.email},run,assertRequestContext);}
/** Original personal Account preferences do not require verified email. This
 * separate named entry retains current session/family authority and grants no
 * verified external-recipient or Company authority to its caller. */
export function withCurrentSelfAccountPreferences<Result>(actor:SelfAccountActor,run:(database:SqlDatabase)=>Promise<Result>,assertRequestContext:()=>void){return currentSessionPreferences(actor,{kind:'self-account'},run,assertRequestContext);}
