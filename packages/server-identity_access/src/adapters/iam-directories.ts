import {directoryTx as createDirectoryTransaction} from '@erpx/server-identity_access/adapters/directory-transaction';
import {connectionById} from '@erpx/server-identity_access/public/directory-lookup';
import {synchronizeConnection} from '@erpx/server-identity_access/public/directory-synchronization';
import { createDirectories, createDirectoryJobDrain, createRunDirectoryJob, type DirectoryTransaction } from '@erpx/server-identity_access/application/directories';
const clients = new WeakMap<DirectoryTransaction, PoolClient>();
import { iamDb } from "@erpx/server-identity_access/public/iam-db";
import "server-only";
import { z } from "zod";
import type { PoolClient } from "pg";
import { iamCommand } from "@erpx/server-identity_access/public/iam-transactions";
import { assertCompanyScope, type IamActor } from "@erpx/server-identity_access/public/iam-context";
import { requireFreshMfa } from "@erpx/server-identity_access/public/iam-mfa";
import { proposeIamReview } from "@erpx/server-identity_access/public/iam-service";
import {insertIamGrant,validateIamGrant} from '@erpx/server-identity_access/public/iam-grants';
import { readDirectory, testDirectory } from "@erpx/server-identity_access/public/iam-providers";
import { ApiError } from "@erpx/server-runtime/public/http";
import { runtimeEnv } from "@erpx/server-runtime/public/runtime";
import { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
export { connectionInput } from '@erpx/server-identity_access/domain/directories';
import { connectionInput, type ConnectionRow } from '@erpx/server-identity_access/domain/directories';
export type { ConnectionRow } from '@erpx/server-identity_access/domain/directories';


export async function saveConnection(db: PostgresDatabase, actor: IamActor, input: z.infer<typeof connectionInput>, key: string) { return directoryService(db).saveConnection(actor, input, key); }
export async function testConnection(db: PostgresDatabase, actor: IamActor, id: string, version: number, key: string) { return directoryService(db).testConnection(actor, id, version, key); }
export async function proposeConnectionActivation(db: PostgresDatabase, actor: IamActor, id: string, version: number, key: string) { return directoryService(db).proposeConnectionActivation(actor, id, version, key); }
export async function disableConnection(db: PostgresDatabase, actor: IamActor, id: string, version: number, key: string) { return directoryService(db).disableConnection(actor, id, version, key); }
/** Only complete snapshots replace membership; credentials never enter the snapshot. */


export { mappingInput } from '@erpx/server-identity_access/domain/directories';
import { mappingInput } from '@erpx/server-identity_access/domain/directories';
export async function proposeMapping(db: PostgresDatabase, actor: IamActor, input: z.infer<typeof mappingInput>, key: string) { return directoryService(db).proposeMapping(actor, input, key); }
export async function applyDirectoryReview(c: PoolClient, actor: IamActor, kind: string, p: Record<string, unknown>) { return directoryService().applyDirectoryReview(directoryTx(c, actor), actor, kind, p); }
async function drainOneDirectoryJob(raw:PostgresDatabase){const db=iamDb(raw);return createRunDirectoryJob({now:Date.now,token:()=>crypto.randomUUID(),claim:async(now,token,leaseUntil)=>await db
        .prepare(`UPDATE iam_sync_jobs SET lease_until=?,lease_token=?,attempts=attempts+1 WHERE connection_id=(SELECT j.connection_id FROM iam_sync_jobs j JOIN iam_connections c ON c.id=j.connection_id WHERE c.status='active' AND j.next_run_at<=? AND (j.lease_until IS NULL OR j.lease_until<?) ORDER BY j.next_run_at FOR UPDATE OF j SKIP LOCKED LIMIT 1) RETURNING connection_id`)
        .bind(leaseUntil, token, now, now)
        .first<{
        connection_id: string;
    }>(),connection:async(id)=>await db
        .prepare("SELECT * FROM iam_connections WHERE id=?")
        .bind(id)
        .first<ConnectionRow>(),renewing:async(connection,token,run)=>{    const renewal = setInterval(() => {
        void db
            .prepare("UPDATE iam_sync_jobs SET lease_until=? WHERE connection_id=? AND lease_token=?")
            .bind(Date.now() + 120000, connection.id, token)
            .run()
            .catch(() => { });
    }, 30000);
try{await run();}finally{clearInterval(renewal);}},synchronize:(connection)=>synchronizeConnection(db,connection),complete:async(connectionId,token,nextRun)=>{await db
            .prepare("UPDATE iam_sync_jobs SET next_run_at=?,lease_until=NULL,lease_token=NULL WHERE connection_id=? AND lease_token=?")
            .bind(nextRun, connectionId, token)
            .run();}})();}
export async function drainDirectoryJobs(db: PostgresDatabase) { return createDirectoryJobDrain({ one: () => drainOneDirectoryJob(db) })(); }
export { refreshDirectoryMember } from "@erpx/server-identity_access/public/iam-directory-refresh";
function directoryTx(c:PoolClient,actor?:IamActor):DirectoryTransaction {const tx=createDirectoryTransaction(c,actor);clients.set(tx,c);return tx;}
function directoryService(raw?: PostgresDatabase) {
    const db = raw ? iamDb(raw) : undefined;
    return createDirectories({ error: (...args) => new ApiError(...args), now: Date.now, id: () => crypto.randomUUID(), companyScope: assertCompanyScope, fresh: requireFreshMfa, apiError: (error): error is ApiError => error instanceof ApiError, connection: (companyId, id) => connectionById(db!, companyId, id), route: async (id, actor) => { await (await runtimeEnv()).CONTROL_DB.prepare('INSERT INTO iam_connection_routes VALUES(?,?,?) ON CONFLICT(connection_id) DO NOTHING').bind(id, actor.holdingId, actor.companyId).run(); }, test: testDirectory, read: readDirectory, testFailure: async (id, version, code) => { await db!.prepare("UPDATE iam_connections SET health='error',last_error=? WHERE id=? AND version=?").bind(code, id, version).run(); }, syncFailure: async (connection, startedAt, code, cutoff) => { await db!.prepare("UPDATE iam_connections SET health='error',last_error=? WHERE id=? AND version=? AND COALESCE(last_sync_at,0)<=?").bind(code, connection.id, connection.version, startedAt).run(); await db!.prepare("UPDATE iam_identities SET observed_at=LEAST(observed_at,?) WHERE connection_id=? AND EXISTS(SELECT 1 FROM iam_connections WHERE id=? AND version=? AND health='error' AND COALESCE(last_sync_at,0)<=?)").bind(cutoff, connection.id, connection.id, connection.version, startedAt).run(); }, command: async (actor, key, operation, payload, permission, run) => iamCommand(db!, actor, key, operation, payload, permission, async (c) => run(directoryTx(c, actor))), snapshotTransaction: async (connection, run) => {
            const c = await db!.pool.connect();
            try {
                await c.query('BEGIN ISOLATION LEVEL SERIALIZABLE');
                const result = await run(directoryTx(c));
                await c.query('COMMIT');
                return result;
            }
            catch (error) {
                await c.query('ROLLBACK');
                throw error;
            }
            finally {
                c.release();
            }
        }, validateGrant: (tx, actor, input, parentId) => validateIamGrant(clients.get(tx)!, actor, input, parentId), insertGrant: (tx, actor, input, parentId, source, sourceId) => insertIamGrant(clients.get(tx)!, actor, input, parentId, source, sourceId), review: (tx, actor, kind, payload, reason) => proposeIamReview(clients.get(tx)!, actor, kind, payload, reason) });
}
