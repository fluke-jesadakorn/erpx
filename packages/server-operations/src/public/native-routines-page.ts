import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';

/**
 * Strict owner transport for the native Operations routine template/schedule
 * commands. Reads stay on the retained Work projection path until their own
 * native slice lands; only the covered command shapes are forwarded.
 */
export type NativeRoutinesConfiguration = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
}>;

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): NativeRoutinesConfiguration {
  const raw = process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG;
  if (raw === undefined)
    throw new ApiError(
      503,
      'NATIVE_ROUTINES_UNAVAILABLE',
      'The native Operations routine command is unavailable.',
    );
  let value: NativeRoutinesConfiguration;
  try {
    value = JSON.parse(raw) as NativeRoutinesConfiguration;
  } catch {
    throw new ApiError(
      503,
      'NATIVE_ROUTINES_CONFIGURATION_INVALID',
      'Routine transport is misconfigured.',
    );
  }
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    throw new ApiError(
      503,
      'NATIVE_ROUTINES_CONFIGURATION_INVALID',
      'Routine transport is misconfigured.',
    );
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
    value.privateBaseUrl !== base.origin ||
    !Number.isInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000
  )
    throw new ApiError(
      503,
      'NATIVE_ROUTINES_CONFIGURATION_INVALID',
      'Routine transport is misconfigured.',
    );
  return value;
}

export function nativeRoutinesAvailable(): boolean {
  return process.env.ERPX_NATIVE_OPERATIONS_ROUTINES_CONFIG !== undefined;
}

function forwarded(request: Request): Headers {
  const headers = new Headers({ accept: 'application/json', 'content-type': 'application/json' });
  for (const name of [
    'host',
    'cookie',
    'origin',
    'x-erpx-context-company',
    'x-erpx-context-assignment',
    'x-erpx-context-revision',
    'x-erpx-context-assignment-version',
    'x-erpx-workspace-branch',
    'x-erpx-workspace-department',
  ]) {
    const value = request.headers.get(name);
    if (value) headers.set(name, value);
  }
  return headers;
}

/** Forward one covered routine command. `resource` selects the retained
 * `routine-templates` or `routine-schedules` route family and `path` is the
 * already-encoded suffix after the resource (for example `/template-1/clone`). */
export async function nativeRoutineCommand(
  request: Request,
  holdingSlug: string,
  resource: 'routine-templates' | 'routine-schedules',
  path: string,
  body: unknown,
  key: string,
): Promise<unknown> {
  const config = configuration();
  const headers = forwarded(request);
  headers.set('idempotency-key', key);
  let response: Response;
  try {
    response = await fetch(
      `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingSlug)}/${resource}${path}`,
      {
        method: 'POST',
        headers,
        body: JSON.stringify(body ?? {}),
        cache: 'no-store',
        signal: AbortSignal.timeout(config.timeoutMs),
      },
    );
  } catch {
    throw new ApiError(
      503,
      'NATIVE_ROUTINES_UNAVAILABLE',
      'The native Operations routine command is unavailable.',
    );
  }
  const payload = (await response.json().catch(() => null)) as
    | { data?: unknown; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok)
    throw new ApiError(
      response.status,
      payload?.error?.code ?? 'NATIVE_ROUTINES_FAILED',
      payload?.error?.message ?? 'The routine command failed.',
    );
  return payload?.data;
}
