import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import { nativeRequest } from '@erpx/platform-runtime-server/public/native-http';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';

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

export type InvoicePageQuery = {
  q?: string;
  kind?: string;
  status?: string;
  queue?: string;
  cursor?: string;
  chartId?: string;
};
export type InvoicePage = {
  items: Array<Record<string, string | number | null> & { id: string; page_key: string }>;
  nextCursor: string | null;
  previousCursor: string | null;
  hasMore: boolean;
};
export type AgingRow = Record<string, string | number | null | boolean> & {
  id: string;
  source_id: string | null;
  legacy_date_evidence: boolean;
  due_date: string;
};

type NativeActor = Pick<WorkContext, 'holdingId' | 'holdingSlug' | 'companyId'>;

function selectTarget(actor: NativeActor) {
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
  return { config, target };
}

async function nativeGet(
  actor: NativeActor,
  route: string,
  params: Record<string, string | undefined> = {},
): Promise<unknown> {
  const { config, target } = selectTarget(actor);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/finance/native-lane-2/${route}`,
    config.base,
  );
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined && value !== '') url.searchParams.set(key, value);
  }
  let response: Response;
  try {
    response = await nativeRequest(
      url,
      {
        method: 'GET',
        headers: {
          host: target.host,
          origin: target.origin,
          accept: 'application/json',
          'cache-control': 'no-cache',
          'x-erpx-context-company': actor.companyId,
        },
        signal: AbortSignal.timeout(config.timeoutMs),
      },
      config.timeoutMs,
    );
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
    return JSON.parse(await readBody(response.body));
  } catch {
    return fail(502);
  }
}

const parsePage = (payload: unknown): InvoicePage => {
  if (!record(payload) || !record(payload.data)) return fail(502);
  const data = payload.data;
  if (!Array.isArray(data.items)) return fail(502);
  const items = data.items.map((item: unknown) => {
    if (!record(item) || typeof item.id !== 'string' || typeof item.page_key !== 'string')
      return fail(502);
    if (!Object.values(item).every((value) =>
      value === null || typeof value === 'string' || typeof value === 'number'))
      return fail(502);
    return item as Record<string, string | number | null> & { id: string; page_key: string };
  });
  if (
    !(data.nextCursor === null || typeof data.nextCursor === 'string') ||
    !(data.previousCursor === null || typeof data.previousCursor === 'string') ||
    typeof data.hasMore !== 'boolean'
  )
    return fail(502);
  return {
    items,
    nextCursor: data.nextCursor as string | null,
    previousCursor: data.previousCursor as string | null,
    hasMore: data.hasMore,
  };
};

const parseAging = (payload: unknown): AgingRow[] => {
  if (!record(payload) || !Array.isArray(payload.data)) return fail(502);
  return payload.data.map((row: unknown) => {
    if (
      !record(row) ||
      typeof row.id !== 'string' ||
      typeof row.due_date !== 'string' ||
      typeof row.historical_outstanding_minor !== 'number' ||
      typeof row.historical_base_outstanding_minor !== 'number' ||
      typeof row.legacy_date_evidence !== 'boolean'
    )
      return fail(502);
    return row as AgingRow;
  });
};

const validAgingDate = (value: string) => {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const year = Number(value.slice(0, 4));
  const month = Number(value.slice(5, 7));
  const day = Number(value.slice(8, 10));
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  return month >= 1 && month <= 12 && day >= 1 && day <= days[month - 1]!;
};

/** Read the owner-owned overview counts for one bounded Company. */
export async function financeOverviewCountsNative(actor: NativeActor): Promise<OverviewCounts> {
  return parseCounts(await nativeGet(actor, 'overview-counts'));
}

/** Read the owner-owned, Company-scoped invoice list page. */
export async function financeInvoicesPageNative(
  actor: NativeActor,
  query: InvoicePageQuery = {},
): Promise<InvoicePage> {
  const kind = query.kind ?? '';
  const queue = query.queue ?? '';
  if (
    !['', 'receivable', 'payable'].includes(kind) ||
    !['', 'open'].includes(queue) ||
    (query.q?.length ?? 0) > 200 ||
    (query.status?.length ?? 0) > 40 ||
    (query.chartId !== undefined && (query.chartId.length === 0 || query.chartId.length > 200)) ||
    (query.cursor !== undefined && (query.cursor.length === 0 || query.cursor.length > 4096))
  )
    return fail(422);
  return parsePage(await nativeGet(actor, 'invoices-page', {
    q: query.q,
    kind: query.kind,
    status: query.status,
    queue: query.queue,
    chartId: query.chartId,
    cursor: query.cursor,
  }));
}

/** Read the owner-owned, Company-scoped historical aging rows. */
export async function financeAgingBalancesNative(
  actor: NativeActor,
  asOf: string,
  kind: 'receivable' | 'payable',
  chartId?: string,
): Promise<AgingRow[]> {
  if (
    !['receivable', 'payable'].includes(kind) ||
    !validAgingDate(asOf) ||
    (chartId !== undefined && (chartId.length === 0 || chartId.length > 200))
  )
    return fail(422);
  return parseAging(await nativeGet(actor, 'aging', { kind, asOf, chartId }));
}

/** True when the native lane consumer switch is configured. */
export function nativeLane2Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[NATIVE_FINANCE_LANE_2_CONFIG_KEY] !== undefined;
}
