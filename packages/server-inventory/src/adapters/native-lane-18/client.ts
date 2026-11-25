import 'server-only';
import { headers } from 'next/headers';
import { createHash } from 'node:crypto';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlValueRow } from '@erpx/server-runtime/public/runtime/sql';

/**
 * Strict private transport for the owner-local Inventory lane-18 stock read.
 *
 * The axum owner owns the retired statement, the Company/branch scope and the
 * row projection. This client emits no SQL and executes nothing on the
 * caller's database. Every registered Holding is supported: the target is
 * selected from the caller's WorkContext (or, for a legacy fact caller without
 * one, from the exact incoming Host). There is no first-Holding default and an
 * unknown target never reaches the private service.
 */
export type Lane18Actor = Pick<WorkContext, 'holdingId' | 'holdingSlug' | 'companyId'>;
export type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
export type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;
type Row = SqlValueRow;

const ENV = 'ERPX_NATIVE_INVENTORY_LANE18_READ_CONFIG';
const MAX_ROWS = 1000;
const ROUTE = 'inventory/lane-18/stock-availability';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const fail = (
  code = 'INVENTORY_LANE18_READ_UNAVAILABLE',
  status = 503,
  message = 'The native Inventory stock service is unavailable.',
): never => {
  throw new ApiError(status, code, message);
};
const unavailable = (): never => fail();
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const scopeId = (value: unknown): value is string =>
  text(value, 200) && value.length > 0 && /^[A-Za-z0-9_.:+-]+$/.test(String(value));
const hostId = (value: unknown): value is string =>
  text(value, 253) && value.length > 0 && !/[\s/?#@]/.test(String(value));

function configuration(): Settings {
  const raw = process.env[ENV];
  if (!raw) return unavailable();
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return fail('INVENTORY_LANE18_CONFIG_INVALID'); }
  if (
    !record(value) || !text(value.privateBaseUrl, 512) ||
    typeof value.timeoutMs !== 'number' || !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 || value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) || value.holdings.length < 1 || value.holdings.length > 64
  ) return fail('INVENTORY_LANE18_CONFIG_INVALID');
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return fail('INVENTORY_LANE18_CONFIG_INVALID'); }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
    RESERVED_PORTS.has(port) || (port >= 3200 && port <= 3299) || base.username || base.password ||
    base.origin !== value.privateBaseUrl
  ) return fail('INVENTORY_LANE18_CONFIG_INVALID');
  const holdings: Holding[] = [];
  const hosts = new Set<string>();
  const ids = new Set<string>();
  for (const item of value.holdings) {
    if (
      !record(item) || !text(item.origin, 512) || !hostId(item.host) ||
      !scopeId(item.holdingId) || !text(item.holdingSlug, 128) ||
      !/^[a-z0-9_-]{1,128}$/.test(String(item.holdingSlug)) ||
      (item.companyId !== undefined && !scopeId(item.companyId))
    ) return fail('INVENTORY_LANE18_CONFIG_INVALID');
    let origin: URL;
    try { origin = new URL(String(item.origin)); } catch { return fail('INVENTORY_LANE18_CONFIG_INVALID'); }
    if (
      origin.protocol !== 'https:' || origin.origin !== item.origin ||
      origin.username || origin.password || origin.host !== String(item.host) ||
      hosts.has(String(item.host)) || ids.has(String(item.holdingId))
    ) return fail('INVENTORY_LANE18_CONFIG_INVALID');
    hosts.add(String(item.host));
    ids.add(String(item.holdingId));
    holdings.push({
      origin: origin.origin,
      host: String(item.host),
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
      ...(item.companyId === undefined ? {} : { companyId: String(item.companyId) }),
    });
  }
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Resolve exactly one registered Holding. A WorkContext caller must name a
 * registered (holdingId, holdingSlug); a legacy fact caller is pinned by the
 * exact incoming Host. A configured companyId additionally pins the Company. */
async function targetFor(config: Settings, actor?: Lane18Actor): Promise<Holding> {
  const incoming = await headers().catch(() => null);
  const host = incoming?.get('host') ?? null;
  if (actor) {
    const target = config.holdings.find(
      holding =>
        holding.holdingId === actor.holdingId &&
        holding.holdingSlug === actor.holdingSlug &&
        (holding.companyId === undefined || holding.companyId === actor.companyId),
    );
    if (!target || (host !== null && host !== target.host))
      return fail('INVENTORY_LANE18_WORK_CONTEXT_CHANGED', 409);
    return target;
  }
  const target = host === null ? undefined : config.holdings.find(holding => holding.host === host);
  if (!target) return fail('INVENTORY_LANE18_WORK_CONTEXT_CHANGED', 409);
  return target;
}

function parseRow(value: unknown): Row | null {
  if (!record(value)) return null;
  const keys = Object.keys(value).sort();
  if (keys.join(',') !== 'product_id,quality_held,quantity_milli,reserved_milli,warehouse_id') return null;
  if (!scopeId(value.warehouse_id) || !scopeId(value.product_id)) return null;
  if (typeof value.quantity_milli !== 'number' || !Number.isSafeInteger(value.quantity_milli) || value.quantity_milli < 0) return null;
  if (typeof value.reserved_milli !== 'number' || !Number.isSafeInteger(value.reserved_milli) || value.reserved_milli < 0) return null;
  if (value.quality_held !== 0 && value.quality_held !== 1) return null;
  return {
    warehouse_id: value.warehouse_id,
    product_id: value.product_id,
    quantity_milli: value.quantity_milli,
    reserved_milli: value.reserved_milli,
    quality_held: value.quality_held,
  };
}

function parsePayload(payload: unknown): Row[] {
  if (!record(payload) || Object.keys(payload).sort().join(',') !== 'count,rows') return unavailable();
  if (!Array.isArray(payload.rows) || payload.rows.length > MAX_ROWS) return unavailable();
  if (typeof payload.count !== 'number' || payload.count !== payload.rows.length) return unavailable();
  const rows: Row[] = [];
  for (const item of payload.rows) {
    const row = parseRow(item);
    if (!row) return unavailable();
    rows.push(row);
  }
  return rows;
}

/** Read the owner-local stock-availability snapshot. `branch` is an already
 * resolved assignment scope; the second TS branch bind of the retired query
 * was the same value and is intentionally not sent twice. */
export async function readStockAvailability(
  actor: Lane18Actor | undefined,
  company: string,
  branch: string | null,
): Promise<Row[]> {
  if (!scopeId(company) || (branch !== null && !scopeId(branch)))
    throw new ApiError(422, 'INVENTORY_LANE18_REQUEST_INVALID', 'Use one bounded Company stock read.');
  const config = configuration();
  const holding = await targetFor(config, actor);
  if (holding.companyId !== undefined && holding.companyId !== company)
    return fail('INVENTORY_LANE18_WORK_CONTEXT_CHANGED', 409);

  const key = 'inv-lane18-' + createHash('sha256').update(holding.holdingId).update('\u0000').update(company).update('\u0000').update(branch ?? '').digest('hex').slice(0, 32);
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/${ROUTE}`, config.privateBaseUrl);
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'GET',
      headers: {
        host: holding.host,
        origin: holding.origin,
        accept: 'application/json',
        'cache-control': 'no-cache',
        'idempotency-key': key,
        'x-erpx-context-company': company,
        'x-erpx-context-holding': holding.holdingId,
        ...(branch === null ? {} : { 'x-erpx-context-branch': branch }),
      },
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch { return unavailable(); }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    const payload = await response.json().catch((): unknown => null);
    const err = record(payload) ? (payload as { error?: { code?: string; message?: string } }).error : undefined;
    if (response.status >= 400 && response.status < 500) {
      throw new ApiError(response.status, err?.code ?? 'INVENTORY_LANE18_READ_FAILED', err?.message ?? 'The native Inventory read was rejected.');
    }
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  return parsePayload(payload);
}

export function nativeInventoryLane18Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
