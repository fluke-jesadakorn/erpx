import 'server-only';
import type {PoolClient} from 'pg';
import type {ConnectionRow} from '@erpx/server-identity_access/domain/directories';
import type {ProviderSnapshot} from '@erpx/server-identity_access/domain/providers';
import type {DirectoryTransaction} from '@erpx/server-identity_access/application/directories';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import {iamDb} from '@erpx/server-identity_access/public/iam-db';
import {ApiError} from '@erpx/server-runtime/public/http';
import {readDirectory} from '@erpx/server-identity_access/public/iam-providers';
import {validateIamGrant,insertIamGrant} from '@erpx/server-identity_access/public/iam-grants';
import {directoryTx} from '@erpx/server-identity_access/adapters/directory-transaction';
import {createDirectorySynchronization} from '@erpx/server-identity_access/application/directory-synchronization';
function synchronization(raw: PostgresDatabase) {
const db=iamDb(raw),clients=new WeakMap<DirectoryTransaction,PoolClient>();
const client=(tx:DirectoryTransaction)=>{const c=clients.get(tx);if(!c)throw Error('DIRECTORY_TRANSACTION_CLOSED');return c;};
return createDirectorySynchronization({error: (...args) => new ApiError(...args),
now: Date.now,
id: () => crypto.randomUUID(),
apiError: (error): error is ApiError => error instanceof ApiError,
read: readDirectory,
syncFailure: async (connection, startedAt, code, cutoff) => { await db!.prepare("UPDATE iam_connections SET health='error',last_error=? WHERE id=? AND version=? AND COALESCE(last_sync_at,0)<=?").bind(code, connection.id, connection.version, startedAt).run(); await db!.prepare("UPDATE iam_identities SET observed_at=LEAST(observed_at,?) WHERE connection_id=? AND EXISTS(SELECT 1 FROM iam_connections WHERE id=? AND version=? AND health='error' AND COALESCE(last_sync_at,0)<=?)").bind(cutoff, connection.id, connection.id, connection.version, startedAt).run(); },
snapshotTransaction: async (connection, run) => {
            const c = await db!.pool.connect();let tx: DirectoryTransaction|undefined;
            try {
                await c.query('BEGIN ISOLATION LEVEL SERIALIZABLE');
                tx=directoryTx(c);clients.set(tx,c);
                const result = await run(tx);
                await c.query('COMMIT');
                return result;
            }
            catch (error) {
                await c.query('ROLLBACK');
                throw error;
            }
            finally {
                if(tx)clients.delete(tx);c.release();
            }
        },
validateGrant: (tx, actor, input, parentId) => validateIamGrant(client(tx), actor, input, parentId),
insertGrant: (tx, actor, input, parentId, source, sourceId) => insertIamGrant(client(tx), actor, input, parentId, source, sourceId)});
}
export async function applyDirectorySnapshot(db: PostgresDatabase,connection: ConnectionRow,snapshot: ProviderSnapshot) {return synchronization(db).applyDirectorySnapshot(connection,snapshot);}
export async function synchronizeConnection(db: PostgresDatabase,connection: ConnectionRow) {return synchronization(db).synchronizeConnection(connection);}
