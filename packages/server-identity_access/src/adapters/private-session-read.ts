import "server-only";
import {runtimeEnv} from "../../../../packages/server-runtime/src/public/runtime";
import {transactionDatabase,type PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import {ApiError} from "@erpx/server-runtime/public/http";
import {assertAccountPreferenceSession} from "@erpx/server-identity_access/adapters/account-preference-session";

type PrivateSessionActor={userId:string;sessionId:string;email:string;audience:string|null};
/** Normal authentication must precede this lease. Control is read-only and is
 * acquired before a Holding unit. Its assertion must run inside that unit's
 * final checks, never as a fallible check after the Holding audit commits.
 * A completed read may precede a queued family revocation; this is not a
 * distributed transaction or a guarantee about later network delivery. */
export async function withCurrentPrivateSessionRead<Result>(actor:PrivateSessionActor,run:(assertCurrent:()=>Promise<void>)=>Promise<Result>){
 const owner=(await runtimeEnv()).CONTROL_DB as PostgresDatabase;
 if(!owner.pool)throw new ApiError(503,"SESSION_READ_TRANSACTION_REQUIRED","Private reads require the native current-session owner.");
 const client=await owner.pool.connect();let active=false;
 try{
  await client.query("BEGIN ISOLATION LEVEL READ COMMITTED");await client.query("SET LOCAL lock_timeout='5s'");
  // EXCLUSIVE precedes every snapshot and row lock. A family writer takes a
  // parent FOR UPDATE before DELETE; SHARE here could deadlock its upgrade.
  await client.query("LOCK TABLE session IN EXCLUSIVE MODE");
  await client.query("LOCK TABLE sso_session_bindings IN SHARE MODE");
  const database=transactionDatabase(client);active=true;
  const assertCurrent=async()=>{
   if(!active)throw new Error("PRIVATE_SESSION_READ_CLOSED");
   try{await assertAccountPreferenceSession(database,actor);}catch(error){if(error instanceof ApiError&&error.code==="SESSION_REVOKED")throw new ApiError(401,"SESSION_REVOKED","Sign in again to open this private document.");throw error;}
   const user=await database.prepare('SELECT email,"emailVerified" verified FROM "user" WHERE id=?').bind(actor.userId).first<{email:string;verified:number}>();
   if(!user||!Number(user.verified)||user.email.toLowerCase()!==actor.email.toLowerCase())throw new ApiError(401,"SESSION_REVOKED","Sign in again to open this private document.");
  };
  await assertCurrent();const result=await run(assertCurrent);await client.query("COMMIT");return result;
 }catch(error){await client.query("ROLLBACK").catch(()=>{});if(["55P03","40P01"].includes((error as {code?:string}).code??""))throw new ApiError(409,"SESSION_AUTHORITY_BUSY","Your sign-in is changing. Reload and try again.");throw error;
 }finally{active=false;client.release();}
}
