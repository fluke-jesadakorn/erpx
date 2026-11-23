import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { readableRecordSourceIds as posSources } from "@erpx/server-pos/public/records-source-facts";
import { readablePmsSourceIds } from "@erpx/server-pms/public/pms/source";
import { headers } from 'next/headers';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/server-runtime/public/http';

/** Owned asset source facts for aggregate readers. The native Assets register
 * route (`erpx_server_assets::public::asset_http`) is the mounted contract: it
 * returns only rows the current work scope may read and the owning Rust port
 * rechecks each row's SHA-256 image digest. This adapter emits no owner SQL;
 * it carries the returned identifiers into the caller's predicate as bindings. */
type AssetIdsSettings = {
    privateBaseUrl: string;
    timeoutMs: number;
    holdings: { origin: string; holdingSlug: string; holdingId: string }[];
};
const fail = (status: number, code: string): never => {
    throw new ApiError(status, code, 'The native Assets source read is unavailable.');
};
const object = (value: unknown): value is Record<string, unknown> =>
    !!value && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown): value is string => typeof value === 'string' && value.length > 0;
function assetIdsConfig(raw: string): AssetIdsSettings {
    let value: unknown;
    try { value = JSON.parse(raw); } catch { return fail(503, 'NATIVE_ASSET_SOURCE_READ_CONFIG_INVALID'); }
    if (!object(value) || typeof value.privateBaseUrl !== 'string' ||
        !Number.isSafeInteger(value.timeoutMs) || (value.timeoutMs as number) < 250 || (value.timeoutMs as number) > 30000 ||
        !Array.isArray(value.holdings) || value.holdings.length < 1 || value.holdings.length > 64)
        return fail(503, 'NATIVE_ASSET_SOURCE_READ_CONFIG_INVALID');
    let base: URL;
    try { base = new URL(value.privateBaseUrl); } catch { return fail(503, 'NATIVE_ASSET_SOURCE_READ_CONFIG_INVALID'); }
    if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
        base.username || base.password || value.privateBaseUrl !== base.origin)
        return fail(503, 'NATIVE_ASSET_SOURCE_READ_CONFIG_INVALID');
    for (const holding of value.holdings) {
        if (!object(holding) || typeof holding.origin !== 'string' ||
            typeof holding.holdingSlug !== 'string' || !text(holding.holdingId))
            return fail(503, 'NATIVE_ASSET_SOURCE_READ_CONFIG_INVALID');
        try {
            const origin = new URL(holding.origin);
            if (origin.protocol !== 'https:' || holding.origin !== origin.origin || origin.port)
                return fail(503, 'NATIVE_ASSET_SOURCE_READ_CONFIG_INVALID');
        } catch { return fail(503, 'NATIVE_ASSET_SOURCE_READ_CONFIG_INVALID'); }
    }
    return value as AssetIdsSettings;
}
async function readAssetSources(settings: AssetIdsSettings, actor: WorkContext, incoming: Headers): Promise<{ ids: string[]; digest: string }> {
    const target = settings.holdings.find(h => h.holdingSlug === actor.holdingSlug && h.holdingId === actor.holdingId);
    if (!target || !actor.authority || !actor.iam) return fail(409, 'WORK_CONTEXT_CHANGED');
    const host = incoming.get('host');
    if (!host || new URL(target.origin).host !== host) return fail(409, 'WORK_CONTEXT_CHANGED');
    const h = new Headers({ host, accept: 'application/json', 'cache-control': 'no-cache' });
    const cookie = incoming.get('cookie');
    if (cookie) {
        if (new TextEncoder().encode(cookie).length > 8192) return fail(503, 'NATIVE_ASSET_SOURCE_READ_COOKIE_LIMIT');
        h.set('cookie', cookie);
    }
    h.set('x-erpx-context-company', actor.companyId);
    h.set('x-erpx-context-assignment', actor.assignmentId);
    h.set('x-erpx-context-revision', String(actor.authority.revision));
    h.set('x-erpx-context-assignment-version', String(actor.authority.assignmentVersion));
    if (actor.scope.kind !== 'holding' && actor.scope.branchSlug) h.set('x-erpx-workspace-branch', actor.scope.branchSlug);
    if (actor.scope.kind === 'department') h.set('x-erpx-workspace-department', actor.scope.departmentSlug);
    const url = new URL(`/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/assets`, settings.privateBaseUrl);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), settings.timeoutMs);
    let response: Response;
    try { response = await fetch(url, { method: 'GET', headers: h, signal: controller.signal, redirect: 'error', cache: 'no-store' }); }
    catch { return fail(503, 'NATIVE_ASSET_SOURCE_READ_UNAVAILABLE'); }
    finally { clearTimeout(timer); }
    if (response.status === 403 || response.status === 404) {
        void response.body?.cancel().catch(() => undefined);
        // No readable asset sources for this actor, matching the retired
        // permission-scoped projection rather than failing the whole query.
        return { ids: [], digest: 'empty' };
    }
    if (!response.ok || response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
        void response.body?.cancel().catch(() => undefined);
        return fail(503, 'NATIVE_ASSET_SOURCE_READ_UNAVAILABLE');
    }
    let parsed: unknown;
    try { parsed = await response.json(); } catch { return fail(503, 'NATIVE_ASSET_SOURCE_READ_RESPONSE_INVALID'); }
    if (!object(parsed) || !object(parsed.data) || !object(parsed.context) || !Array.isArray(parsed.data.items))
        return fail(503, 'NATIVE_ASSET_SOURCE_READ_RESPONSE_INVALID');
    const context = parsed.context;
    if (context.companyId !== actor.companyId || context.assignmentId !== actor.assignmentId ||
        context.revision !== actor.authority.revision || context.assignmentVersion !== actor.authority.assignmentVersion ||
        !Array.isArray(context.permissions) ||
        !actor.iam.permissions.every(permission => (context.permissions as unknown[]).includes(permission)))
        return fail(409, 'WORK_CONTEXT_CHANGED');
    const ids: string[] = [];
    const image: [string, number][] = [];
    for (const item of parsed.data.items) {
        if (!object(item) || !text(item.id) || typeof item.version !== 'number' || !Number.isSafeInteger(item.version) ||
            item.company_id !== actor.companyId)
            return fail(503, 'NATIVE_ASSET_SOURCE_READ_RESPONSE_INVALID');
        ids.push(item.id);
        image.push([item.id, item.version]);
    }
    image.sort((a, b) => a[0].localeCompare(b[0]));
    return { ids, digest: createHash('sha256').update(JSON.stringify(image)).digest('hex') };
}
async function nativeAssetSourceIds(actor: WorkContext): Promise<{ sql: string; bindings: unknown[] }> {
    const raw = process.env.ERPX_NATIVE_ASSETS_CONFIG;
    if (raw === undefined) return fail(503, 'NATIVE_ASSET_SOURCE_READ_UNAVAILABLE');
    const settings = assetIdsConfig(raw);
    const incoming = await headers();
    const first = await readAssetSources(settings, actor, incoming);
    // Recheck the native source set: a changed digest fails closed rather than
    // widening or narrowing an aggregate reader's source authority.
    const recheck = await readAssetSources(settings, actor, incoming);
    if (recheck.digest !== first.digest) return fail(409, 'NATIVE_ASSET_SOURCES_CHANGED');
    return recheck.ids.length
        ? { sql: recheck.ids.map(() => '?').join(','), bindings: recheck.ids }
        : { sql: 'NULL', bindings: [] };
}

/** Aggregate readers must apply the same source authority as ticket detail. */
export async function supportTicketSourceReadPredicate(actor: WorkContext, alias = "t") {
  const source = `${alias}.source_record_json::jsonb`, definitions = [
    ["pos_sale", await posSources(actor)], ["asset", await nativeAssetSourceIds(actor)], ["pms_record", await readablePmsSourceIds(actor)],
  ] as const;
  return { sql: `(${alias}.source_record_json IS NULL OR ${definitions.map(([kind, projection]) =>
    `(${source}->>'kind'='${kind}' AND ${source}->>'id' IN (${projection.sql}))`).join(" OR ")})`,
    bindings: definitions.flatMap(([, projection]) => projection.bindings) };
}
