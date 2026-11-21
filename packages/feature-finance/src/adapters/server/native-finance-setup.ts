import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { Readable } from 'node:stream';
import { headers } from 'next/headers';
import {
  directHttp,
  NativeFinanceReadError,
  type NativeFinanceReadConfiguration,
  type NativeFinanceReadTransport,
} from './native-journal-detail';
import { nativeFinanceConfiguration, type NativeJournalPageContext } from './native-journal-page';

type Row = Record<string, string | number | boolean | null>;
export type NativeChartAccounts = {
  chart: Row;
  accounts: Row[];
  periods: Row[];
};
export type NativeFinanceSetupCheck = {
  id: string;
  label: string;
  complete: boolean;
  detail: string;
};
export type NativeSetupRow = Record<string, string | number | null>;
export type NativeFinanceSetup = {
  charts: NativeSetupRow[];
  parties: NativeSetupRow[];
  checks: NativeFinanceSetupCheck[];
};

const limit = 256 * 1024;
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
const setupRow = (value: unknown): value is NativeSetupRow =>
  object(value) &&
  Object.values(value).every(
    (item) =>
      item === null ||
      typeof item === 'string' ||
      (typeof item === 'number' && Number.isSafeInteger(item)),
  );
const setupRows = (value: unknown, max = 10000): value is NativeSetupRow[] =>
  Array.isArray(value) && value.length <= max && value.every(setupRow);
const checks = (value: unknown): value is NativeFinanceSetupCheck[] =>
  Array.isArray(value) &&
  value.length <= 32 &&
  value.every(
    (check) =>
      object(check) &&
      text(check.id, 100) &&
      text(check.label, 200) &&
      typeof check.complete === 'boolean' &&
      text(check.detail, 1000),
  );
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

function requestHeaders(
  incoming: Pick<Headers, 'get'>,
  actor: NativeJournalPageContext,
  branchSlug: string | undefined,
  departmentSlug: string | undefined,
): Headers {
  const out = new Headers({ accept: 'application/json', 'cache-control': 'no-cache' });
  const host = incoming.get('host');
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
  out.set('x-erpx-context-revision', String(actor.authority!.revision));
  out.set('x-erpx-context-assignment-version', String(actor.authority!.assignmentVersion));
  if (branchSlug) out.set('x-erpx-workspace-branch', branchSlug);
  if (departmentSlug) out.set('x-erpx-workspace-department', departmentSlug);
  return out;
}

function validateContext(
  value: unknown,
  actor: NativeJournalPageContext,
  branchSlug: string | undefined,
  departmentSlug: string | undefined,
): asserts value is Record<string, unknown> {
  if (!object(value) || !object(value.context)) return fail('NATIVE_FINANCE_SETUP_RESPONSE_INVALID');
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

export function createNativeFinanceSetupReader(
  raw: string,
  transport: NativeFinanceReadTransport = directHttp,
) {
  const config = configuration(raw);
  return async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeJournalPageContext,
    holdingSlug: string,
    chartId: string,
  ): Promise<NativeChartAccounts> => {
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
    const out = requestHeaders(incoming, actor, branchSlug, departmentSlug);
    const url = new URL(
      `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/charts/${encodeURIComponent(chartId)}/accounts`,
      config.privateBaseUrl,
    );
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeoutMs);
    try {
      const response = await transport(url, out, controller.signal);
      if (response.redirected || response.status !== 200) {
        void response.body?.cancel().catch(() => undefined);
        return fail('NATIVE_FINANCE_SETUP_UNAVAILABLE', response.status);
      }
      if (response.headers.get('content-type')?.split(';')[0] !== 'application/json') {
        return fail('NATIVE_FINANCE_SETUP_RESPONSE_INVALID');
      }
      const value: unknown = await response.json();
      validateContext(value, actor, branchSlug, departmentSlug);
      const data = value.data;
      if (!object(data) || !ledger(data.chart) || !rows(data.accounts) || !rows(data.periods)) {
        return fail('NATIVE_FINANCE_SETUP_RESPONSE_INVALID');
      }
      return { chart: data.chart, accounts: data.accounts, periods: data.periods };
    } catch (error) {
      if (error instanceof NativeFinanceReadError) throw error;
      return fail(
        controller.signal.aborted ? 'NATIVE_FINANCE_SETUP_ABORTED' : 'NATIVE_FINANCE_SETUP_UNAVAILABLE',
      );
    } finally {
      clearTimeout(timer);
    }
  };
}

export function createNativeFinanceSetupWorkspaceReader(
  raw: string,
  transport: NativeFinanceReadTransport = directHttp,
) {
  const config = configuration(raw);
  return async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeJournalPageContext,
    holdingSlug: string,
  ): Promise<NativeFinanceSetup> => {
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
    const out = requestHeaders(incoming, actor, branchSlug, departmentSlug);
    const url = new URL(
      `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/finance/charts/setup/accounts?view=setup`,
      config.privateBaseUrl,
    );
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeoutMs);
    try {
      const response = await transport(url, out, controller.signal);
      if (response.redirected || response.status !== 200) {
        void response.body?.cancel().catch(() => undefined);
        return fail('NATIVE_FINANCE_SETUP_UNAVAILABLE', response.status);
      }
      if (response.headers.get('content-type')?.split(';')[0] !== 'application/json') {
        return fail('NATIVE_FINANCE_SETUP_RESPONSE_INVALID');
      }
      const value: unknown = await response.json();
      validateContext(value, actor, branchSlug, departmentSlug);
      const data = value.data;
      if (
        !object(data) ||
        !setupRows(data.charts) ||
        !setupRows(data.parties) ||
        !checks(data.checks)
      ) {
        return fail('NATIVE_FINANCE_SETUP_RESPONSE_INVALID');
      }
      return { charts: data.charts, parties: data.parties, checks: data.checks };
    } catch (error) {
      if (error instanceof NativeFinanceReadError) throw error;
      return fail(
        controller.signal.aborted ? 'NATIVE_FINANCE_SETUP_ABORTED' : 'NATIVE_FINANCE_SETUP_UNAVAILABLE',
      );
    } finally {
      clearTimeout(timer);
    }
  };
}

export async function nativeFinanceSetupWorkspace(
  actor: NativeJournalPageContext | undefined,
  holdingSlug: string,
): Promise<NativeFinanceSetup | undefined> {
  const raw = process.env.ERPX_NATIVE_FINANCE_SETUP_CONFIG;
  if (raw === undefined) return undefined;
  if (!actor) return fail('NATIVE_FINANCE_CONTEXT_CHANGED');
  if (actor.scope.kind !== 'holding') return undefined;
  return createNativeFinanceSetupWorkspaceReader(raw)(await headers(), actor, holdingSlug);
}

export async function nativeChartAccounts(
  actor: NativeJournalPageContext | undefined,
  holdingSlug: string,
  chartId: string | undefined,
): Promise<NativeChartAccounts | undefined> {
  const raw = process.env.ERPX_NATIVE_FINANCE_SETUP_CONFIG;
  if (raw === undefined || !chartId) return undefined;
  if (!actor) return fail('NATIVE_FINANCE_CONTEXT_CHANGED');
  if (actor.scope.kind !== 'holding') return undefined;
  return createNativeFinanceSetupReader(raw)(await headers(), actor, holdingSlug, chartId);
}

function directPost(
  url: URL,
  headers: Headers,
  body: Uint8Array,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const outgoing = httpRequest(
      url,
      { method: 'POST', agent, maxHeaderSize: 8192, headers: Object.fromEntries(headers), signal },
      (incoming) => {
        incoming.once('close', () => agent.destroy());
        try {
          const responseHeaders = new Headers();
          for (let i = 0; i < incoming.rawHeaders.length; i += 2) {
            responseHeaders.append(incoming.rawHeaders[i]!, incoming.rawHeaders[i + 1]!);
          }
          resolve(
            new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
              status: incoming.statusCode ?? 502,
              headers: responseHeaders,
            }),
          );
        } catch (error) {
          incoming.destroy();
          agent.destroy();
          reject(error);
        }
      },
    );
    outgoing.once('error', (error) => {
      agent.destroy();
      reject(error);
    });
    outgoing.end(body);
  });
}

const closed = (status: number, code: string, message: string) =>
  Response.json({ error: { code, message } }, { status, headers: { 'Cache-Control': 'private, no-store' } });

async function bytes(body: ReadableStream<Uint8Array> | null, max: number): Promise<Uint8Array> {
  if (!body) return new Uint8Array();
  const reader = body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      size += next.value.length;
      if (size > max) {
        await reader.cancel();
        throw new Error('BODY_LIMIT');
      }
      chunks.push(next.value);
    }
  } finally {
    reader.releaseLock();
  }
  const result = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.length;
  }
  return result;
}

type SetupAction =
  | { kind: 'chart'; input: Record<string, unknown> }
  | { kind: 'import'; input: Record<string, unknown> }
  | { kind: 'account'; chartId: string; input: Record<string, unknown> }
  | { kind: 'status'; chartId: string; accountId: string; input: Record<string, unknown> }
  | { kind: 'period'; input: Record<string, unknown> };

/** Exact native setup command. An absent config selects the legacy route. */
export async function nativeFinanceSetupCommand(
  request: Request,
  holdingSlug: string,
  action: SetupAction,
): Promise<Response | undefined> {
  const raw = process.env.ERPX_NATIVE_FINANCE_SETUP_CONFIG;
  if (raw === undefined) return undefined;
  const settings = configuration(raw);
  const host = request.headers.get('host');
  const target = settings.holdings.find(
    (holding) => new URL(holding.origin).host === host && holding.holdingSlug === holdingSlug,
  );
  if (!target) return closed(404, 'NOT_FOUND', 'Not found.');
  const origin = request.headers.get('origin');
  if (origin !== null && origin !== target.origin) return closed(403, 'ORIGIN_FORBIDDEN', 'Use the application origin.');
  const key = request.headers.get('idempotency-key');
  if (!key || key.length < 8 || key.length > 180) {
    return closed(422, 'IDEMPOTENCY_KEY_REQUIRED', 'Idempotency-Key is required.');
  }
  const body = new TextEncoder().encode(JSON.stringify(action.input));
  const out = new Headers({ host: host!, accept: 'application/json', 'content-type': 'application/json' });
  for (const name of [
    'cookie',
    'origin',
    'x-erpx-context-company',
    'x-erpx-context-revision',
    'x-erpx-context-assignment',
    'x-erpx-context-assignment-version',
    'x-erpx-workspace-branch',
    'x-erpx-workspace-department',
  ]) {
    const value = request.headers.get(name);
    if (value !== null) out.set(name, value);
  }
  out.set('idempotency-key', key);
  const path =
    action.kind === 'chart'
      ? '/finance/charts'
      : action.kind === 'import'
        ? '/finance/charts/import'
        : action.kind === 'account'
          ? `/finance/charts/${encodeURIComponent(action.chartId)}/accounts`
          : action.kind === 'status'
            ? `/finance/charts/${encodeURIComponent(action.chartId)}/accounts/${encodeURIComponent(action.accountId)}/status`
            : '/finance/periods';
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holdingSlug)}${path}`, settings.privateBaseUrl);
  const controller = new AbortController();
  const cancel = () => controller.abort();
  request.signal.addEventListener('abort', cancel, { once: true });
  if (request.signal.aborted) controller.abort();
  const timer = setTimeout(cancel, settings.timeoutMs);
  try {
    const response = await directPost(url, out, body, controller.signal);
    if (response.status >= 300 && response.status < 400) {
      void response.body?.cancel();
      return closed(502, 'NATIVE_FINANCE_SETUP_UNAVAILABLE', 'Unable to confirm Finance setup.');
    }
    if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
      void response.body?.cancel();
      return closed(502, 'NATIVE_FINANCE_SETUP_UNAVAILABLE', 'Unable to confirm Finance setup.');
    }
    const payload = await bytes(response.body, 32768);
    let value: unknown;
    try {
      value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(payload));
    } catch {
      return closed(502, 'NATIVE_FINANCE_SETUP_UNAVAILABLE', 'Unable to confirm Finance setup.');
    }
    if ((response.status === 200 || response.status === 201) && object(value) && object(value.data)) {
      return Response.json(value, { status: response.status, headers: { 'Cache-Control': 'private, no-store' } });
    }
    const error = object(value) && object(value.error) ? value.error : undefined;
    if (
      response.status < 400 ||
      response.status > 599 ||
      !error ||
      typeof error.code !== 'string' ||
      typeof error.message !== 'string'
    ) {
      return closed(502, 'NATIVE_FINANCE_SETUP_UNAVAILABLE', 'Unable to confirm Finance setup.');
    }
    return closed(response.status, error.code, error.message);
  } catch {
    return closed(503, 'NATIVE_FINANCE_SETUP_UNAVAILABLE', 'Unable to confirm Finance setup.');
  } finally {
    clearTimeout(timer);
    request.signal.removeEventListener('abort', cancel);
  }
}
