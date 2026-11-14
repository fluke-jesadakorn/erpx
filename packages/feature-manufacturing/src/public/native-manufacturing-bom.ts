import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import type {
  NativeManufacturingBom,
  NativeManufacturingBomLine,
  NativeManufacturingBomOperation,
  NativeManufacturingBomSpecification,
} from '../lib/native-manufacturing-types';

/** Rendering input from retained page authorization, never a grant. */
export type NativeManufacturingPageContext = {
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

export type NativeManufacturingConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

type Row = Record<string, unknown>;
type Context = Row & { modules: string[]; permissions: string[] };

export type NativeManufacturingBomReadInput = Readonly<{
  holdingSlug: string;
  branchSlug?: string;
  departmentSlug?: string;
  expected?: Readonly<{
    companyId: string;
    assignmentId: string;
    revision: number;
    assignmentVersion: number;
  }>;
}>;

export type NativeManufacturingTransport = (
  method: 'GET',
  url: URL,
  headers: Headers,
  body: undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeManufacturingError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Manufacturing request could not be completed.');
    this.name = 'NativeManufacturingError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeManufacturingError(code, status, message);
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
  text(value, 200) && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const trimmed = (value: unknown, minimum: number, maximum: number): value is string =>
  text(value, maximum) &&
  [...value].length >= minimum &&
  value.trim() === value &&
  !/[\u0000-\u001f\u007f]/.test(value);
const optionalIdentifier = (value: unknown) =>
  value === null || identifier(value);
const modes = new Set(['bom', 'formula']);
const statuses = new Set(['draft', 'approved', 'rejected', 'retired']);
const MAX_SPECIFICATIONS = 200;
const MAX_LINES_PER_SPECIFICATION = 100;
const MAX_OPERATIONS_PER_SPECIFICATION = 50;
const MAX_TOTAL_LINES = 20_000;
const MAX_TOTAL_OPERATIONS = 10_000;

const lineRow = (value: unknown): value is NativeManufacturingBomLine =>
  exact(value, [
    'id', 'specification_id', 'product_id', 'quantity_milli',
    'substitution_group', 'ordinal',
  ]) &&
  identifier(value.id) &&
  identifier(value.specification_id) &&
  identifier(value.product_id) &&
  integer(value.quantity_milli, 1) &&
  optionalIdentifier(value.substitution_group) &&
  (value.substitution_group === null ||
    (text(value.substitution_group, 80) &&
      value.substitution_group.trim() === value.substitution_group)) &&
  integer(value.ordinal);

const operationRow = (value: unknown): value is NativeManufacturingBomOperation =>
  exact(value, [
    'id', 'specification_id', 'resource_id', 'resource_name', 'name',
    'setup_minutes', 'run_minutes_per_batch', 'ordinal',
  ]) &&
  identifier(value.id) &&
  identifier(value.specification_id) &&
  identifier(value.resource_id) &&
  trimmed(value.resource_name, 2, 160) &&
  trimmed(value.name, 2, 160) &&
  integer(value.setup_minutes) &&
  integer(value.run_minutes_per_batch, 1) &&
  integer(value.ordinal);

const specificationRow = (value: unknown): value is NativeManufacturingBomSpecification =>
  exact(value, [
    'id', 'company_id', 'name', 'mode', 'output_product_id',
    'batch_quantity_milli', 'expected_yield_bps', 'status', 'created_by',
    'reviewed_by', 'version', 'created_at', 'updated_at', 'lines', 'operations',
  ]) &&
  identifier(value.id) &&
  identifier(value.company_id) &&
  trimmed(value.name, 2, 160) &&
  typeof value.mode === 'string' &&
  modes.has(value.mode) &&
  identifier(value.output_product_id) &&
  integer(value.batch_quantity_milli, 1) &&
  integer(value.expected_yield_bps, 1) &&
  value.expected_yield_bps <= 20_000 &&
  typeof value.status === 'string' &&
  statuses.has(value.status) &&
  identifier(value.created_by) &&
  optionalIdentifier(value.reviewed_by) &&
  (value.status === 'draft'
    ? value.reviewed_by === null
    : identifier(value.reviewed_by) && value.reviewed_by !== value.created_by) &&
  integer(value.version, 1) &&
  integer(value.created_at) &&
  integer(value.updated_at) &&
  value.updated_at >= value.created_at &&
  Array.isArray(value.lines) &&
  value.lines.length >= 1 &&
  value.lines.length <= MAX_LINES_PER_SPECIFICATION &&
  value.lines.every(lineRow) &&
  Array.isArray(value.operations) &&
  value.operations.length <= MAX_OPERATIONS_PER_SPECIFICATION &&
  value.operations.every(operationRow);

function bomData(value: unknown, companyId: string): NativeManufacturingBom {
  if (
    !exact(value, ['specifications']) ||
    !Array.isArray(value.specifications) ||
    value.specifications.length > MAX_SPECIFICATIONS ||
    !value.specifications.every(specificationRow)
  )
    return failure('NATIVE_MANUFACTURING_RESPONSE_INVALID');

  const specifications = value.specifications as NativeManufacturingBomSpecification[];
  const specificationIds = new Set<string>();
  const lineIds = new Set<string>();
  const operationIds = new Set<string>();
  let totalLines = 0;
  let totalOperations = 0;
  for (const specification of specifications) {
    if (
      specification.company_id !== companyId ||
      specificationIds.has(specification.id) ||
      specification.lines.length > MAX_LINES_PER_SPECIFICATION ||
      specification.operations.length > MAX_OPERATIONS_PER_SPECIFICATION
    )
      return failure('NATIVE_MANUFACTURING_RESPONSE_INVALID');
    specificationIds.add(specification.id);
    totalLines += specification.lines.length;
    totalOperations += specification.operations.length;
    if (totalLines > MAX_TOTAL_LINES || totalOperations > MAX_TOTAL_OPERATIONS)
      return failure('NATIVE_MANUFACTURING_RESPONSE_INVALID');
    const products = new Set<string>();
    const ordinals = new Set<number>();
    for (const line of specification.lines) {
      if (
        line.specification_id !== specification.id ||
        line.product_id === specification.output_product_id ||
        lineIds.has(line.id) ||
        products.has(line.product_id) ||
        ordinals.has(line.ordinal)
      )
        return failure('NATIVE_MANUFACTURING_RESPONSE_INVALID');
      lineIds.add(line.id);
      products.add(line.product_id);
      ordinals.add(line.ordinal);
    }
    const operationOrdinals = new Set<number>();
    for (const operation of specification.operations) {
      if (
        operation.specification_id !== specification.id ||
        operationIds.has(operation.id) ||
        operationOrdinals.has(operation.ordinal)
      )
        return failure('NATIVE_MANUFACTURING_RESPONSE_INVALID');
      operationIds.add(operation.id);
      operationOrdinals.add(operation.ordinal);
    }
  }
  return { specifications };
}

function context(
  value: unknown,
  actor: NativeManufacturingPageContext,
  input: NativeManufacturingBomReadInput,
): Context {
  if (
    !exact(value, [
      'userId', 'holdingId', 'companyId', 'branchId', 'departmentId', 'branchSlug',
      'departmentSlug', 'assignmentId', 'assignmentVersion', 'revision', 'role',
      'assignmentScopeKind', 'modules', 'permissions', 'expiresAt',
    ])
  )
    return failure('NATIVE_MANUFACTURING_RESPONSE_INVALID');
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
    !Array.isArray(parsed.modules) ||
    parsed.modules.length > 128 ||
    !parsed.modules.every((item) => identifier(item)) ||
    parsed.modules.length !== actor.modules.length ||
    !parsed.modules.every((item) => actor.modules.includes(item)) ||
    !Array.isArray(parsed.permissions) ||
    parsed.permissions.length > 512 ||
    !parsed.permissions.every((item) => identifier(item)) ||
    parsed.permissions.length !== (actor.iam?.permissions.length ?? 0) ||
    !parsed.permissions.every((item) => (actor.iam?.permissions ?? []).includes(item))
  )
    return failure('NATIVE_MANUFACTURING_CONTEXT_CHANGED');
  if (
    input.expected &&
    (parsed.companyId !== input.expected.companyId ||
      parsed.assignmentId !== input.expected.assignmentId ||
      parsed.revision !== input.expected.revision ||
      parsed.assignmentVersion !== input.expected.assignmentVersion)
  )
    return failure('NATIVE_MANUFACTURING_CONTEXT_CHANGED');
  return parsed;
}

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(raw: NativeManufacturingConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure('NATIVE_MANUFACTURING_CONFIGURATION_INVALID');
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
    return failure('NATIVE_MANUFACTURING_CONFIGURATION_INVALID');
  const holdings = new Map<string, { holdingId: string; holdingSlug: string }>();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure('NATIVE_MANUFACTURING_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure('NATIVE_MANUFACTURING_CONFIGURATION_INVALID');
    holdings.set(origin.host, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function transportCall(
  transport: NativeManufacturingTransport,
  base: URL,
  holdings: Map<string, { holdingId: string; holdingSlug: string }>,
  incoming: Pick<Headers, 'get'>,
  holdingSlug: string,
  input: NativeManufacturingBomReadInput,
  timeout: number,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = host ? holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== holdingSlug)
    return Promise.reject(new NativeManufacturingError('NATIVE_MANUFACTURING_AUTHORITY_INVALID'));
  const headers = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeManufacturingError('NATIVE_MANUFACTURING_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  for (const [key, slug] of [
    ['x-erpx-workspace-branch', input.branchSlug],
    ['x-erpx-workspace-department', input.departmentSlug],
  ] as const) {
    if (slug !== undefined) {
      if (!text(slug, 128) || !/^[a-z0-9_-]+$/.test(slug))
        return Promise.reject(new NativeManufacturingError('NATIVE_MANUFACTURING_REQUEST_INVALID'));
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
      return Promise.reject(new NativeManufacturingError('NATIVE_MANUFACTURING_REQUEST_INVALID'));
    headers.set('x-erpx-context-company', input.expected.companyId);
    headers.set('x-erpx-context-assignment', input.expected.assignmentId);
    headers.set('x-erpx-context-revision', String(input.expected.revision));
    headers.set('x-erpx-context-assignment-version', String(input.expected.assignmentVersion));
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/manufacturing/specifications`,
    base,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, timeout);
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
        return failure('NATIVE_MANUFACTURING_RESPONSE_LIMIT');
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
    return failure('NATIVE_MANUFACTURING_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_MANUFACTURING_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_MANUFACTURING_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_MANUFACTURING_RESPONSE_INVALID');
  }
}

export function nativeManufacturingAvailable(): boolean {
  return process.env.ERPX_NATIVE_MANUFACTURING_CONFIG !== undefined;
}

export function nativeManufacturingConfiguration(raw: string): NativeManufacturingConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeManufacturingError('NATIVE_MANUFACTURING_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings)
  )
    return failure('NATIVE_MANUFACTURING_CONFIGURATION_INVALID');
  if (
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure('NATIVE_MANUFACTURING_CONFIGURATION_INVALID');
  return value as unknown as NativeManufacturingConfiguration;
}

export function createNativeManufacturingBomClient(
  raw: NativeManufacturingConfiguration,
  transport: NativeManufacturingTransport = directHttp,
) {
  const config = configuration(raw);
  const request = (
    incoming: Pick<Headers, 'get'>,
    actor: NativeManufacturingPageContext,
    input: NativeManufacturingBomReadInput,
    signal?: AbortSignal,
  ) => {
    if (!actor.authority || !actor.iam || actor.holdingSlug !== input.holdingSlug)
      return Promise.reject(new NativeManufacturingError('NATIVE_MANUFACTURING_CONTEXT_CHANGED'));
    return transportCall(
      transport,
      config.base,
      config.holdings,
      incoming,
      input.holdingSlug,
      input,
      config.timeout,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeManufacturingError) throw error;
      return failure('NATIVE_MANUFACTURING_UNAVAILABLE');
    });
  };
  return {
    bom: async (
      incoming: Pick<Headers, 'get'>,
      actor: NativeManufacturingPageContext,
      input: NativeManufacturingBomReadInput,
      signal?: AbortSignal,
    ): Promise<NativeManufacturingBom> => {
      const value = await boundedJson(await request(incoming, actor, input, signal));
      if (!exact(value, ['data', 'context']))
        return failure('NATIVE_MANUFACTURING_RESPONSE_INVALID');
      context(value.context, actor, input);
      return bomData(value.data, actor.companyId);
    },
  };
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
