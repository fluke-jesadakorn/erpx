import 'server-only';
import { Agent, request, type ProxyEnv } from 'node:http';
import { Readable } from 'node:stream';
import { allowedPrivateLoopbackPort } from '@erpx/contracts/private-loopback';
import type {
  CompanyPackageGrant,
  CompanyPackageHistoryRight,
  NativeServicePackageOverview,
  PackageDefinition,
} from '@erpx/contracts/commercial-service';

/** Rendering input from retained Platform Commercial authorization, never a grant. */
export type NativeCompanyPackageActor = Readonly<{
  userId: string;
  platformAssignmentId: string;
  platformRole: string;
  roleRevision: number;
  modules: readonly string[];
  permissions: readonly string[];
}>;

export type NativeCompanyPackagesConfiguration = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  production?: boolean;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
}>;

type Row = Record<string, unknown>;
export type NativeCompanyPackagesTransport = (
  method: 'GET',
  url: URL,
  headers: Headers,
  body: undefined,
  signal: AbortSignal,
) => Promise<Response>;

export class NativeCompanyPackagesError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? 'The native Commercial package request could not be completed.');
    this.name = 'NativeCompanyPackagesError';
    this.code = code;
    this.status = status;
  }
}

const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeCompanyPackagesError(code, status, message);
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
  text(value, 200) && value.length > 0 && !/[\u0000-\u001f\u007f]/u.test(value);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const texts = (value: unknown, max = 256): value is string[] =>
  Array.isArray(value) && value.length <= max && value.every((item) => text(item, 256));
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left);
  const b = new Set(right);
  return a.size === b.size && [...a].every((value) => b.has(value));
};

const packageDefinition = (value: unknown): value is PackageDefinition =>
  exact(value, [
    'id',
    'version',
    'productId',
    'kind',
    'visibleCapabilities',
    'includedCapabilities',
    'dependencies',
  ]) &&
  identifier(value.id) &&
  integer(value.version, 1) &&
  identifier(value.productId) &&
  (value.kind === 'base' || value.kind === 'addon') &&
  texts(value.visibleCapabilities) &&
  texts(value.includedCapabilities) &&
  texts(value.dependencies);

const packageGrant = (value: unknown, companyId: string): value is CompanyPackageGrant =>
  exact(value, [
    'companyId',
    'packageId',
    'packageVersion',
    'state',
    'startsAt',
    'endsAt',
    'version',
    'reason',
    'evidence',
  ]) &&
  value.companyId === companyId &&
  identifier(value.packageId) &&
  integer(value.packageVersion, 1) &&
  (value.state === 'active' || value.state === 'draining' || value.state === 'inactive') &&
  integer(value.startsAt) &&
  (value.endsAt === null ||
    (integer(value.endsAt) && (value.endsAt as number) > (value.startsAt as number))) &&
  integer(value.version, 1) &&
  text(value.reason, 1000) &&
  text(value.evidence, 1000);

const packageHistory = (
  value: unknown,
  companyId: string,
): value is CompanyPackageHistoryRight =>
  exact(value, [
    'companyId',
    'packageId',
    'packageVersion',
    'sourceGrantVersion',
    'sourceEventId',
    'retainedAt',
  ]) &&
  value.companyId === companyId &&
  identifier(value.packageId) &&
  integer(value.packageVersion, 1) &&
  integer(value.sourceGrantVersion, 1) &&
  identifier(value.sourceEventId) &&
  integer(value.retainedAt);

function packageOverview(
  value: unknown,
  holding: Readonly<{ origin: string; holdingSlug: string; holdingId: string }>,
  companyId: string,
  actor: NativeCompanyPackageActor,
  context: unknown,
): NativeServicePackageOverview {
  if (
    !exact(value, [
      'companyId',
      'revision',
      'packages',
      'billingMode',
      'observedAt',
      'grants',
      'history',
    ]) ||
    value.companyId !== companyId ||
    typeof value.revision !== 'string' ||
    !/^[a-f0-9]{64}$/.test(value.revision) ||
    !Array.isArray(value.packages) ||
    value.packages.length < 1 ||
    value.packages.length > 1024 ||
    !value.packages.every(packageDefinition) ||
    typeof value.billingMode !== 'string' ||
    !/^[a-z][a-z0-9_-]{0,39}$/.test(value.billingMode) ||
    !integer(value.observedAt) ||
    !Array.isArray(value.grants) ||
    value.grants.length > 1024 ||
    !value.grants.every((grant) => packageGrant(grant, companyId)) ||
    !Array.isArray(value.history) ||
    value.history.length > 4096 ||
    !value.history.every((history) => packageHistory(history, companyId))
  )
    return failure('NATIVE_COMMERCIAL_RESPONSE_INVALID');
  const packages = new Set<string>();
  for (const definition of value.packages) {
    const key = `${definition.id}:${definition.version}`;
    if (packages.has(key)) return failure('NATIVE_COMMERCIAL_RESPONSE_INVALID');
    packages.add(key);
  }
  const grants = new Set<string>();
  for (const grant of value.grants) {
    if (grants.has(grant.packageId)) return failure('NATIVE_COMMERCIAL_RESPONSE_INVALID');
    grants.add(grant.packageId);
  }
  const history = new Set<string>();
  for (const right of value.history) {
    const key = `${right.packageId}:${right.packageVersion}`;
    if (history.has(key)) return failure('NATIVE_COMMERCIAL_RESPONSE_INVALID');
    history.add(key);
  }
  const current = contextValue(context, holding, companyId, actor);
  if (!current) return failure('NATIVE_COMMERCIAL_CONTEXT_CHANGED');
  return value as unknown as NativeServicePackageOverview;
}

function contextValue(
  value: unknown,
  holding: Readonly<{ origin: string; holdingSlug: string; holdingId: string }>,
  companyId: string,
  actor: NativeCompanyPackageActor,
): boolean {
  if (
    !exact(value, [
      'userId',
      'holdingId',
      'holdingSlug',
      'companyId',
      'platformAssignmentId',
      'platformRole',
      'roleRevision',
      'modules',
      'permissions',
      'expiresAt',
    ]) ||
    !identifier(value.userId) ||
    !identifier(value.holdingId) ||
    !identifier(value.holdingSlug) ||
    !identifier(value.companyId) ||
    !identifier(value.platformAssignmentId) ||
    !text(value.platformRole, 80) ||
    !integer(value.roleRevision, 1) ||
    !texts(value.modules, 512) ||
    !texts(value.permissions, 1024) ||
    !integer(value.expiresAt, 1)
  )
    return false;
  return (
    value.userId === actor.userId &&
    value.holdingId === holding.holdingId &&
    value.holdingSlug === holding.holdingSlug &&
    value.companyId === companyId &&
    value.platformAssignmentId === actor.platformAssignmentId &&
    value.platformRole === actor.platformRole &&
    value.roleRevision === actor.roleRevision &&
    sameSet(actor.modules, value.modules) &&
    sameSet(actor.permissions, value.permissions)
  );
}

export function nativeCompanyPackagesConfigured(): boolean {
  return process.env.ERPX_NATIVE_COMMERCIAL_CONFIG !== undefined;
}

export function nativeCompanyPackagesConfiguration(
  raw: string,
): NativeCompanyPackagesConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return failure('NATIVE_COMMERCIAL_CONFIGURATION_INVALID');
  }
  if (
    !record(value) ||
    (value.production !== undefined && typeof value.production !== 'boolean') ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings)
  )
    return failure('NATIVE_COMMERCIAL_CONFIGURATION_INVALID');
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl as string);
  } catch {
    return failure('NATIVE_COMMERCIAL_CONFIGURATION_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    !allowedPrivateLoopbackPort(port, value.production === true) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30000 ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return failure('NATIVE_COMMERCIAL_CONFIGURATION_INVALID');
  const hosts = new Set<string>();
  const slugs = new Set<string>();
  for (const holding of value.holdings) {
    if (
      !exact(holding, ['origin', 'holdingSlug', 'holdingId']) ||
      !text(holding.origin) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holding.holdingSlug) ||
      !identifier(holding.holdingId)
    )
      return failure('NATIVE_COMMERCIAL_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(holding.origin as string);
    } catch {
      return failure('NATIVE_COMMERCIAL_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.port ||
      origin.username ||
      origin.password ||
      origin.origin !== holding.origin ||
      hosts.has(origin.host) ||
      slugs.has(holding.holdingSlug)
    )
      return failure('NATIVE_COMMERCIAL_CONFIGURATION_INVALID');
    hosts.add(origin.host);
    slugs.add(holding.holdingSlug);
  }
  return value as unknown as NativeCompanyPackagesConfiguration;
}

function transportCall(
  transport: NativeCompanyPackagesTransport,
  config: NativeCompanyPackagesConfiguration,
  incoming: Pick<Headers, 'get'>,
  actor: NativeCompanyPackageActor,
  holdingId: string,
  companyId: string,
  suffix: 'packages' | 'subscriptions' = 'packages',
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get('host');
  const holding = config.holdings.find((entry) => entry.holdingId === holdingId);
  if (!holding || !host || new URL(holding.origin).host !== host)
    return Promise.reject(new NativeCompanyPackagesError('NATIVE_COMMERCIAL_AUTHORITY_INVALID'));
  if (!identifier(companyId))
    return Promise.reject(new NativeCompanyPackagesError('NATIVE_COMMERCIAL_REQUEST_INVALID'));
  const headers = new Headers({
    host,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-platform-assignment': actor.platformAssignmentId,
    'x-erpx-platform-role': actor.platformRole,
    'x-erpx-context-company': companyId,
    'x-erpx-context-revision': String(actor.roleRevision),
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeCompanyPackagesError('NATIVE_COMMERCIAL_COOKIE_LIMIT'));
    headers.set('cookie', cookie);
  }
  const origin = incoming.get('origin');
  if (origin) headers.set('origin', origin);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/commercial/companies/${encodeURIComponent(companyId)}/${suffix}`,
    config.privateBaseUrl,
  );
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
        return failure('NATIVE_COMMERCIAL_RESPONSE_LIMIT');
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
    return failure('NATIVE_COMMERCIAL_UNAVAILABLE', response.status);
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
    return failure(code ?? 'NATIVE_COMMERCIAL_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return failure('NATIVE_COMMERCIAL_RESPONSE_INVALID');
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return failure('NATIVE_COMMERCIAL_RESPONSE_INVALID');
  }
}

export type NativeCompanyPackagesClient = ((
  incoming: Pick<Headers, 'get'>,
  actor: NativeCompanyPackageActor,
  holdingId: string,
  companyId: string,
  signal?: AbortSignal,
) => Promise<NativeServicePackageOverview>) & {
  subscriptions: (
    incoming: Pick<Headers, 'get'>,
    actor: NativeCompanyPackageActor,
    holdingId: string,
    companyId: string,
    signal?: AbortSignal,
  ) => Promise<Record<string, unknown>>;
};

export function createNativeCompanyPackagesClient(
  raw: NativeCompanyPackagesConfiguration,
  transport: NativeCompanyPackagesTransport = directHttp,
): NativeCompanyPackagesClient {
  const config = nativeCompanyPackagesConfiguration(JSON.stringify(raw));
  const overview = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeCompanyPackageActor,
    holdingId: string,
    companyId: string,
    signal?: AbortSignal,
  ): Promise<NativeServicePackageOverview> => {
    if (
      !identifier(actor.userId) ||
      !identifier(actor.platformAssignmentId) ||
      !text(actor.platformRole, 80) ||
      !integer(actor.roleRevision, 1) ||
      !texts(actor.modules, 512) ||
      !texts(actor.permissions, 1024)
    )
      return failure('NATIVE_COMMERCIAL_REQUEST_INVALID');
    const holding = config.holdings.find((entry) => entry.holdingId === holdingId);
    if (!holding)
      return failure('NATIVE_COMMERCIAL_AUTHORITY_INVALID');
    const value = await boundedJson(
      await transportCall(
        transport,
        config,
        incoming,
        actor,
        holdingId,
        companyId,
        'packages',
        signal,
      ).catch((error: unknown) => {
        if (error instanceof NativeCompanyPackagesError) throw error;
        return failure('NATIVE_COMMERCIAL_UNAVAILABLE');
      }),
    );
    if (!exact(value, ['data', 'context']))
      return failure('NATIVE_COMMERCIAL_RESPONSE_INVALID');
    return packageOverview(value.data, holding, companyId, actor, value.context);
  };
  const subscriptions = async (
    incoming: Pick<Headers, 'get'>,
    actor: NativeCompanyPackageActor,
    holdingId: string,
    companyId: string,
    signal?: AbortSignal,
  ): Promise<Record<string, unknown>> => {
    const holding = config.holdings.find((entry) => entry.holdingId === holdingId);
    if (!holding) return failure('NATIVE_COMMERCIAL_AUTHORITY_INVALID');
    const value = await boundedJson(
      await transportCall(
        transport,
        config,
        incoming,
        actor,
        holdingId,
        companyId,
        'subscriptions',
        signal,
      ).catch((error: unknown) => {
        if (error instanceof NativeCompanyPackagesError) throw error;
        return failure('NATIVE_COMMERCIAL_UNAVAILABLE');
      }),
    );
    if (!exact(value, ['data', 'context']) || !record(value.data))
      return failure('NATIVE_COMMERCIAL_RESPONSE_INVALID');
    if (
      value.data.companyId !== companyId ||
      typeof value.data.revision !== 'string' ||
      !integer(value.data.observedAt) ||
      !Array.isArray(value.data.subscriptions) ||
      !Array.isArray(value.data.history) ||
      !Array.isArray(value.data.transitions)
    )
      return failure('NATIVE_COMMERCIAL_RESPONSE_INVALID');
    if (!contextValue(value.context, holding, companyId, actor))
      return failure('NATIVE_COMMERCIAL_CONTEXT_CHANGED');
    return value.data;
  };
  return Object.assign(overview, { subscriptions });
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
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as ProxyEnv,
    });
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

export function nativeCompanyPackagesFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeCompanyPackagesError)) return undefined;
  if (error.code === 'NATIVE_COMMERCIAL_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your Provider context changed. Reload and review before retrying.',
    };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403)
    return {
      status: 403,
      code: error.code === 'NATIVE_COMMERCIAL_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return { status: 404, code: 'NOT_FOUND', message: 'Packages not found in this scope.' };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_COMMERCIAL_UNAVAILABLE',
      message: 'The native Commercial package service is unavailable.',
    };
  if (
    error.code === 'NATIVE_COMMERCIAL_CONFIGURATION_INVALID' ||
    error.code === 'NATIVE_COMMERCIAL_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_COMMERCIAL_COOKIE_LIMIT' ||
    error.code === 'NATIVE_COMMERCIAL_RESPONSE_INVALID' ||
    error.code === 'NATIVE_COMMERCIAL_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_COMMERCIAL_UNAVAILABLE',
      message: 'The native Commercial package reader is unavailable.',
    };
  if (error.code === 'NATIVE_COMMERCIAL_REQUEST_INVALID')
    return {
      status: 422,
      code: 'COMMERCIAL_REQUEST_INVALID',
      message: 'Review the Company package request.',
    };
  return undefined;
}
