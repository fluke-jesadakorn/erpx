import 'server-only';
import {iamDb} from '@erpx/server-identity_access/public/iam-db';
import {ApiError} from '@erpx/server-runtime/public/http';
import {holdingResources,runtimeEnv} from '@erpx/server-runtime/public/runtime';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import type {ConnectionRow} from '@erpx/server-identity_access/domain/directories';
import {createConnectionLookup,requireDirectoryRoute} from '@erpx/server-identity_access/application/directory-lookup';
export async function connectionById(db: PostgresDatabase, companyId: string, id: string) { db = iamDb(db); return createConnectionLookup({ error: (...args) => new ApiError(...args), lookup: async (companyId, id) => db.prepare('SELECT * FROM iam_connections WHERE company_id=? AND id=?').bind(companyId, id).first<ConnectionRow>() })(companyId, id); }
export async function routedConnection(id: string) {
    const env = await runtimeEnv();
    const route = requireDirectoryRoute(await env.CONTROL_DB.prepare("SELECT * FROM iam_connection_routes WHERE connection_id=?")
        .bind(id)
        .first<{
        holding_id: string | null;
        company_id: string;
    }>(),(...args)=>new ApiError(...args));
    const db = (route.holding_id
        ? (await holdingResources(route.holding_id)).write
        : env.CONTROL_DB) as PostgresDatabase;
    return {
        db,
        holdingId: route.holding_id,
        connection: await connectionById(db, route.company_id, id),
    };
}
