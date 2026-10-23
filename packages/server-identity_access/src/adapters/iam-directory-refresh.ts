import { accountAuthoritySnapshot } from '@erpx/server-identity_access/adapters/account-authority-snapshot';
import 'server-only';import {iamDb} from '@erpx/server-identity_access/public/iam-db';import {ApiError} from '@erpx/server-runtime/public/http';import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';import type {ConnectionRow} from '@erpx/server-identity_access/public/iam-directories';import {createRefreshDirectoryMember} from '@erpx/server-identity_access/application/directory-refresh';
export async function refreshDirectoryMember(db:PostgresDatabase,companyId:string,userId:string){db=iamDb(db);return createRefreshDirectoryMember({stale:async(companyId,userId,cutoff)=>await db
    .prepare(
      "SELECT c.* FROM iam_members m JOIN iam_connections c ON c.id=m.connection_id JOIN iam_identities i ON i.id=m.identity_id WHERE m.company_id=? AND m.user_id=? AND m.status='active' AND m.login_policy='directory' AND c.status='active' AND i.observed_at<=?",
    )
    .bind(companyId, userId, cutoff)
    .first<ConnectionRow>(),synchronize:async(row)=>{const{synchronizeConnection}=await import('@erpx/server-identity_access/public/directory-synchronization');await synchronizeConnection(db,row);},now:Date.now,error:(...args)=>new ApiError(...args)})(companyId,userId);}

/** Fresh readiness; optional grants remain unusable until directory refresh succeeds. */
export async function accountDirectoryGate(raw:PostgresDatabase,companyId:string,userId:string,sessionId:string,options:{includeAccountGrants?:boolean;observedNow?:number}={}) {
 const db=iamDb(raw);
 const observedNow=options.observedNow??Date.now();
 let current=options.includeAccountGrants?await accountAuthoritySnapshot(raw,companyId,userId,observedNow,sessionId):undefined;
 const gate=current??await db.prepare(`WITH revoked AS MATERIALIZED (
   SELECT EXISTS(SELECT 1 FROM iam_revoked_sessions WHERE company_id=? AND user_id=? AND session_id=?) blocked
 ) SELECT blocked revoked,(SELECT to_jsonb(c) FROM iam_members m
   JOIN iam_connections c ON c.id=m.connection_id JOIN iam_identities i ON i.id=m.identity_id
   WHERE NOT revoked.blocked AND m.company_id=? AND m.user_id=? AND m.status='active'
     AND m.login_policy='directory' AND c.status='active' AND i.observed_at<=? LIMIT 1) stale
 FROM revoked`).bind(companyId,userId,sessionId,companyId,userId,Date.now()-300000)
 .first<{revoked:boolean;stale:ConnectionRow|string|null}>();
 if(!gate)throw new ApiError(500,'ACCOUNT_ACCESS_UNAVAILABLE','Unable to read current account access.');
 const stale=typeof gate.stale==='string'?JSON.parse(gate.stale) as ConnectionRow:gate.stale;
 return {revoked:gate.revoked,get accountGrants(){return current?{rows:current.rows,roles:current.roles}:undefined;},refresh:async()=>{
   // Never initiate external directory work for a revoked session.
   if(gate.revoked)throw new ApiError(401,'SESSION_REVOKED','Sign in again to access this company.');
   await createRefreshDirectoryMember({stale:async()=>stale,synchronize:async(row:ConnectionRow)=>{
     const {synchronizeConnection}=await import('@erpx/server-identity_access/public/directory-synchronization');
     await synchronizeConnection(db,row);
   },now:Date.now,error:(...args)=>new ApiError(...args)})(companyId,userId);
   if(options.includeAccountGrants&&stale){
     // Provider work can change membership or revoke the original session.
     // Discard the pre-refresh projection and read fresh authority afterwards.
     current=await accountAuthoritySnapshot(raw,companyId,userId,observedNow,sessionId);
     if(current.revoked)throw new ApiError(401,'SESSION_REVOKED','Sign in again to access this company.');
     if(current.stale)throw new ApiError(403,'DIRECTORY_ACCESS_UNAVAILABLE','Directory verification failed. Dependent access is unavailable until a successful refresh.');
   }
 }};
}
