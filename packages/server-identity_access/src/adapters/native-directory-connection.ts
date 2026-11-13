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
    'NATIVE_DIRECTORY_CONNECTION_CONFIGURATION_INVALID',
    'Directory connection transport is misconfigured.',
  );
}

function unavailable(): ApiError {
  return new ApiError(
    503,
    'NATIVE_DIRECTORY_CONNECTION_UNAVAILABLE',
    'The native Directory connection service is unavailable.',
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

export function nativeDirectoryConnectionAvailable(): boolean {
  return process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG !== undefined;
}

function holdingFor(
  request: Request,
  holdingSlug: string,
  configuration: Configuration,
): Holding {
  const host = request.headers.get('host');
  const holding = host ? configuration.holdings.get(host) : undefined;
  if (!holding || holding.holdingSlug !== holdingSlug)
    throw new ApiError(
      403,
      'NATIVE_DIRECTORY_CONNECTION_AUTHORITY_INVALID',
      'Use the registered Holding host.',
    );
  return holding;
}

function headers(
  request: Request,
  holding: Holding,
  idempotencyKey: string,
): Headers {
  const headers = new Headers({
    host: new URL(holding.origin).host,
    accept: 'application/json',
    'content-type': 'application/json',
    'idempotency-key': idempotencyKey,
  });
  for (const name of CONTEXT_HEADERS) {
    const value = request.headers.get(name);
    if (value) headers.set(name, value);
  }
  return headers;
}

/** One reviewed Directory connection command. */
export async function nativeDirectoryConnectionCommand(
  request: Request,
  holdingSlug: string,
  operation: string,
  id: string | null,
  body: unknown,
  key: string,
): Promise<unknown> {
  if (!['save', 'test', 'activate', 'disable'].includes(operation))
    throw new ApiError(404, 'NOT_FOUND', 'Connection action not found.');
  const config = configuration();
  const holding = holdingFor(request, holdingSlug, config);
  const suffix =
    operation === 'save'
      ? ''
      : `/${encodeURIComponent(String(id))}/${encodeURIComponent(operation)}`;
  let response: Response;
  try {
    response = await fetch(
      `${config.base.origin}/api/v1/holdings/${encodeURIComponent(holdingSlug)}/iam/connections${suffix}`,
      {
        method: 'POST',
        headers: headers(request, holding, key),
        body: JSON.stringify(body),
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
      value?.error?.code ?? 'NATIVE_DIRECTORY_CONNECTION_FAILED',
      value?.error?.message ?? 'The Directory connection request failed.',
    );
  return value?.data;
}
