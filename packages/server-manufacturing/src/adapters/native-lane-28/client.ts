import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';

/**
 * Strict private transport for the native Manufacturing lane-28 initial-stock
 * history read. The axum owner executes the EXISTS guard with sqlx and returns
 * the boolean fact; this bridge emits no SQL, accepts no SQL from a caller and
 * fails closed when the native owner is unavailable or malformed. There is no
 * TypeScript fallback.
 *
 * Holdings are the exact registered development targets. A Holding is selected
 * by the caller's WorkContext (holdingId + holdingSlug, with company
 * cross-check) or by an explicit company identity. There is no first-holding
 * default: an unresolvable context fails closed.
 */
type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Settings = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;

export type Lane28WorkContext = Readonly<{
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;

export type PostingReferencesFacts = Readonly<{
  journalIds: readonly string[];
  orderIds: readonly string[];
  invalid: boolean;
  image: string;
}>;

const ENV = 'ERPX_NATIVE_MANUFACTURING_LANE28_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const reject = (status: number, code: string, message: string): never => {
  throw new ApiError(status, code, message);
};
const unavailable = (
  code = 'MANUFACTURING_LANE28_UNAVAILABLE',
  message = 'The native Manufacturing stock-history read service is unavailable.',
): never => reject(503, code, message);
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const identifier = (value: unknown): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= 200;

function configuration(): Settings {
  const raw = process.env[ENV];
  if (!raw) return unavailable('MANUFACTURING_LANE28_CONFIGURATION_REQUIRED');
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable('MANUFACTURING_LANE28_CONFIGURATION_INVALID');
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
    return unavailable('MANUFACTURING_LANE28_CONFIGURATION_INVALID');
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable('MANUFACTURING_LANE28_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl
  )
    return unavailable('MANUFACTURING_LANE28_CONFIGURATION_INVALID');
  const holdings: Holding[] = [];
  const seen = new Set<string>();
  for (const item of value.holdings) {
    if (
      !record(item) ||
      typeof item.origin !== 'string' ||
      typeof item.host !== 'string' ||
      !identifier(item.holdingId) ||
      typeof item.holdingSlug !== 'string' ||
      !/^[a-z0-9_-]{1,128}$/.test(item.holdingSlug) ||
      (item.companyId !== undefined && !identifier(item.companyId))
    )
      return unavailable('MANUFACTURING_LANE28_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return unavailable('MANUFACTURING_LANE28_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== item.origin ||
      origin.host !== item.host ||
      origin.username ||
      origin.password
    )
      return unavailable('MANUFACTURING_LANE28_CONFIGURATION_INVALID');
    const key = `${item.holdingId}\u0000${item.holdingSlug}`;
    if (seen.has(key)) return unavailable('MANUFACTURING_LANE28_CONFIGURATION_INVALID');
    seen.add(key);
    holdings.push({
      origin: origin.origin,
      host: item.host,
      holdingId: item.holdingId,
      holdingSlug: item.holdingSlug,
      companyId: item.companyId as string | undefined,
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

function selectHolding(config: Settings, actor?: Lane28WorkContext, companyId?: string): Holding {
  if (actor) {
    const target = config.holdings.find(
      (holding) => holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
    );
    if (!target) return reject(409, 'WORK_CONTEXT_CHANGED', 'Choose a current Manufacturing Holding.');
    if (actor.companyId && target.companyId && actor.companyId !== target.companyId)
      return reject(409, 'WORK_CONTEXT_CHANGED', 'The Manufacturing company context changed.');
    if (companyId && target.companyId && companyId !== target.companyId)
      return reject(409, 'WORK_CONTEXT_CHANGED', 'The Manufacturing company scope changed.');
    return target;
  }
  if (companyId) {
    const target = config.holdings.find((holding) => holding.companyId === companyId);
    if (!target) return reject(409, 'WORK_CONTEXT_CHANGED', 'Use a registered Holding for this company.');
    return target;
  }
  return reject(409, 'WORK_CONTEXT_REQUIRED', 'A WorkContext or company identity is required.');
}

async function call(
  config: Settings,
  target: Holding,
  path: string,
  headers: Readonly<Record<string, string>>,
): Promise<unknown> {
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/manufacturing/lane28/${path}`,
    config.base,
  );
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'GET',
      headers: {
        host: target.host,
        origin: target.origin,
        accept: 'application/json',
        'cache-control': 'no-cache',
        ...headers,
      },
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    return unavailable();
  }
  if (response.status !== 200) {
    const payload = await response.json().catch((): unknown => null);
    const error = record(payload) && record(payload.error) ? payload.error : undefined;
    if (response.status >= 400 && response.status < 500)
      throw new ApiError(
        response.status,
        typeof error?.code === 'string' ? error.code : 'MANUFACTURING_LANE28_READ_FAILED',
        typeof error?.message === 'string' ? error.message : 'The native Manufacturing read was rejected.',
      );
    return unavailable();
  }
  return await response.json().catch((): unknown => null);
}

/** Read the owner-local initial-stock history fact. Holding selection uses the
 *  caller's WorkContext; no SQL leaves this transport. */
export async function nativeManufacturingLane28StockHistory(
  companyId: string,
  warehouseId: string,
  productId: string,
  actor?: Lane28WorkContext,
): Promise<boolean> {
  for (const [name, value] of [['company', companyId], ['warehouse', warehouseId], ['product', productId]] as const) {
    if (!identifier(value)) return unavailable(`MANUFACTURING_LANE28_${name.toUpperCase()}_INVALID`, `Use one bounded Manufacturing ${name} identity.`);
  }
  const config = configuration();
  const target = selectHolding(config, actor, companyId);
  const payload = await call(config, target, 'initial-stock-history', {
    'x-erpx-context-company': companyId,
    'x-erpx-context-warehouse': warehouseId,
    'x-erpx-context-product': productId,
  });
  if (!record(payload) || !record(payload.data) || typeof payload.data.history !== 'boolean')
    return unavailable('MANUFACTURING_LANE28_RESPONSE_INVALID');
  return payload.data.history;
}

/** Read the owner-local existing-posting economic references. The Rust owner
 *  executes the SQL and computes ids/witness/image; this transport validates
 *  the exact response shape and emits no SQL. */
export async function nativeManufacturingLane28PostingReferences(
  companyId: string,
  actor?: Lane28WorkContext,
): Promise<PostingReferencesFacts> {
  if (!identifier(companyId))
    return unavailable('MANUFACTURING_LANE28_COMPANY_INVALID', 'Use one bounded Manufacturing company identity.');
  const config = configuration();
  const target = selectHolding(config, actor, companyId);
  const payload = await call(config, target, 'existing-posting-references', {
    'x-erpx-context-company': companyId,
  });
  if (
    !record(payload) ||
    !record(payload.data) ||
    !Array.isArray(payload.data.journalIds) ||
    !Array.isArray(payload.data.orderIds) ||
    typeof payload.data.invalid !== 'boolean' ||
    typeof payload.data.image !== 'string'
  )
    return unavailable('MANUFACTURING_LANE28_RESPONSE_INVALID');
  for (const value of [...payload.data.journalIds, ...payload.data.orderIds]) {
    if (!identifier(value)) return unavailable('MANUFACTURING_LANE28_RESPONSE_INVALID');
  }
  return {
    journalIds: payload.data.journalIds as readonly string[],
    orderIds: payload.data.orderIds as readonly string[],
    invalid: payload.data.invalid,
    image: payload.data.image,
  };
}

/** True when the native lane-28 owner transport is configured. */
export function nativeManufacturingLane28Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
