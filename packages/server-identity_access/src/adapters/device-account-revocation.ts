import 'server-only';
import {activeHoldingBindings,holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import type {TransactionalSqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
export type DeviceChildSession={id:string;userId:string};
/** Control's session-first lease precedes these original Holding IAM fences.
 * The selection is acknowledged only after every active Holding rejects the
 * old contexts, including queued actors. Partial failure revokes safely and
 * requires SSO re-entry; it never claims a distributed business transaction. */
export async function invalidateDeviceWorkContexts(sessions:readonly DeviceChildSession[]){
 if(!sessions.length)return;
 try{
  for(const holding of await activeHoldingBindings()){
   const database=(await holdingResources(holding.id)).write as TransactionalSqlDatabase;
   await database.transaction(async transaction=>{
    await transaction.prepare("SET LOCAL lock_timeout='5s'").run();
    // Match queued authority's order before inserting a tombstone or removing
    // a context. Its SHARE lease serializes an in-flight owner commit first.
    await transaction.prepare('LOCK TABLE iam_revoked_sessions,active_work_contexts IN SHARE ROW EXCLUSIVE MODE').run();
    for(const session of sessions){
     await transaction.prepare(`INSERT INTO iam_revoked_sessions(session_id,company_id,user_id,revoked_at)
      SELECT session_id,company_id,user_id,? FROM active_work_contexts WHERE session_id=? AND user_id=?
      ON CONFLICT(session_id,company_id) DO NOTHING`).bind(Date.now(),session.id,session.userId).run();
     await transaction.prepare('DELETE FROM active_work_contexts WHERE session_id=? AND user_id=?').bind(session.id,session.userId).run();
    }
   });
  }
 }catch{throw new ApiError(503,'DEVICE_ACCOUNT_REENTRY_REQUIRED','Work context revocation could not finish. Re-enter through Account before continuing.');}
}
