import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

/**
 * Strict, fail-closed transport for the native Finance overview-counts read.
 *
 * No SQL is authored here and no SQL is accepted from a caller. The Rust owner
 * executes the statement and projects the seven bounded counts; a missing
 * configuration, a changed WorkContext or any malformed reply throws an
 * explicit ApiError. There is no TypeScript fallback.
 */
export const NATIVE_FINANCE_LANE_2_CONFIG_KEY = 'ERPX_NATIVE_FINANCE_LANE_2_CONFIG';
const BODY_LIMIT = 2 * 1024 * 1024;
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

export type OverviewCounts = Readonly<{
  journals: number;
  posting: number;
  invoices: number;
  payments: number;
  receivable: number;
  payable: number;
  reconcile: number;
}>;

type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Config = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;

const COUNT_KEYS = [
  'journals',
  'posting',
  'invoices',
  'payments',
  'receivable',
  'payable',
  'reconcile',
] as const;

export const fail = (status = 503): never => {
  throw new ApiError(
    status,
    'FINANCE_NATIVE_LANE_2_UNAVAILABLE',
    'The native Finance overview read is unavailable.',
  );
};

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const text = (value: unknown, max = 512): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;

export function nativeLane2Config(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): Config {
  const raw = environment[NATIVE_FINANCE_LANE_2_CONFIG_KEY];
  if (raw === undefined) return fail();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail();
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl, 512) ||
    typeof value.timeoutMs !== 'number' ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return fail();
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail();
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
    return fail();

  const holdings: Holding[] = [];
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !text(item.holdingId, 200) ||
      !text(item.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(String(item.holdingSlug)) ||
      (item.companyId !== undefined &&
        (!text(item.companyId, 200) || /[\u0000-\u001f/?#\\]/.test(String(item.companyId))))
    )
      return fail();
    let origin: URL;
    try {
      origin = new URL(String(item.origin));
    } catch {
      return fail();
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password
    )
      return fail();
    holdings.push({
      origin: origin.origin,
      host: String(item.host),
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
      ...(item.companyId === undefined ? {} : { companyId: String(item.companyId) }),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

const validIdentifier = (value: string) =>
  value.length > 0 &&
  value.length <= 200 &&
  !/[\u0000-\u001f/?#\\]/.test(value);

async function readBody(body: ReadableStream<Uint8Array>): Promise<string> {
  const reader = body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > BODY_LIMIT) {
        await reader.cancel();
        return fail(502);
      }
      chunks.push(next.value);
    }
  } finally {
    reader.releaseLock();
  }
  const data = new Uint8Array(bytes);
  let offset = 0;
  for (const chunk of chunks) {
    data.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return new TextDecoder('utf-8', { fatal: true }).decode(data);
}

const parseCounts = (payload: unknown): OverviewCounts => {
  if (!record(payload) || !record(payload.data)) return fail(502);
  const data = payload.data;
  for (const key of COUNT_KEYS) {
    const count = data[key];
    if (typeof count !== 'number' || !Number.isSafeInteger(count) || count < 0) return fail(502);
  }
  return {
    journals: data.journals as number,
    posting: data.posting as number,
    invoices: data.invoices as number,
    payments: data.payments as number,
    receivable: data.receivable as number,
    payable: data.payable as number,
    reconcile: data.reconcile as number,
  };
};

/** Read the owner-owned overview counts for one bounded Company. */
export async function financeOverviewCountsNative(
  actor: Pick<WorkContext, 'holdingId' | 'holdingSlug' | 'companyId'>,
): Promise<OverviewCounts> {
  const config = nativeLane2Config();
  if (!validIdentifier(actor.companyId)) return fail(422);
  // A holding is selected only by the caller's exact WorkContext pair. There is
  // no first-holding default: an unregistered pair fails closed.
  const target = config.holdings.find(
    (holding) => holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
  );
  if (!target) throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'The Holding context changed.');
  if (target.companyId !== undefined && target.companyId !== actor.companyId)
    throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'The Holding company scope changed.');

  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/finance/native-lane-2/overview-counts`,
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
        'x-erpx-context-company': actor.companyId,
      },
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    return fail();
  }
  if (response.redirected || response.status !== 200 || !response.body) {
    void response.body?.cancel().catch(() => undefined);
    return fail(response.status);
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body.cancel().catch(() => undefined);
    return fail(502);
  }
  try {
    return parseCounts(JSON.parse(await readBody(response.body)));
  } catch {
    return fail(502);
  }
}

/** True when the native lane consumer switch is configured. */
export function nativeLane2Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[NATIVE_FINANCE_LANE_2_CONFIG_KEY] !== undefined;
}
