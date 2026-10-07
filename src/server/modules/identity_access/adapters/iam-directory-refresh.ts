import 'server-only';import {iamDb} from '@/server/modules/identity_access/public/iam-db';import {ApiError} from '@/server/modules/runtime/public/http';import type {PostgresDatabase} from '@/server/modules/runtime/public/runtime/postgres';import type {ConnectionRow} from '@/server/modules/identity_access/public/iam-directories';import {createRefreshDirectoryMember} from '../application/directory-refresh';
export async function refreshDirectoryMember(db:PostgresDatabase,companyId:string,userId:string){db=iamDb(db);return createRefreshDirectoryMember({stale:async(companyId,userId,cutoff)=>await db
    .prepare(
      "SELECT c.* FROM iam_members m JOIN iam_connections c ON c.id=m.connection_id JOIN iam_identities i ON i.id=m.identity_id WHERE m.company_id=? AND m.user_id=? AND m.status='active' AND m.login_policy='directory' AND c.status='active' AND i.observed_at<=?",
    )
    .bind(companyId, userId, cutoff)
    .first<ConnectionRow>(),synchronize:async(row)=>{const{synchronizeConnection}=await import('@/server/modules/identity_access/public/iam-directories');await synchronizeConnection(db,row);},now:Date.now,error:(...args)=>new ApiError(...args)})(companyId,userId);}
