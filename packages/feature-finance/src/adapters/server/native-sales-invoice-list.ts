import 'server-only';
import {headers} from 'next/headers';
import {
  directHttp,
  NativeFinanceReadError,
  type NativeFinanceReadConfiguration,
  type NativeFinanceReadTransport,
} from './native-journal-detail';
import {nativeFinanceConfiguration, type NativeJournalPageContext} from './native-journal-page';

type Row = Record<string, string | number | null>;
export type NativeSalesInvoiceList = {
  query: {tab: string; q?: string; status?: string; startDate?: string; endDate?: string; currency?: string; queue?: string; cursor?: string};
  summary: Row[];
  received: Row[];
  page: {
    items: Row[];
    nextCursor: string | null;
    previousCursor: string | null;
    hasMore: boolean;
  };
  permissionRequired: string | null;
};

const fail = (code: string, status?: number): never => {
  throw new NativeFinanceReadError(code, status);
};
const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const ledger = (value: unknown): value is Row =>
  object(value) &&
  Object.values(value).every(
    (item) =>
      item === null ||
      typeof item === 'string' ||
      (typeof item === 'number' && Number.isSafeInteger(item)),
  );
const rows = (value: unknown, max = 10000): value is Row[] =>
  Array.isArray(value) && value.length <= max && value.every(ledger);
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};
const text = (value: unknown, max = 200): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= max;

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

function invoiceRow(value: unknown): Row {
  if (
    !ledger(value) ||
    !text(value.id) ||
    typeof value.number !== 'string' ||
    typeof value.date !== 'string' ||
    typeof value.status !== 'string' ||
    typeof value.currency !== 'string' ||
    typeof value.party_name !== 'string' ||
    typeof value.page_key !== 'string'
  ) {
    return fail('NATIVE_FINANCE_SALES_RESPONSE_INVALID');
  }
  return value;
}

function summaryRow(value: unknown, kind: 'summary' | 'received'): Row {
  if (
    !ledger(value) ||
    typeof value.currency !== 'string' ||
    !integer(kind === 'summary' ? value.awaiting_minor : value.received_minor)
  ) {
    return fail('NATIVE_FINANCE_SALES_RESPONSE_INVALID');
  }
  if (kind === 'summary' && !integer(value.overdue_minor)) {
    return fail('NATIVE_FINANCE_SALES_RESPONSE_INVALID');
  }
  return value;
}

function validate(
  value: unknown,
  actor: NativeJournalPageContext,
  branchSlug: string | undefined,
  departmentSlug: string | undefined,
): NativeSalesInvoiceList {
  if (!object(value) || !object(value.context) || !object(value.data)) {
    return fail('NATIVE_FINANCE_SALES_RESPONSE_INVALID');
  }
  const context = value.context;
  const data = value.data;
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
  if (
    !object(data.query) ||
    data.query.tab !== 'invoices' ||
    !rows(data.summary) ||
    !rows(data.received) ||
    !object(data.page) ||
    !rows(data.page.items, 30) ||
    typeof data.page.hasMore !== 'boolean' ||
    (data.page.nextCursor !== null && !text(data.page.nextCursor, 4096)) ||
    (data.page.previousCursor !== null && !text(data.page.previousCursor, 4096)) ||
    (data.permissionRequired !== null && typeof data.permissionRequired !== 'string')
  ) {
    return fail('NATIVE_FINANCE_SALES_RESPONSE_INVALID');
  }
  return {
    query: data.query as NativeSalesInvoiceList['query'],
    summary: data.summary.map((row) => summaryRow(row, 'summary')),
    received: data.received.map((row) => summaryRow(row, 'received')),
    page: {
      items: data.page.items.map(invoiceRow),
      nextCursor: data.page.nextCursor as string | null,
      previousCursor: data.page.previousCursor as string | null,
      hasMore: data.page.hasMore,
    },
    permissionRequired: data.permissionRequired as string | null,
  };
}

export function createNativeSalesInvoiceListReader(
  raw: string,
  transport: NativeFinanceReadTransport = directHttp,
) {
  const config = configuration(raw);
  return async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeJournalPageContext,
    holdingSlug: string,
    query: {tab?: string; q?: string; status?: string; startDate?: string; endDate?: string; currency?: string; queue?: string; cursor?: string},
  ): Promise<NativeSalesInvoiceList> => {
    if (!actor.authority || !actor.iam) {
      return fail('NATIVE_FINANCE_CONTEXT_CHANGED');
    }
    const host = incoming.get('host');
    const holding = config.holdings.find(
      (entry) => new URL(entry.origin).host === host && entry.holdingSlug === holdingSlug,
    );
    if (!holding || holding.holdingId !== actor.holdingId) {
      return fail('NATIVE_FINANCE_AUTHORITY_INVALID');
    }
    if (actor.holdingSlug !== holdingSlug) {
      return fail('NATIVE_FINANCE_CONTEXT_CHANGED');
    }
    const branchSlug = actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug;
    const departmentSlug =
      actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined;
    const out = new Headers({host: host!, accept: 'application/json', 'cache-control': 'no-cache'});
    const cookie = incoming.get('cookie');
    if (cookie) {
      if (new TextEncoder().encode(cookie).length > 8192) {
        return fail('NATIVE_FINANCE_COOKIE_LIMIT');
      }
      out.set('cookie', cookie);
    }
    const origin = incoming.get('origin');
    if (origin) out.set('origin', origin);
    if (branchSlug) out.set('x-erpx-workspace-branch', branchSlug);
    if (departmentSlug) out.set('x-erpx-workspace-department', departmentSlug);
    out.set('x-erpx-context-company', actor.companyId);
    out.set('x-erpx-context-assignment', actor.assignmentId);
    out.set('x-erpx-context-revision', String(actor.authority.revision));
    out.set('x-erpx-context-assignment-version', String(actor.authority.assignmentVersion));
    const url = new URL(
      `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/sales/invoices`,
      config.privateBaseUrl,
    );
    url.searchParams.set('tab', query.tab ?? 'invoices');
    if (query.q) url.searchParams.set('q', query.q);
    if (query.status) url.searchParams.set('status', query.status);
    if (query.startDate) url.searchParams.set('startDate', query.startDate);
    if (query.endDate) url.searchParams.set('endDate', query.endDate);
    if (query.currency) url.searchParams.set('currency', query.currency);
    if (query.queue) url.searchParams.set('queue', query.queue);
    if (query.cursor) url.searchParams.set('cursor', query.cursor);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeoutMs);
    try {
      const response = await transport(url, out, controller.signal);
      if (response.redirected || response.status !== 200) {
        void response.body?.cancel().catch(() => undefined);
        return fail('NATIVE_FINANCE_SALES_UNAVAILABLE', response.status);
      }
      if (response.headers.get('content-type')?.split(';')[0] !== 'application/json') {
        return fail('NATIVE_FINANCE_SALES_RESPONSE_INVALID');
      }
      return validate(await response.json(), actor, branchSlug, departmentSlug);
    } catch (error) {
      if (error instanceof NativeFinanceReadError) throw error;
      return fail(
        controller.signal.aborted ? 'NATIVE_FINANCE_SALES_ABORTED' : 'NATIVE_FINANCE_SALES_UNAVAILABLE',
      );
    } finally {
      clearTimeout(timer);
    }
  };
}

export async function nativeSalesInvoiceList(
  actor: NativeJournalPageContext | undefined,
  holdingSlug: string,
  query: {tab?: string; q?: string; status?: string; startDate?: string; endDate?: string; currency?: string; queue?: string; cursor?: string},
): Promise<NativeSalesInvoiceList | undefined> {
  const raw = process.env.ERPX_NATIVE_FINANCE_JOURNAL_CONFIG;
  if (raw === undefined) return undefined;
  if (!actor) return fail('NATIVE_FINANCE_CONTEXT_CHANGED');
  return createNativeSalesInvoiceListReader(raw)(await headers(), actor, holdingSlug, query);
}
