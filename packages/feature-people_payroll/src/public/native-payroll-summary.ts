import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/** Rendering input from retained page authorization, never a grant. */
export type NativePeoplePayrollPageContext = {
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  assignmentId: string;
  role: string;
  assignmentScope: string;
  scope:
    | { kind: 'holding' }
    | { kind: 'branch'; branchSlug: string }
    | { kind: 'department'; branchSlug?: string; departmentSlug: string };
  authority?: { revision: number; assignmentVersion: number };
  modules: readonly string[];
  iam?: { permissions: readonly string[] };
};

export type NativePeoplePayrollConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

export type NativePeoplePayrollReadInput = Readonly<{
  holdingSlug: string;
  branchSlug?: string;
  departmentSlug?: string;
  payMonth?: string;
  expected?: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

export type NativePayrollSummaryRow = Readonly<{
  pay_month: string;
  currency: string;
  run_count: number;
  headcount: number;
  gross_minor: number;
  deductions_minor: number;
  employer_cost_minor: number;
  net_minor: number;
}>;

export type NativePayrollRun = Readonly<{
  id: string;
  period_start: string;
  period_end: string;
  pay_date: string;
  currency: string;
  status: string;
  mode: string;
  revision: number;
  version: number;
  totals_json: string | null;
  payment_batch_id: string | null;
  payment_status: string | null;
  net_payable_minor: number | null;
}> & Record<string, unknown>;

export type NativePeoplePayrollTransport = (
  method: 'GET',
  url: URL,
  headers: Headers,
  body: undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativePeoplePayrollError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native People Payroll request could not be completed.');
    this.name = 'NativePeoplePayrollError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativePeoplePayrollError(code, status, message);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string =>
  text(value, 200) && value.length > 0;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const safeInteger = (value: unknown): value is number =>
  integer(value, Number.MIN_SAFE_INTEGER);
const optionalText = (value: unknown, max = 4096) =>
  value === null || text(value, max);
const optionalInteger = (value: unknown) =>
  value === null || safeInteger(value);
const texts = (value: unknown, max = 256): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};
const month = (value: unknown): value is string =>
  typeof value === 'string' && /^\d{4}-(0[1-9]|1[0-2])$/.test(value);
const date = (value: unknown): value is string =>
  typeof value === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(value) && value.length === 10;
const currency = (value: unknown): value is string =>
  typeof value === 'string' && /^[A-Z]{3}$/.test(value);

const summaryRow = (row: unknown) =>
  exact(row, [
    'pay_month', 'currency', 'run_count', 'headcount', 'gross_minor',
    'deductions_minor', 'employer_cost_minor', 'net_minor',
  ]) &&
  month(row.pay_month) &&
  currency(row.currency) &&
  integer(row.run_count, 1) &&
  integer(row.headcount) &&
  safeInteger(row.gross_minor) &&
  safeInteger(row.deductions_minor) &&
  safeInteger(row.employer_cost_minor) &&
  safeInteger(row.net_minor);

const runRow = (row: unknown) =>
  exact(row, [
    'id', 'period_start', 'period_end', 'pay_date', 'currency', 'status', 'mode',
    'revision', 'version', 'totals_json', 'payment_batch_id', 'payment_status',
    'net_payable_minor',
  ]) &&
  identifier(row.id) &&
  date(row.period_start) &&
  date(row.period_end) &&
  date(row.pay_date) &&
  currency(row.currency) &&
  identifier(row.status) &&
  identifier(row.mode) &&
  integer(row.revision, 1) &&
  integer(row.version, 1) &&
  (row.totals_json === null || objectJson(row.totals_json)) &&
  optionalText(row.payment_batch_id, 200) &&
  optionalText(row.payment_status, 200) &&
  optionalInteger(row.net_payable_minor) &&
  (row.net_payable_minor === null || row.net_payable_minor >= 0);

function objectJson(value: unknown): value is string {
  if (!text(value, 2 * 1024 * 1024) || value.length === 0) return false;
  try {
    return record(JSON.parse(value));
  } catch {
    return false;
  }
}

function summaryRows(value: unknown): NativePayrollSummaryRow[] {
  if (!Array.isArray(value) || value.length > 120 || !value.every(summaryRow))
    return failure('NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID');
  const seen = new Set<string>();
  for (const row of value) {
    const key = `${row.pay_month}:${row.currency}`;
    if (seen.has(key)) return failure('NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID');
    seen.add(key);
  }
  return value as NativePayrollSummaryRow[];
}

function runRows(value: unknown): NativePayrollRun[] {
  if (!Array.isArray(value) || value.length > 2000 || !value.every(runRow))
    return failure('NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID');
  const seen = new Set<string>();
  for (const row of value) {
    if (seen.has(row.id)) return failure('NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID');
    seen.add(row.id);
  }
  return value as NativePayrollRun[];
}

function context(value: unknown, actor: NativePeoplePayrollPageContext, input: NativePeoplePayrollReadInput): Context {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID');
  const parsed = value as Context;
  if (
    parsed.userId !== actor.userId ||
    parsed.holdingId !== actor.holdingId ||
    parsed.companyId !== actor.companyId ||
    parsed.branchId !== actor.branchId ||
    parsed.departmentId !== actor.departmentId ||
    parsed.branchSlug !== (input.branchSlug ?? null) ||
    parsed.departmentSlug !== (input.departmentSlug ?? null) ||
    parsed.assignmentId !== actor.assignmentId ||
    parsed.assignmentVersion !== actor.authority?.assignmentVersion ||
    parsed.revision !== actor.authority?.revision ||
    parsed.role !== actor.role ||
    parsed.assignmentScopeKind !== actor.assignmentScope ||
    !integer(parsed.expiresAt, 1) ||
    !texts(parsed.modules) ||
    !sameSet(actor.modules, parsed.modules) ||
    !texts(parsed.permissions) ||
    !sameSet(actor.iam?.permissions ?? [], parsed.permissions)
  )
    return failure('NATIVE_PEOPLE_PAYROLL_CONTEXT_CHANGED');
  if (
    input.expected &&
    (parsed.companyId !== input.expected.companyId ||
      parsed.assignmentId !== input.expected.assignmentId ||
      parsed.revision !== input.expected.revision ||
      parsed.assignmentVersion !== input.expected.assignmentVersion)
  )
    return failure('NATIVE_PEOPLE_PAYROLL_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativePeoplePayrollConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_PEOPLE_PAYROLL_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reserved.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    raw.privateBaseUrl !== base.origin ||
    !integer(raw.timeoutMs, 250) ||
    raw.timeoutMs > 30000 ||
    raw.holdings.length < 1 ||
    raw.holdings.length > 64
  )
    return failure('NATIVE_PEOPLE_PAYROLL_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_PEOPLE_PAYROLL_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      origin.port ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_PEOPLE_PAYROLL_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativePeoplePayrollTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  holdingSlug: string,
  path: 'summary' | 'runs',
  input: NativePeoplePayrollReadInput,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== holdingSlug)
    return Promise.reject(new NativePeoplePayrollError('NATIVE_PEOPLE_PAYROLL_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativePeoplePayrollError('NATIVE_PEOPLE_PAYROLL_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativePeoplePayrollError('NATIVE_PEOPLE_PAYROLL_REQUEST_INVALID'));
      headers.set(key, slug);
    }
  }
  if (input.expected) {
    if (
      !identifier(input.expected.companyId) ||
      !identifier(input.expected.assignmentId) ||
      !integer(input.expected.revision) ||
      !integer(input.expected.assignmentVersion, 1)
    )
      return Promise.reject(new NativePeoplePayrollError('NATIVE_PEOPLE_PAYROLL_REQUEST_INVALID'));
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/people/payroll/${path}`,
    base,
  );
  if (input.payMonth !== undefined) url.searchParams.set('payMonth', input.payMonth);
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, 30000);
  return transport('GET', url, headers, undefined, controller.signal).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  });
}

async function boundedBytes(response: Response, limit: number): Promise<Uint8Array> {
  if (!response.body) return new Uint8Array();
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > limit) {
        await reader.cancel();
        return failure('NATIVE_PEOPLE_PAYROLL_RESPONSE_LIMIT');
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

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_PEOPLE_PAYROLL_UNAVAILABLE', response.status);
  }
  const contentType = response.headers.get('content-type')?.split(';')[0]?.trim();
  if (response.status !== 200) {
    const body = await boundedBytes(response, 64 * 1024);
    let code: string | undefined;
    let message: string | undefined;
    if (contentType === 'application/json') {
      try {
        const value: unknown = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(body));
        if (record(value) && record(value.error)) {
          if (text(value.error.code, 200)) code = value.error.code;
          if (text(value.error.message, 2000)) message = value.error.message;
        }
      } catch {
        // A malformed owner error keeps the bounded transport failure.
      }
    }
    return failure(code ?? 'NATIVE_PEOPLE_PAYROLL_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID');
  }
}

export function nativePeoplePayrollAvailable(): boolean {
  return process.env.ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG !== undefined;
}

export function nativePeoplePayrollConfiguration(raw: string): NativePeoplePayrollConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativePeoplePayrollError('NATIVE_PEOPLE_PAYROLL_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings)
  )
    return failure('NATIVE_PEOPLE_PAYROLL_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_PEOPLE_PAYROLL_CONFIGURATION_INVALID');
  return value as unknown as NativePeoplePayrollConfiguration;
}

export function createNativePeoplePayrollClient(
  raw: NativePeoplePayrollConfiguration,
  transport: NativePeoplePayrollTransport = directHttp,
) {
  const config = configuration(raw);
  const request = (
    incoming: Pick<Headers, 'get'>,
    actor: NativePeoplePayrollPageContext,
    input: NativePeoplePayrollReadInput,
    path: 'summary' | 'runs',
    signal?: AbortSignal,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return Promise.reject(new NativePeoplePayrollError('NATIVE_PEOPLE_PAYROLL_CONTEXT_CHANGED'));
    return transportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      input.holdingSlug,
      path,
      input,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativePeoplePayrollError) throw error;
      return failure('NATIVE_PEOPLE_PAYROLL_UNAVAILABLE');
    });
  };
  const summary = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativePeoplePayrollPageContext,
    input: NativePeoplePayrollReadInput,
    signal?: AbortSignal,
  ): Promise<NativePayrollSummaryRow[]> => {
    const value = await boundedJson(await request(incoming, actor, input, 'summary', signal));
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID');
    context(value.context, actor, input);
    return summaryRows(value.data);
  };
  const runs = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativePeoplePayrollPageContext,
    input: NativePeoplePayrollReadInput,
    signal?: AbortSignal,
  ): Promise<NativePayrollRun[]> => {
    const value = await boundedJson(await request(incoming, actor, input, 'runs', signal));
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID');
    context(value.context, actor, input);
    return runRows(value.data);
  };
  return { summary, runs };
}

/** Fresh bounded socket; bypasses patched fetch/HMR and ambient proxies. */
export function directHttp(
  method: 'GET',
  url: URL,
  headers: Headers,
  body: undefined,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import('node:http').ProxyEnv });
    const outgoing = request(
      url,
      { method, agent, headers: { ...Object.fromEntries(headers) }, signal, maxHeaderSize: 8192 },
      (incoming) => {
        incoming.once('close', () => agent.destroy());
        try {
          const status = incoming.statusCode ?? 502;
          const values = new Headers();
          for (let index = 0; index < incoming.rawHeaders.length; index += 2)
            values.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
          resolve(
            new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
              status: status >= 200 && status <= 599 ? status : 502,
              headers: values,
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
    outgoing.end();
  });
}
