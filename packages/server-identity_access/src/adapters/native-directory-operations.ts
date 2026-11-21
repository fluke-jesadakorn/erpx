import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';

type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId: string }>;
type Configuration = Readonly<{
  base: URL;
  holdings: ReadonlyMap<string, Holding>;
  timeout: number;
}>;

const CONTEXT_HEADERS = [
  'cookie',
  'origin',
  'x-erpx-context-company',
  'x-erpx-context-assignment',
  'x-erpx-context-revision',
  'x-erpx-context-assignment-version',
  'x-erpx-workspace-branch',
  'x-erpx-workspace-department',
] as const;

const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configurationInvalid(): ApiError {
  return new ApiError(
    503,
    'NATIVE_DIRECTORY_OPERATION_CONFIGURATION_INVALID',
    'Directory operation transport is misconfigured.',
  );
}
function unavailable(): ApiError {
  return new ApiError(
    503,
    'NATIVE_DIRECTORY_OPERATION_UNAVAILABLE',
    'The native Directory operation service is unavailable.',
  );
}
function configuration(): Configuration {
  const raw = process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG;
  if (raw === undefined) throw unavailable();
  let value: { privateBaseUrl?: unknown; holdings?: unknown; timeoutMs?: unknown };
  try {
    value = JSON.parse(raw) as typeof value;
  } catch {
    throw configurationInvalid();
  }
  let base: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
  } catch {
    throw configurationInvalid();
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
    value.privateBaseUrl !== base.origin ||
    !Number.isInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    throw configurationInvalid();
  const holdings = new Map<string, Holding>();
  for (const item of value.holdings as unknown[]) {
    if (!item || typeof item !== 'object') throw configurationInvalid();
    const holding = item as Holding;
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      throw configurationInvalid();
    }
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      origin.username ||
      origin.password ||
      typeof holding.holdingId !== 'string' ||
      !/^[A-Za-z0-9_.:-]{1,200}$/.test(holding.holdingId) ||
      typeof holding.holdingSlug !== 'string' ||
      !/^[a-z0-9_-]{1,128}$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      throw configurationInvalid();
    holdings.set(origin.host, holding);
  }
  return { base, holdings, timeout: value.timeoutMs as number };
}

export function nativeDirectoryOperationsAvailable(): boolean {
  return process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG !== undefined;
}

function holdingFor(
  headers: Headers,
  holdingSlug: string,
  configuration: Configuration,
): Holding {
  const origin = headers.get('origin');
  let host = headers.get('host')?.split(':')[0];
  if (!host && origin) {
    try {
      host = new URL(origin).host.split(':')[0];
    } catch {
      throw unavailable();
    }
  }
  if (!host) throw unavailable();
  const holding = configuration.holdings.get(host);
  if (!holding || holding.holdingSlug !== holdingSlug) throw unavailable();
  return holding;
}

function requestHeaders(
  source: Headers,
  holding: Holding,
  idempotencyKey?: string,
): Headers {
  const headers = new Headers({
    host: new URL(holding.origin).host,
    accept: 'application/json',
    'content-type': 'application/json',
  });
  if (idempotencyKey) headers.set('idempotency-key', idempotencyKey);
  for (const name of CONTEXT_HEADERS) {
    const value = source.get(name);
    if (value) headers.set(name, value);
  }
  return headers;
}

function holdingById(holdingId: string, configuration: Configuration): Holding {
  for (const holding of configuration.holdings.values())
    if (holding.holdingId === holdingId) return holding;
  throw unavailable();
}

async function requestHolding(
  source: Headers,
  holding: Holding,
  path: string,
  method: 'GET' | 'POST',
  body?: unknown,
  key?: string,
): Promise<unknown> {
  const config = configuration();
  let response: Response;
  try {
    response = await fetch(
      `${config.base.origin}/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/iam/directory-operations${path}`,
      {
        method,
        headers: requestHeaders(source, holding, key),
        body: body === undefined ? undefined : JSON.stringify(body),
        cache: 'no-store',
        signal: AbortSignal.timeout(config.timeout),
      },
    );
  } catch {
    throw unavailable();
  }
  const value = (await response.json().catch(() => null)) as
    | { data?: unknown; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok)
    throw new ApiError(
      response.status,
      value?.error?.code ?? 'NATIVE_DIRECTORY_OPERATION_FAILED',
      value?.error?.message ?? 'The Directory operation request failed.',
    );
  return value?.data;
}

function request(
  source: Headers,
  holdingSlug: string,
  path: string,
  method: 'GET' | 'POST',
  body?: unknown,
  key?: string,
): Promise<unknown> {
  const config = configuration();
  const holding = holdingFor(source, holdingSlug, config);
  return requestHolding(source, holding, path, method, body, key);
}

/** Read the Directory automation workspace for a Holding slug. */
export function nativeDirectoryOperationsRead(
  headers: Headers,
  holdingSlug: string,
): Promise<unknown> {
  return request(headers, holdingSlug, '', 'GET');
}

export function nativeDirectoryOperationsRefresh(
  headers: Headers,
  holdingSlug: string,
  input: unknown,
  key: string,
): Promise<unknown> {
  return request(headers, holdingSlug, '/refresh', 'POST', input, key);
}

export function nativeDirectoryOperationsConfigure(
  headers: Headers,
  holdingSlug: string,
  input: unknown,
  key: string,
): Promise<unknown> {
  return request(headers, holdingSlug, '/configure', 'POST', input, key);
}

export function nativeDirectoryOperationsPreview(
  headers: Headers,
  holdingSlug: string,
  input: unknown,
): Promise<unknown> {
  return request(headers, holdingSlug, '/preview', 'POST', input);
}

export function nativeDirectoryOperationsSubmit(
  headers: Headers,
  holdingSlug: string,
  input: unknown,
  key: string,
  direct = false,
): Promise<unknown> {
  return request(
    headers,
    holdingSlug,
    direct ? '/apply' : '/submit',
    'POST',
    input,
    key,
  );
}

export function nativeDirectoryOperationsReview(
  headers: Headers,
  holdingSlug: string,
  payload: unknown,
  key: string,
): Promise<unknown> {
  return request(headers, holdingSlug, '/reviews/apply', 'POST', payload, key);
}

export function nativeDirectoryOperationsRetry(
  headers: Headers,
  holdingSlug: string,
  input: unknown,
  key: string,
): Promise<unknown> {
  return request(headers, holdingSlug, '/retry', 'POST', input, key);
}

export function nativeDirectoryOperationsCancel(
  headers: Headers,
  holdingSlug: string,
  input: unknown,
  key: string,
): Promise<unknown> {
  return request(headers, holdingSlug, '/cancel', 'POST', input, key);
}

/** Server-side transport by the selected Holding identity (no browser slug). */
export function nativeDirectoryOperationsReadForHolding(
  headers: Headers,
  holdingId: string,
): Promise<unknown> {
  return requestHolding(headers, holdingById(holdingId, configuration()), '', 'GET');
}

export function nativeDirectoryOperationsPreviewForHolding(
  headers: Headers,
  holdingId: string,
  input: unknown,
): Promise<unknown> {
  return requestHolding(
    headers,
    holdingById(holdingId, configuration()),
    '/preview',
    'POST',
    input,
  );
}

export function nativeDirectoryOperationsSubmitForHolding(
  headers: Headers,
  holdingId: string,
  input: unknown,
  key: string,
  direct = false,
): Promise<unknown> {
  return requestHolding(
    headers,
    holdingById(holdingId, configuration()),
    direct ? '/apply' : '/submit',
    'POST',
    input,
    key,
  );
}
