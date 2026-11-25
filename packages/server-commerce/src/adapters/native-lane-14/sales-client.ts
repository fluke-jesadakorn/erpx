import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlResult } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

/**
 * Strict multi-Holding transport for the native Commerce Sales reads. The
 * axum owner executes the SQL, enforces Company/branch/department scope and
 * projects the reply; this client sends only bounded identities and fails
 * closed with 503 when the owner is absent or the reply is malformed.
 */
type Actor = Pick<
  WorkContext,
  'holdingId' | 'holdingSlug' | 'companyId' | 'branchId' | 'departmentId'
>;
type Holding = Readonly<{
  origin: string;
  host: string;
  holdingSlug: string;
  holdingId: string;
}>;
type Config = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;

export const NATIVE_LANE14_SALES_CONFIG_ENV = 'ERPX_NATIVE_COMMERCE_LANE14_SALES_CONFIG';

const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const UNAVAILABLE = 'COMMERCE_LANE14_SALES_UNAVAILABLE';
const CONFIG_INVALID = 'COMMERCE_LANE14_SALES_CONFIG_INVALID';
const RESPONSE_INVALID = 'COMMERCE_LANE14_SALES_RESPONSE_INVALID';

const fail = (code = UNAVAILABLE, message = 'The native Commerce Sales read service is unavailable.'): never => {
  throw new ApiError(503, code, message);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const validId = (value: unknown): value is string =>
  typeof value === 'string' && value.length > 0 && value.length <= 200 &&
  /^[A-Za-z0-9._:-]+$/.test(value);

export function nativeLane14SalesConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[NATIVE_LANE14_SALES_CONFIG_ENV] !== undefined;
}

function config(): Config {
  const raw = process.env[NATIVE_LANE14_SALES_CONFIG_ENV];
  if (raw === undefined) return fail(CONFIG_INVALID);
  let value: {
    privateBaseUrl?: unknown;
    timeoutMs?: unknown;
    holdings?: unknown;
  };
  try {
    value = JSON.parse(raw) as typeof value;
  } catch {
    return fail(CONFIG_INVALID);
  }
  if (
    !record(value) ||
    typeof value.privateBaseUrl !== 'string' ||
    typeof value.timeoutMs !== 'number' ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return fail(CONFIG_INVALID);
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail(CONFIG_INVALID);
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username || base.password || base.origin !== value.privateBaseUrl
  )
    return fail(CONFIG_INVALID);
  const holdings: Holding[] = [];
  const seenSlugs = new Set<string>();
  const seenIds = new Set<string>();
  for (const entry of value.holdings) {
    if (!record(entry)) return fail(CONFIG_INVALID);
    if (
      typeof entry.origin !== 'string' ||
      typeof entry.host !== 'string' ||
      typeof entry.holdingSlug !== 'string' ||
      typeof entry.holdingId !== 'string' ||
      !/^[a-z0-9_-]{1,128}$/.test(entry.holdingSlug) ||
      !validId(entry.holdingId) ||
      seenSlugs.has(entry.holdingSlug) ||
      seenIds.has(entry.holdingId)
    )
      return fail(CONFIG_INVALID);
    let origin: URL;
    try {
      origin = new URL(entry.origin);
    } catch {
      return fail(CONFIG_INVALID);
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== entry.origin ||
      origin.username || origin.password ||
      origin.host !== entry.host
    )
      return fail(CONFIG_INVALID);
    seenSlugs.add(entry.holdingSlug);
    seenIds.add(entry.holdingId);
    holdings.push({
      origin: origin.origin,
      host: entry.host,
      holdingSlug: entry.holdingSlug,
      holdingId: entry.holdingId,
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Never pick a pool implicitly: the caller's Holding must match exactly. */
function select(config: Config, actor: Actor): Holding {
  const holding = config.holdings.find(
    item => item.holdingId === actor.holdingId && item.holdingSlug === actor.holdingSlug,
  );
  if (!holding)
    return fail('COMMERCE_LANE14_SALES_HOLDING_INVALID', 'The selected Holding changed.');
  return holding;
}

/** GET/POST with an explicit Host header; fetch forbids overriding Host. */
function send(
  method: 'GET' | 'POST',
  url: URL,
  headers: Record<string, string>,
  body: string | undefined,
  timeoutMs: number,
): Promise<{ status: number; value: unknown }> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const outgoing = httpRequest(
      url,
      {
        method,
        agent,
        headers: {
          ...headers,
          ...(body === undefined ? {} : { 'content-length': String(Buffer.byteLength(body)) }),
        },
        signal: AbortSignal.timeout(timeoutMs),
        maxHeaderSize: 8192,
      },
      incoming => {
        const chunks: Buffer[] = [];
        incoming.on('data', chunk => chunks.push(Buffer.from(chunk)));
        incoming.on('end', () => {
          agent.destroy();
          const text = Buffer.concat(chunks).toString('utf8');
          let value: unknown = null;
          try {
            value = text ? JSON.parse(text) : null;
          } catch {
            value = null;
          }
          resolve({ status: incoming.statusCode ?? 502, value });
        });
        incoming.on('error', error => {
          agent.destroy();
          reject(error);
        });
      },
    );
    outgoing.on('error', error => {
      agent.destroy();
      reject(error);
    });
    outgoing.end(body);
  });
}

export const SALES_PRODUCT_KEYS = ['id', 'sku', 'name', 'tax_bps'] as const;
export const SALES_PRODUCT_POINT_KEYS = ['id', 'version'] as const;
export const SALES_COUNT_KEYS = ['party_id', 'count'] as const;
export const SALES_OPEN_ORDER_KEYS = [
  'id', 'number', 'kind', 'state', 'due_date', 'currency', 'total_minor', 'version',
] as const;
export const SALES_SUPPORT_KEYS = ['id'] as const;

export type SalesRow = Record<string, string | number | null>;

function isCell(cell: unknown): boolean {
  return cell === null || typeof cell === 'string' ||
    (typeof cell === 'number' && Number.isFinite(cell) && Number.isSafeInteger(cell));
}

function isRow(value: unknown, keys: readonly string[]): value is SalesRow {
  if (!record(value)) return false;
  const names = Object.keys(value);
  if (names.length !== keys.length) return false;
  return names.every(name => keys.includes(name)) && Object.values(value).every(isCell);
}

function isRows(value: unknown, keys: readonly string[], max: number): value is SalesRow[] {
  return Array.isArray(value) && value.length <= max && value.every(row => isRow(row, keys));
}

function envelope(rows: SalesRow[]): SqlResult<SalesRow> {
  return { results: rows, success: true, meta: { changes: 0, duration: 0 } };
}

function responseFailure(status: number, value: unknown): never {
  const error = record(value) && record(value.error) ? value.error : {};
  if (status >= 400 && status < 500) {
    throw new ApiError(
      status,
      typeof error.code === 'string' && error.code.length <= 100 ? error.code : 'COMMERCE_LANE14_SALES_FAILED',
      typeof error.message === 'string' && error.message.length <= 300
        ? error.message
        : 'The native Commerce Sales read was rejected.',
    );
  }
  return fail();
}

async function read(
  actor: Actor,
  route: string,
  recordId: string | undefined,
  partyIds: readonly string[] | undefined,
): Promise<{ status: number; value: unknown }> {
  if (
    !validId(actor.companyId) ||
    (actor.branchId !== null && actor.branchId !== undefined && !validId(actor.branchId)) ||
    (actor.departmentId !== null && actor.departmentId !== undefined && !validId(actor.departmentId)) ||
    (recordId !== undefined && !validId(recordId)) ||
    (partyIds !== undefined && (partyIds.length < 1 || partyIds.length > 64 || !partyIds.every(validId)))
  )
    return fail(RESPONSE_INVALID);
  const cfg = config();
  const holding = select(cfg, actor);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/commerce/lane14/${route}`,
    cfg.base,
  );
  const headers: Record<string, string> = {
    host: holding.host,
    origin: holding.origin,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': actor.companyId,
  };
  if (actor.branchId !== null && actor.branchId !== undefined)
    headers['x-erpx-context-branch'] = actor.branchId;
  if (actor.departmentId !== null && actor.departmentId !== undefined)
    headers['x-erpx-context-department'] = actor.departmentId;
  if (recordId !== undefined) headers['x-erpx-context-record'] = recordId;
  const body = partyIds === undefined ? undefined : JSON.stringify({ partyIds: [...partyIds] });
  if (body !== undefined) {
    headers['content-type'] = 'application/json';
    headers['idempotency-key'] = 'sales-order-counts';
  }
  try {
    return await send(
      body === undefined ? 'GET' : 'POST',
      url,
      headers,
      body,
      cfg.timeoutMs,
    );
  } catch {
    return fail();
  }
}

function rowsReply(value: unknown, keys: readonly string[], max: number): SqlResult<SalesRow> {
  if (!record(value) || !isRows(value.rows, keys, max)) return fail(RESPONSE_INVALID);
  return envelope(value.rows);
}

function rowReply(value: unknown, keys: readonly string[]): SalesRow | null {
  if (!record(value) || !('row' in value)) return fail(RESPONSE_INVALID);
  const row = value.row;
  if (row === null || row === undefined) return null;
  if (!isRow(row, keys)) return fail(RESPONSE_INVALID);
  return row;
}

export async function nativeSalesPricingProducts(
  actor: Actor,
): Promise<SqlResult<SalesRow>> {
  const response = await read(actor, 'sales-pricing-products', undefined, undefined);
  if (response.status !== 200) return responseFailure(response.status, response.value);
  return rowsReply(response.value, SALES_PRODUCT_KEYS, 10_000);
}

export async function nativeSalesPricingProduct(
  actor: Actor,
  id: string,
): Promise<SalesRow | null> {
  const response = await read(actor, 'sales-pricing-product', id, undefined);
  if (response.status !== 200) return responseFailure(response.status, response.value);
  return rowReply(response.value, SALES_PRODUCT_POINT_KEYS);
}

export async function nativeSalesOrderCounts(
  actor: Actor,
  ids: readonly string[],
): Promise<SalesRow[]> {
  const response = await read(actor, 'sales-order-counts', undefined, ids);
  if (response.status !== 200) return responseFailure(response.status, response.value);
  return rowsReply(response.value, SALES_COUNT_KEYS, 64).results;
}

export async function nativeSalesCustomerOpenOrders(
  actor: Actor,
  partyId: string,
): Promise<SalesRow[]> {
  const response = await read(actor, 'sales-customer-open-orders', partyId, undefined);
  if (response.status !== 200) return responseFailure(response.status, response.value);
  return rowsReply(response.value, SALES_OPEN_ORDER_KEYS, 30).results;
}

export async function nativeSalesSupportOrderSource(
  actor: Actor,
  partyId: string,
): Promise<{ id: string } | null> {
  const response = await read(actor, 'sales-support-order-source', partyId, undefined);
  if (response.status !== 200) return responseFailure(response.status, response.value);
  const row = rowReply(response.value, SALES_SUPPORT_KEYS);
  if (row === null) return null;
  return typeof row.id === 'string' ? { id: row.id } : fail(RESPONSE_INVALID);
}
