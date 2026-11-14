import 'server-only';
import { headers } from 'next/headers';
import { ApiError } from '@erpx/server-runtime/public/http';
import {
  directHttp,
  NativeFinanceReadError,
  type NativeFinanceReadConfiguration,
  type NativeFinanceReadTransport,
} from './native-journal-detail';
import { nativeFinanceConfiguration, type NativeJournalPageContext } from './native-journal-page';

type Row = Record<string, string | number | boolean | null>;
export type NativeFinanceBankPage = {
  items: Row[];
  nextCursor: string | null;
  previousCursor: string | null;
  hasMore: boolean;
};
export type NativeFinanceBankWorkspace = {
  accounts: Row[];
  statements: Row[];
  statementPage: NativeFinanceBankPage | null;
  transactions: Row[];
  transactionPage: NativeFinanceBankPage | null;
  payments: Row[];
  selectedTransaction: Row | null;
};
export type NativeFinanceBankQuery = {
  print?: string;
  transactionCursor?: string;
  statementCursor?: string;
  match?: string;
  new?: string;
  chart?: string;
  view?: string;
  q?: string;
};

const limit = 2 * 1024 * 1024;
const fail = (code: string, status?: number): never => {
  throw new NativeFinanceReadError(code, status);
};
const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Record<string, unknown> =>
  object(value) && Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
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
const rows = (value: unknown, max: number): value is Row[] =>
  Array.isArray(value) && value.length <= max && value.every(ledger);

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
    config.timeoutMs > 30000 ||
    config.holdings.length < 1 ||
    config.holdings.length > 64
  ) {
    return fail('NATIVE_FINANCE_CONFIGURATION_INVALID');
  }
  return config;
}

function scope(actor: NativeJournalPageContext, holdingSlug: string) {
  if (!actor.authority || !actor.iam || actor.holdingSlug !== holdingSlug) {
    return fail('NATIVE_FINANCE_CONTEXT_CHANGED');
  }
  if (!text(actor.userId) || !text(actor.holdingId) || !text(actor.companyId) || !text(actor.assignmentId))
    return fail('NATIVE_FINANCE_AUTHORITY_INVALID');
  const branchSlug = actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug;
  const departmentSlug =
    actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined;
  return { branchSlug, departmentSlug };
}

function requested(
  incoming: Pick<Headers, 'get'>,
  actor: NativeJournalPageContext,
  branchSlug: string | undefined,
  departmentSlug: string | undefined,
): Headers {
  const host = incoming.get('host');
  if (!host) return fail('NATIVE_FINANCE_AUTHORITY_INVALID');
  const output = new Headers({ host, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192) return fail('NATIVE_FINANCE_COOKIE_LIMIT');
    output.set('cookie', cookie);
  }
  const origin = incoming.get('origin');
  if (origin) output.set('origin', origin);
  const expected = {
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority!.revision),
    'x-erpx-context-assignment-version': String(actor.authority!.assignmentVersion),
  } as const;
  for (const [name, value] of Object.entries(expected)) {
    const seen = incoming.get(name);
    if (seen !== null && seen !== value) return fail('NATIVE_FINANCE_CONTEXT_CHANGED');
    output.set(name, value);
  }
  if (branchSlug) output.set('x-erpx-workspace-branch', branchSlug);
  if (departmentSlug) output.set('x-erpx-workspace-department', departmentSlug);
  return output;
}

function url(
  base: string,
  holdingSlug: string,
  query: NativeFinanceBankQuery,
): URL {
  const target = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/bank`,
    base,
  );
  for (const name of [
    'print',
    'transactionCursor',
    'statementCursor',
    'match',
    'new',
    'chart',
    'view',
    'q',
  ] as const) {
    const value = query[name];
    if (value) target.searchParams.set(name, value);
  }
  return target;
}

function page(value: unknown, row: (value: unknown) => boolean): value is NativeFinanceBankPage {
  return (
    exact(value, ['items', 'nextCursor', 'previousCursor', 'hasMore']) &&
    Array.isArray(value.items) &&
    value.items.length <= 30 &&
    value.items.every(row) &&
    (value.nextCursor === null || (typeof value.nextCursor === 'string' && value.nextCursor.length <= 4096)) &&
    (value.previousCursor === null ||
      (typeof value.previousCursor === 'string' && value.previousCursor.length <= 4096)) &&
    typeof value.hasMore === 'boolean'
  );
}

function owned(actor: NativeJournalPageContext, row: Row) {
  return row.company_id === actor.companyId;
}

function statement(actor: NativeJournalPageContext, row: Row) {
  return (
    owned(actor, row) &&
    text(row.id) &&
    text(row.bank_account) &&
    text(row.statement_number) &&
    typeof row.statement_date === 'string' &&
    integer(row.opening_minor) &&
    integer(row.closing_minor) &&
    (row.currency === null || text(row.currency, 3)) &&
    text(row.base_currency, 3) &&
    integer(row.version, 1)
  );
}

function transaction(actor: NativeJournalPageContext, row: Row) {
  return (
    owned(actor, row) &&
    text(row.id) &&
    text(row.statement_id) &&
    text(row.transaction_date, 32) &&
    typeof row.external_id === 'string' &&
    typeof row.description === 'string' &&
    typeof row.amount_minor === 'number' &&
    Number.isSafeInteger(row.amount_minor) &&
    text(row.currency, 3) &&
    ['unmatched', 'proposed', 'matched', 'ignored'].includes(String(row.match_status)) &&
    text(row.bank_account) &&
    text(row.statement_number)
  );
}

function payment(actor: NativeJournalPageContext, row: Row) {
  return (
    owned(actor, row) &&
    text(row.id) &&
    text(row.payment_number) &&
    text(row.payment_date, 32) &&
    text(row.currency, 3) &&
    integer(row.amount_minor, 1) &&
    ['released', 'settled'].includes(String(row.status))
  );
}

function account(row: unknown) {
  return (
    exact(row, ['id', 'chart_id', 'code', 'name', 'currency', 'base_currency']) &&
    text(row.id) &&
    text(row.chart_id) &&
    typeof row.code === 'string' &&
    typeof row.name === 'string' &&
    (row.currency === null || text(row.currency, 3)) &&
    text(row.base_currency, 3)
  );
}

function workspace(value: unknown, actor: NativeJournalPageContext): NativeFinanceBankWorkspace {
  if (!exact(value, ['data']) || !exact(value.data, [
    'accounts',
    'statements',
    'statementPage',
    'transactions',
    'transactionPage',
    'payments',
    'selectedTransaction',
  ])) {
    return fail('NATIVE_FINANCE_RESPONSE_INVALID');
  }
  const data = value.data;
  if (
    !rows(data.accounts, 1000) ||
    !data.accounts.every((row) => account(row)) ||
    !rows(data.statements, 10000) ||
    !data.statements.every((row) => statement(actor, row)) ||
    !rows(data.transactions, 10000) ||
    !data.transactions.every((row) => transaction(actor, row)) ||
    !rows(data.payments, 10000) ||
    !data.payments.every((row) => payment(actor, row)) ||
    !(
      data.statementPage === null ||
      page(data.statementPage, (row) => ledger(row) && statement(actor, row))
    ) ||
    !(
      data.transactionPage === null ||
      page(data.transactionPage, (row) => ledger(row) && transaction(actor, row))
    ) ||
    !(
      data.selectedTransaction === null ||
      (ledger(data.selectedTransaction) && transaction(actor, data.selectedTransaction))
    ) ||
    data.accounts.length + data.statements.length + data.transactions.length + data.payments.length > 30000
  ) {
    return fail('NATIVE_FINANCE_RESPONSE_INVALID');
  }
  return data as unknown as NativeFinanceBankWorkspace;
}

async function replyBytes(body: ReadableStream<Uint8Array> | null): Promise<Uint8Array> {
  if (!body) return fail('NATIVE_FINANCE_RESPONSE_INVALID');
  const reader = body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > limit) {
        await reader.cancel();
        return fail('NATIVE_FINANCE_RESPONSE_LIMIT');
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
    offset += chunk.length;
  }
  return data;
}

export function createNativeFinanceBankRead(
  raw: string,
  transport: NativeFinanceReadTransport = directHttp,
) {
  const config = configuration(raw);
  return async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeJournalPageContext,
    holdingSlug: string,
    query: NativeFinanceBankQuery,
  ): Promise<NativeFinanceBankWorkspace> => {
    const { branchSlug, departmentSlug } = scope(actor, holdingSlug);
    const host = incoming.get('host');
    const holding = config.holdings.find(
      (entry) => new URL(entry.origin).host === host && entry.holdingSlug === holdingSlug,
    );
    if (!holding || holding.holdingId !== actor.holdingId) {
      return fail('NATIVE_FINANCE_AUTHORITY_INVALID');
    }
    const output = requested(incoming, actor, branchSlug, departmentSlug);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeoutMs);
    try {
      const response = await transport(url(config.privateBaseUrl, holdingSlug, query), output, controller.signal);
      if (response.redirected || response.status !== 200) {
        let code = 'NATIVE_FINANCE_UNAVAILABLE';
        if (response.body) {
          try {
            const value: unknown = JSON.parse(
              new TextDecoder('utf-8', { fatal: true }).decode(await replyBytes(response.body)),
            );
            if (object(value) && object(value.error) && text(value.error.code, 120)) {
              code = value.error.code;
            }
          } catch {
            // A closed native failure still maps by its HTTP status.
          }
        }
        return fail(code, response.status);
      }
      if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
        void response.body?.cancel().catch(() => undefined);
        return fail('NATIVE_FINANCE_RESPONSE_INVALID');
      }
      let parsed: unknown;
      try {
        parsed = JSON.parse(
          new TextDecoder('utf-8', { fatal: true }).decode(await replyBytes(response.body)),
        );
      } catch {
        return fail('NATIVE_FINANCE_RESPONSE_INVALID');
      }
      return workspace(parsed, actor);
    } catch (error) {
      if (error instanceof NativeFinanceReadError) throw error;
      return fail(controller.signal.aborted ? 'NATIVE_FINANCE_ABORTED' : 'NATIVE_FINANCE_UNAVAILABLE');
    } finally {
      clearTimeout(timer);
    }
  };
}

export async function nativeFinanceBankWorkspace(
  actor: NativeJournalPageContext | undefined,
  holdingSlug: string,
  query: NativeFinanceBankQuery,
): Promise<NativeFinanceBankWorkspace | undefined> {
  const raw = process.env.ERPX_NATIVE_FINANCE_BANK_CONFIG;
  if (raw === undefined) return undefined;
  if (!actor) return fail('NATIVE_FINANCE_CONTEXT_CHANGED');
  return createNativeFinanceBankRead(raw)(await headers(), actor, holdingSlug, query);
}

function query(search: URLSearchParams): NativeFinanceBankQuery {
  const output: NativeFinanceBankQuery = {};
  const seen = new Set<string>();
  for (const [name, value] of search) {
    if (
      !['print', 'transactionCursor', 'statementCursor', 'match', 'new', 'chart', 'view', 'q'].includes(name) ||
      seen.has(name) ||
      new TextEncoder().encode(value).length > 4096 ||
      [...value].some((character) => character.charCodeAt(0) < 32 || character.charCodeAt(0) === 127)
    ) {
      return fail('NATIVE_FINANCE_QUERY_INVALID', 422);
    }
    seen.add(name);
    if (value) output[name as keyof NativeFinanceBankQuery] = value;
  }
  return output;
}

function errorResponse(error: unknown): Response {
  const status =
    error instanceof NativeFinanceReadError && error.code === 'NATIVE_FINANCE_CONTEXT_CHANGED'
      ? 409
      : error instanceof NativeFinanceReadError && error.code === 'NATIVE_FINANCE_AUTHORITY_INVALID'
        ? 403
        : error instanceof NativeFinanceReadError && error.code === 'NATIVE_FINANCE_QUERY_INVALID'
          ? 422
          : error instanceof NativeFinanceReadError && error.status
            ? error.status >= 400 && error.status <= 599
              ? error.status
              : 503
            : 503;
  const code = error instanceof NativeFinanceReadError ? error.code : 'NATIVE_FINANCE_UNAVAILABLE';
  return Response.json(
    { error: { code, message: 'The bank reconciliation workspace is unavailable.' } },
    {
      status,
      headers: {
        'Cache-Control': 'private, no-store',
        'Referrer-Policy': 'no-referrer',
        'X-Content-Type-Options': 'nosniff',
      },
    },
  );
}

export function nativeFinanceBankPageFailure(error: unknown): ApiError | undefined {
  if (!(error instanceof NativeFinanceReadError)) return undefined;
  if (error.code === 'NATIVE_FINANCE_CONTEXT_CHANGED')
    return new ApiError(409, 'WORK_CONTEXT_CHANGED', 'Your work role changed. Reload and review before submitting.');
  if (error.code === 'NATIVE_FINANCE_AUTHORITY_INVALID')
    return new ApiError(403, 'FORBIDDEN', 'This action is unavailable in your current role.');
  if (error.code === 'NATIVE_FINANCE_QUERY_INVALID')
    return new ApiError(422, 'VALIDATION_FAILED', 'Use the bank reconciliation filters.');
  if (error.code === 'BANK_TRANSACTION_NOT_FOUND')
    return new ApiError(404, 'BANK_TRANSACTION_NOT_FOUND', 'Bank movement not found in this Company.');
  if (error.status === 401) return new ApiError(401, 'UNAUTHENTICATED', 'Sign in to continue.');
  if (error.status === 403) return new ApiError(403, 'FORBIDDEN', 'This action is unavailable in your current role.');
  if (error.status === 404) return new ApiError(404, 'NOT_FOUND', 'Not found.');
  if (error.status === 409)
    return new ApiError(409, 'WORK_CONTEXT_CHANGED', 'Your work role changed. Reload and review before submitting.');
  if (error.status === 422)
    return new ApiError(422, 'VALIDATION_FAILED', 'Use the bank reconciliation filters.');
  return undefined;
}

export function createNativeFinanceBankHttpGet(
  raw: string,
  transport: NativeFinanceReadTransport = directHttp,
) {
  const read = createNativeFinanceBankRead(raw, transport);
  return async (
    request: Request,
    actor: NativeJournalPageContext,
    holdingSlug: string,
  ): Promise<Response> => {
    try {
      const data = await read(request.headers, actor, holdingSlug, query(new URL(request.url).searchParams));
      return Response.json(
        { data },
        {
          status: 200,
          headers: {
            'Cache-Control': 'private, no-store',
            'Referrer-Policy': 'no-referrer',
            'X-Content-Type-Options': 'nosniff',
          },
        },
      );
    } catch (error) {
      return errorResponse(error);
    }
  };
}

export async function nativeFinanceBankHttpGet(
  request: Request,
  actor: NativeJournalPageContext,
  holdingSlug: string,
): Promise<Response | undefined> {
  const raw = process.env.ERPX_NATIVE_FINANCE_BANK_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeFinanceBankHttpGet(raw)(request, actor, holdingSlug);
}
