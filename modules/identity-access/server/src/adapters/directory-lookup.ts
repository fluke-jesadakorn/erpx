import 'server-only';
import {iamDb} from '@erpx/identity-access-server/public/iam-db';
import {createConnectionLookup} from '@erpx/identity-access-server/application/directory-lookup';
import {iamConnectionByIdSql} from './read-tail-family-r27';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {holdingResources,runtimeEnv} from '@erpx/platform-runtime-server/public/runtime';
import type {PostgresDatabase} from '@erpx/platform-runtime-server/public/runtime/postgres';
import type {ConnectionRow} from '@erpx/identity-access-server/domain/directories';
import {requireDirectoryRoute} from '@erpx/identity-access-server/application/directory-lookup';
import {planIdentityControlRead,PC_CONNECTION_ROUTE} from './native-control-read-plan';

/**
 * Fail-closed transport for the mounted owner read
 * `POST /api/v1/holdings/{holding_slug}/identity-access/connection-read`.
 *
 * The axum lane executes `SELECT * FROM iam_connections WHERE company_id=? AND
 * id=?` and projects the complete retired row. This module carries no SQL, no
 * plan for the row read and no TypeScript fallback: absent/malformed config, an
 * unregistered Holding, a transport failure or an invalid reply throws 503
 * instead of reading the connection locally.
 */
export const NATIVE_CONNECTION_READ_ENV = 'ERPX_NATIVE_IDENTITY_ACCESS_CONFIG';

type Target = Readonly<{host: string; holdingSlug: string; holdingId: string}>;
export type NativeConnectionReadConfig = Readonly<{
    base: string;
    timeoutMs: number;
    targets: readonly Target[];
}>;

const RESERVED_PORTS = new Set([3000,3001,3100,3101,3103,5432,5433,5435,8080]);
const COLUMNS = [
    'id','company_id','name','provider','config','status','version',
    'tested_version','last_sync_at','health','last_error','created_by','created_at',
] as const;

const fail = (status: number, code: string, message: string): never => {
    throw new ApiError(status, code, message);
};
const unavailable = (message = 'The native Directory connection read is unavailable.'): never =>
    fail(503, 'NATIVE_CONNECTION_READ_UNAVAILABLE', message);
const invalidConfig = (): never =>
    fail(503, 'NATIVE_CONNECTION_READ_CONFIGURATION_INVALID', 'The native Directory connection transport is misconfigured.');
const invalidTarget = (): never =>
    fail(503, 'NATIVE_CONNECTION_READ_TARGET_INVALID', 'The native Directory connection target is unknown.');
const invalidReply = (): never =>
    fail(502, 'NATIVE_CONNECTION_READ_RESPONSE_INVALID', 'The native Directory connection reply was invalid.');
const record = (value: unknown): value is Record<string,unknown> =>
    value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max: number): value is string =>
    typeof value === 'string' && value.length > 0 && value.length <= max &&
    !/[\u0000-\u001f\u007f]/.test(value);
const integer = (value: unknown): value is number =>
    typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
const identifier = (value: unknown, max = 200): value is string =>
    text(value, max) && /^[A-Za-z0-9_.:-]+$/.test(value);
const validCompanyId = (value: unknown): value is string =>
    text(value, 200) && /^[A-Za-z0-9_.:*-]+$/.test(value);
const slug = (value: unknown): value is string =>
    text(value, 128) && /^[a-z0-9_-]+$/.test(value);

export function nativeConnectionReadConfig(raw: string | undefined): NativeConnectionReadConfig {
    if (raw === undefined || raw === '') return unavailable('The native Directory connection transport is not configured.');
    let value: Record<string,unknown>;
    try {
        const parsed: unknown = JSON.parse(raw);
        if (!record(parsed)) return invalidConfig();
        value = parsed;
    } catch {
        return invalidConfig();
    }
    if (!text(value.privateBaseUrl, 512) || !Number.isSafeInteger(value.timeoutMs) ||
        (value.timeoutMs as number) < 250 || (value.timeoutMs as number) > 30_000 ||
        !Array.isArray(value.holdings) || value.holdings.length < 1 || value.holdings.length > 64)
        return invalidConfig();
    let base: URL;
    try {
        base = new URL(value.privateBaseUrl);
    } catch {
        return invalidConfig();
    }
    if (base.protocol !== 'http:' ||
        !['127.0.0.1','[::1]'].includes(base.hostname) ||
        !base.port || RESERVED_PORTS.has(Number(base.port)) ||
        (Number(base.port) >= 3200 && Number(base.port) <= 3299) ||
        base.username || base.password || base.origin !== value.privateBaseUrl)
        return invalidConfig();
    const targets: Target[] = [];
    const hosts = new Set<string>();
    const holdingIds = new Set<string>();
    for (const item of value.holdings) {
        if (!record(item) || !text(item.origin, 512) || !identifier(item.holdingId) || !slug(item.holdingSlug))
            return invalidConfig();
        let origin: URL;
        try {
            origin = new URL(item.origin);
        } catch {
            return invalidConfig();
        }
        if (origin.protocol !== 'https:' || origin.origin !== item.origin ||
            origin.username || origin.password ||
            hosts.has(origin.host) || holdingIds.has(item.holdingId))
            return invalidConfig();
        hosts.add(origin.host);
        holdingIds.add(item.holdingId);
        targets.push({host: origin.host, holdingSlug: item.holdingSlug, holdingId: item.holdingId});
    }
    return {base: base.origin, timeoutMs: value.timeoutMs as number, targets};
}

function targetFor(config: NativeConnectionReadConfig, holdingId: string): Target {
    if (!identifier(holdingId)) return invalidTarget();
    const matches = config.targets.filter((target) => target.holdingId === holdingId);
    if (matches.length !== 1) return invalidTarget();
    return matches[0]!;
}

/** Validate every column the owner projected from the retired `SELECT *` row. */
export function parseNativeConnection(value: unknown): ConnectionRow {
    if (!record(value) || Object.keys(value).length !== COLUMNS.length ||
        !COLUMNS.every((column) => Object.hasOwn(value, column)))
        return invalidReply();
    const config = value.config;
    if (!record(config) || (config.mode !== 'ldaps' && config.mode !== 'oidc') ||
        !text(config.secretProfile, 80) || !text(config.authority, 200) ||
        !text(value.id, 200) || !text(value.company_id, 200) ||
        !text(value.name, 200) || !text(value.status, 80) ||
        !text(value.health, 80) || !text(value.created_by, 200) ||
        (value.provider !== 'active_directory' && value.provider !== 'entra_id' && value.provider !== 'univention') ||
        !integer(value.version) || !integer(value.created_at) ||
        !(value.tested_version === null || integer(value.tested_version)) ||
        !(value.last_sync_at === null || integer(value.last_sync_at)) ||
        !(value.last_error === null || (typeof value.last_error === 'string' && value.last_error.length <= 2048)))
        return invalidReply();
    return value as unknown as ConnectionRow;
}

/** One owner read. No local SQL, no retained reader and no config fallback. */
export async function nativeConnectionRead(
    holdingId: string,
    companyId: string,
    id: string,
    environment: Readonly<Record<string,string | undefined>> = process.env,
): Promise<ConnectionRow> {
    const config = nativeConnectionReadConfig(environment[NATIVE_CONNECTION_READ_ENV]);
    if (!identifier(id) || !validCompanyId(companyId)) return unavailable('The native Directory connection request was invalid.');
    const target = targetFor(config, holdingId);
    const url = new URL(`/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/identity-access/connection-read`, config.base);
    const headers = new Headers({
        host: target.host,
        accept: 'application/json',
        'content-type': 'application/json',
        'cache-control': 'no-cache',
    });
    let response: Response;
    try {
        response = await fetch(url, {
            method: 'POST',
            headers,
            body: JSON.stringify({company_id: companyId, id}),
            cache: 'no-store',
            redirect: 'error',
            signal: AbortSignal.timeout(config.timeoutMs),
        });
    } catch {
        return unavailable();
    }
    if (response.status !== 200) {
        void response.body?.cancel().catch(() => undefined);
        return unavailable();
    }
    let payload: unknown;
    try {
        payload = JSON.parse(await response.text()) as unknown;
    } catch {
        return invalidReply();
    }
    if (!record(payload) || Object.keys(payload).length !== 1)
        return invalidReply();
    const row = parseNativeConnection(payload.data);
    if (!identifier(row.id) || row.id !== id || row.company_id !== companyId)
        return invalidReply();
    return row;
}

/** Resolve the registered Holding/Company route from the owner's Control plan. */
async function resolveRoute(id: string) {
    const env = await runtimeEnv();
    const plan = await planIdentityControlRead(PC_CONNECTION_ROUTE,[id]);
    return requireDirectoryRoute(await env.CONTROL_DB.prepare(plan.sql)
        .bind(...plan.values)
        .first<{
        holding_id: string | null;
        company_id: string;
    }>(),(...args)=>new ApiError(...args));
}

/** Narrow native-only read for public consumers such as the login page. */
export async function nativeConnectionById(id: string): Promise<ConnectionRow> {
    const route = await resolveRoute(id);
    if (!route.holding_id) return invalidTarget();
    return nativeConnectionRead(route.holding_id, route.company_id, id);
}

/** Retained DB-bearing shape for callers that still run other owner reads. */
export async function routedConnection(id: string) {
    const route = await resolveRoute(id);
    if (!route.holding_id) return invalidTarget();
    const connection = await nativeConnectionRead(route.holding_id, route.company_id, id);
    const db = ((await holdingResources(route.holding_id)).write) as PostgresDatabase;
    return {
        db,
        holdingId: route.holding_id,
        connection,
    };
}

export async function connectionById(db: PostgresDatabase, companyId: string, id: string) { db = iamDb(db); return createConnectionLookup({ error: (...args) => new ApiError(...args), lookup: async (companyId, id) => db.prepare(iamConnectionByIdSql('company_id=? AND id=?')).bind(companyId, id).first<ConnectionRow>() })(companyId, id); }
