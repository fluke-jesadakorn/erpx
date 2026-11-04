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
export type NativeFinancialReport = {
  dates: {startDate: string | null; endDate: string | null};
  balanceSheet: {assets: string; liabilities: string; equity: string; recordedEquity: string; unclosedProfit: string};
  profitAndLoss: {revenue: string; expenses: string; otherIncome: string; otherExpenses: string; netProfit: string};
  trialBalance: Row[];
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
  if (!object(value) || !object(value.context)) return fail('NATIVE_FINANCE_REPORT_RESPONSE_INVALID');
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

const money = (value: unknown): value is string => typeof value === 'string' && /^-?(0|[1-9][0-9]*)\.[0-9]{2}$/.test(value);

export function createNativeFinanceReportReader(
  raw: string,
  transport: NativeFinanceReadTransport = directHttp,
) {
  const config = configuration(raw);
  return async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeJournalPageContext,
    holdingSlug: string,
    query: {chartId?: string; periodId?: string; startDate?: string; endDate?: string},
  ): Promise<NativeFinancialReport> => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== holdingSlug) {
      return fail('NATIVE_FINANCE_CONTEXT_CHANGED');
    }
    const host = incoming.get('host');
    const holding = config.holdings.find(
      (entry) => new URL(entry.origin).host === host && entry.holdingSlug === holdingSlug,
    );
    if (!holding || holding.holdingId !== actor.holdingId) return fail('NATIVE_FINANCE_AUTHORITY_INVALID');
    const branchSlug = actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug;
    const departmentSlug = actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined;
    const out = new Headers({accept: 'application/json', 'cache-control': 'no-cache'});
    if (host) out.set('host', host);
    const cookie = incoming.get('cookie');
    if (cookie) {
      if (new TextEncoder().encode(cookie).length > 8192) return fail('NATIVE_FINANCE_COOKIE_LIMIT');
      out.set('cookie', cookie);
    }
    const origin = incoming.get('origin');
    if (origin) out.set('origin', origin);
    out.set('x-erpx-context-company', actor.companyId);
    out.set('x-erpx-context-assignment', actor.assignmentId);
    out.set('x-erpx-context-revision', String(actor.authority.revision));
    out.set('x-erpx-context-assignment-version', String(actor.authority.assignmentVersion));
    if (branchSlug) out.set('x-erpx-workspace-branch', branchSlug);
    if (departmentSlug) out.set('x-erpx-workspace-department', departmentSlug);
    const url = new URL(
      `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/reports`,
      config.privateBaseUrl,
    );
    if (query.chartId) url.searchParams.set('chart', query.chartId);
    if (query.periodId) url.searchParams.set('periodId', query.periodId);
    if (query.startDate) url.searchParams.set('startDate', query.startDate);
    if (query.endDate) url.searchParams.set('endDate', query.endDate);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeoutMs);
    try {
      const response = await transport(url, out, controller.signal);
      if (response.redirected || response.status !== 200) {
        void response.body?.cancel().catch(() => undefined);
        return fail('NATIVE_FINANCE_REPORT_UNAVAILABLE', response.status);
      }
      if (response.headers.get('content-type')?.split(';')[0] !== 'application/json') {
        return fail('NATIVE_FINANCE_REPORT_RESPONSE_INVALID');
      }
      const value: unknown = await response.json();
      validateContext(value, actor, branchSlug, departmentSlug);
      const data = value.data;
      if (
        !object(data) ||
        !object(data.dates) ||
        !object(data.balanceSheet) ||
        !object(data.profitAndLoss) ||
        !rows(data.trialBalance) ||
        !money(data.balanceSheet.assets) ||
        !money(data.balanceSheet.liabilities) ||
        !money(data.balanceSheet.equity) ||
        !money(data.balanceSheet.recordedEquity) ||
        !money(data.balanceSheet.unclosedProfit) ||
        !money(data.profitAndLoss.revenue) ||
        !money(data.profitAndLoss.expenses) ||
        !money(data.profitAndLoss.otherIncome) ||
        !money(data.profitAndLoss.otherExpenses) ||
        !money(data.profitAndLoss.netProfit)
      ) {
        return fail('NATIVE_FINANCE_REPORT_RESPONSE_INVALID');
      }
      return {
        dates: {
          startDate: data.dates.startDate as string | null,
          endDate: data.dates.endDate as string | null,
        },
        balanceSheet: data.balanceSheet as NativeFinancialReport['balanceSheet'],
        profitAndLoss: data.profitAndLoss as NativeFinancialReport['profitAndLoss'],
        trialBalance: data.trialBalance,
      };
    } catch (error) {
      if (error instanceof NativeFinanceReadError) throw error;
      return fail(
        controller.signal.aborted ? 'NATIVE_FINANCE_REPORT_ABORTED' : 'NATIVE_FINANCE_REPORT_UNAVAILABLE',
      );
    } finally {
      clearTimeout(timer);
    }
  };
}

export async function nativeFinanceReport(
  actor: NativeJournalPageContext | undefined,
  holdingSlug: string,
  query: {chartId?: string; periodId?: string; startDate?: string; endDate?: string},
): Promise<NativeFinancialReport | undefined> {
  const raw = process.env.ERPX_NATIVE_FINANCE_REPORTS_CONFIG;
  if (raw === undefined) return undefined;
  if (!actor) return fail('NATIVE_FINANCE_CONTEXT_CHANGED');
  if (actor.scope.kind !== 'holding') return undefined;
  return createNativeFinanceReportReader(raw)(await headers(), actor, holdingSlug, query);
}
