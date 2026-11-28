import 'server-only';
import {headers} from 'next/headers';
import {
  directHttp,
  NativeFinanceReadError,
  type NativeFinanceReadConfiguration,
  type NativeFinanceReadTransport,
} from './native-journal-detail';
import {nativeFinanceConfiguration, type NativeJournalPageContext} from './native-journal-page';

type Row = Record<string, string | number | boolean | null>;
export type NativeInvoicePage = {
  items: Row[];
  nextCursor: string | null;
  previousCursor: string | null;
  hasMore: boolean;
};
export type NativeInvoiceAgingRow = Row & {
  bucket: string;
  currency: string;
  outstanding_minor: number;
  due_date: string;
};

const fail = (code: string, status?: number): never => {
  throw new NativeFinanceReadError(code, status);
};
const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 200): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= max;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const ledger = (value: unknown): value is Row =>
  object(value) &&
  Object.values(value).every(
    (item) =>
      item === null ||
      typeof item === 'string' ||
      typeof item === 'boolean' ||
      (typeof item === 'number' && Number.isSafeInteger(item)),
  );
const rows = (value: unknown, max = 10000): value is Row[] =>
  Array.isArray(value) && value.length <= max && value.every(ledger);
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};

function configuration(raw: string): NativeFinanceReadConfiguration {
  const config = nativeFinanceConfiguration(raw);
  let base: URL;
  try {
    base = new URL(config.privateBaseUrl);
  } catch {
    return fail('NATIVE_FINANCE_CONFIGURATION_INVALID');
  }
  const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reserved.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    config.privateBaseUrl !== base.origin ||
    !integer(config.timeoutMs, 250) ||
    config.timeoutMs > 30000
  ) {
    return fail('NATIVE_FINANCE_CONFIGURATION_INVALID');
  }
  return config;
}

function validateContext(
  value: unknown,
  actor: NativeJournalPageContext,
  branchSlug: string | undefined,
  departmentSlug: string | undefined,
): asserts value is Record<string, unknown> {
  if (!object(value) || !object(value.context)) {
    return fail('NATIVE_FINANCE_INVOICE_RESPONSE_INVALID');
  }
  const context = value.context;
  if (
    context.userId !== actor.userId ||
    context.holdingId !== actor.holdingId ||
    context.companyId !== actor.companyId ||
    context.branchId !== actor.branchId ||
    context.departmentId !== actor.departmentId ||
    context.branchSlug !== (branchSlug ?? null) ||
    context.departmentSlug !== (departmentSlug ?? null) ||
    context.assignmentId !== actor.assignmentId ||
    context.assignmentVersion !== actor.authority?.assignmentVersion ||
    context.revision !== actor.authority?.revision ||
    !integer(context.expiresAt, 1) ||
    !sameSet(actor.modules, context.modules as string[]) ||
    !sameSet(actor.iam?.permissions ?? [], context.permissions as string[])
  ) {
    return fail('NATIVE_FINANCE_CONTEXT_CHANGED');
  }
}

function requestHeaders(
  incoming: Pick<Headers, 'get'>,
  actor: NativeJournalPageContext,
  branchSlug: string | undefined,
  departmentSlug: string | undefined,
): Headers {
  const out = new Headers({accept: 'application/json', 'cache-control': 'no-cache'});
  const host = incoming.get('host');
  if (host) out.set('host', host);
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192) {
      return fail('NATIVE_FINANCE_COOKIE_LIMIT');
    }
    out.set('cookie', cookie);
  }
  const origin = incoming.get('origin');
  if (origin) out.set('origin', origin);
  out.set('x-erpx-context-company', actor.companyId);
  out.set('x-erpx-context-assignment', actor.assignmentId);
  out.set('x-erpx-context-revision', String(actor.authority!.revision));
  out.set('x-erpx-context-assignment-version', String(actor.authority!.assignmentVersion));
  if (branchSlug) out.set('x-erpx-workspace-branch', branchSlug);
  if (departmentSlug) out.set('x-erpx-workspace-department', departmentSlug);
  return out;
}

function page(value: unknown, actor: NativeJournalPageContext, branchSlug: string | undefined, departmentSlug: string | undefined): NativeInvoicePage {
  validateContext(value, actor, branchSlug, departmentSlug);
  const data = value.data;
  if (
    !object(data) ||
    !rows(data.items, 30) ||
    typeof data.hasMore !== 'boolean' ||
    (data.nextCursor !== null && !text(data.nextCursor, 4096)) ||
    (data.previousCursor !== null && !text(data.previousCursor, 4096))
  ) {
    return fail('NATIVE_FINANCE_INVOICE_RESPONSE_INVALID');
  }
  return {
    items: data.items,
    nextCursor: data.nextCursor as string | null,
    previousCursor: data.previousCursor as string | null,
    hasMore: data.hasMore,
  };
}

function aging(
  value: unknown,
  actor: NativeJournalPageContext,
  branchSlug: string | undefined,
  departmentSlug: string | undefined,
): NativeInvoiceAgingRow[] {
  validateContext(value, actor, branchSlug, departmentSlug);
  if (!rows(value.data)) {
    return fail('NATIVE_FINANCE_INVOICE_RESPONSE_INVALID');
  }
  return value.data.map((row) => {
    if (
      typeof row.bucket !== 'string' ||
      typeof row.currency !== 'string' ||
      typeof row.outstanding_minor !== 'number' ||
      typeof row.due_date !== 'string'
    ) {
      return fail('NATIVE_FINANCE_INVOICE_RESPONSE_INVALID');
    }
    return row as NativeInvoiceAgingRow;
  });
}

export function createNativeInvoiceReader(
  raw: string,
  transport: NativeFinanceReadTransport = directHttp,
) {
  const config = configuration(raw);
  async function request(
    incoming: Pick<Headers, 'get'>,
    actor: NativeJournalPageContext,
    holdingSlug: string,
    path: string,
    query: URLSearchParams,
  ): Promise<unknown> {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== holdingSlug) {
      return fail('NATIVE_FINANCE_CONTEXT_CHANGED');
    }
    const host = incoming.get('host');
    const holding = config.holdings.find(
      (entry) => new URL(entry.origin).host === host && entry.holdingSlug === holdingSlug,
    );
    if (!holding || holding.holdingId !== actor.holdingId) {
      return fail('NATIVE_FINANCE_AUTHORITY_INVALID');
    }
    const branchSlug = actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug;
    const departmentSlug =
      actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined;
    const out = requestHeaders(incoming, actor, branchSlug, departmentSlug);
    const url = new URL(
      `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/invoices${path}`,
      config.privateBaseUrl,
    );
    for (const [key, value] of query) url.searchParams.set(key, value);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeoutMs);
    try {
      const response = await transport(url, out, controller.signal);
      if (response.redirected || response.status !== 200) {
        void response.body?.cancel().catch(() => undefined);
        return fail('NATIVE_FINANCE_INVOICE_UNAVAILABLE', response.status);
      }
      if (response.headers.get('content-type')?.split(';')[0] !== 'application/json') {
        return fail('NATIVE_FINANCE_INVOICE_RESPONSE_INVALID');
      }
      return await response.json();
    } catch (error) {
      if (error instanceof NativeFinanceReadError) throw error;
      return fail(
        controller.signal.aborted
          ? 'NATIVE_FINANCE_INVOICE_ABORTED'
          : 'NATIVE_FINANCE_INVOICE_UNAVAILABLE',
      );
    } finally {
      clearTimeout(timer);
    }
  }
  return {
    page: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativeJournalPageContext,
      holdingSlug: string,
      query: {q?: string; kind?: string; status?: string; queue?: string; cursor?: string; chartId?: string},
    ): Promise<NativeInvoicePage> => {
      const params = new URLSearchParams();
      if (query.q) params.set('q', query.q);
      if (query.kind) params.set('kind', query.kind);
      if (query.status) params.set('status', query.status);
      if (query.queue) params.set('queue', query.queue);
      if (query.cursor) params.set('cursor', query.cursor);
      if (query.chartId) params.set('chartId', query.chartId);
      const value = await request(incoming, actor, holdingSlug, '', params);
      const branchSlug = actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug;
      const departmentSlug =
        actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined;
      return page(value, actor, branchSlug, departmentSlug);
    },
    aging: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativeJournalPageContext,
      holdingSlug: string,
      kind: 'receivable' | 'payable',
      asOf: string,
      chartId?: string,
    ): Promise<NativeInvoiceAgingRow[]> => {
      const params = new URLSearchParams({kind, asOf});
      if (chartId) params.set('chartId', chartId);
      const value = await request(incoming, actor, holdingSlug, '/aging', params);
      const branchSlug = actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug;
      const departmentSlug =
        actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined;
      return aging(value, actor, branchSlug, departmentSlug);
    },
  };
}

export async function nativeInvoicesPage(
  actor: NativeJournalPageContext | undefined,
  holdingSlug: string,
  query: {q?: string; kind?: string; status?: string; queue?: string; cursor?: string; chartId?: string},
): Promise<NativeInvoicePage | undefined> {
  const raw = process.env.ERPX_NATIVE_FINANCE_INVOICES_READ_CONFIG;
  if (raw === undefined) return undefined;
  if (!actor) return fail('NATIVE_FINANCE_CONTEXT_CHANGED');
  if (actor.scope.kind !== 'holding') return undefined;
  return createNativeInvoiceReader(raw).page(await headers(), actor, holdingSlug, query);
}

export async function nativeInvoiceAging(
  actor: NativeJournalPageContext | undefined,
  holdingSlug: string,
  kind: 'receivable' | 'payable',
  asOf: string,
  chartId?: string,
): Promise<NativeInvoiceAgingRow[] | undefined> {
  const raw = process.env.ERPX_NATIVE_FINANCE_INVOICES_READ_CONFIG;
  if (raw === undefined) return undefined;
  if (!actor) return fail('NATIVE_FINANCE_CONTEXT_CHANGED');
  if (actor.scope.kind !== 'holding') return undefined;
  return createNativeInvoiceReader(raw).aging(await headers(), actor, holdingSlug, kind, asOf, chartId);
}
