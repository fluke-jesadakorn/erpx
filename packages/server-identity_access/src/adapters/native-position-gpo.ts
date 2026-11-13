import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';

type Config = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
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

const COMMANDS = new Set([
  'preview',
  'publish',
  'unmap-preview',
  'unmap',
  'retry',
  'retry-config',
]);

function unavailable(): ApiError {
  return new ApiError(
    503,
    'NATIVE_POSITION_GPO_UNAVAILABLE',
    'The native Position GPO service is unavailable.',
  );
}

function configuration(): Config {
  const raw = process.env.ERPX_NATIVE_POSITION_GPO_CONFIG;
  if (raw === undefined) throw unavailable();
  let value: Config;
  try {
    value = JSON.parse(raw) as Config;
  } catch {
    throw new ApiError(
      503,
      'NATIVE_POSITION_GPO_CONFIGURATION_INVALID',
      'Position GPO transport is misconfigured.',
    );
  }
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    throw new ApiError(
      503,
      'NATIVE_POSITION_GPO_CONFIGURATION_INVALID',
      'Position GPO transport is misconfigured.',
    );
  }
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    base.username ||
    base.password ||
    value.privateBaseUrl !== base.origin ||
    !Number.isInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.some(
      (holding) =>
        !holding ||
        typeof holding.origin !== 'string' ||
        !holding.origin.startsWith('https://') ||
        typeof holding.holdingSlug !== 'string' ||
        holding.holdingSlug.length < 1 ||
        typeof holding.holdingId !== 'string' ||
        holding.holdingId.length < 1,
    )
  )
    throw new ApiError(
      503,
      'NATIVE_POSITION_GPO_CONFIGURATION_INVALID',
      'Position GPO transport is misconfigured.',
    );
  return value;
}

/** The native owner service hosts the Position GPO ingress when configured. */
export function nativePositionGpoAvailable(): boolean {
  return process.env.ERPX_NATIVE_POSITION_GPO_CONFIG !== undefined;
}

/**
 * One read: the Position GPO overview for the current Holding. Any other
 * technology read keeps using the existing owner reads until its own mount.
 */
export async function nativePositionGpoOverview(
  request: Request,
  holdingSlug: string,
): Promise<unknown> {
  const config = configuration();
  const headers = new Headers({ accept: 'application/json' });
  for (const name of CONTEXT_HEADERS) {
    const value = request.headers.get(name);
    if (value) headers.set(name, value);
  }
  let response: Response;
  try {
    response = await fetch(
      `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingSlug)}/technology/position-gpo`,
      {
        method: 'GET',
        headers,
        cache: 'no-store',
        signal: AbortSignal.timeout(config.timeoutMs),
      },
    );
  } catch {
    throw unavailable();
  }
  return payload(response);
}

/** One reviewed command: preview, publish, unmap, retry or retry-config. */
export async function nativePositionGpoCommand(
  request: Request,
  holdingSlug: string,
  operation: string,
  body: unknown,
  key: string,
): Promise<unknown> {
  if (!COMMANDS.has(operation))
    throw new ApiError(404, 'NOT_FOUND', 'Position GPO operation not found.');
  const config = configuration();
  const headers = new Headers({ accept: 'application/json', 'content-type': 'application/json' });
  for (const name of CONTEXT_HEADERS) {
    const value = request.headers.get(name);
    if (value) headers.set(name, value);
  }
  headers.set('idempotency-key', key);
  let response: Response;
  try {
    response = await fetch(
      `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingSlug)}/technology/position-gpo/${operation}`,
      {
        method: 'POST',
        headers,
        body: JSON.stringify(body),
        cache: 'no-store',
        signal: AbortSignal.timeout(config.timeoutMs),
      },
    );
  } catch {
    throw unavailable();
  }
  return payload(response);
}

async function payload(response: Response): Promise<unknown> {
  const value = (await response.json().catch(() => null)) as
    | { data?: unknown; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok)
    throw new ApiError(
      response.status,
      value?.error?.code ?? 'NATIVE_POSITION_GPO_FAILED',
      value?.error?.message ?? 'The Position GPO request failed.',
    );
  return value?.data;
}
