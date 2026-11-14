import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';

/** Strict owner transport for the native Staff Work task reads/commands. */
export type NativeStaffWorkConfiguration = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
}>;

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): NativeStaffWorkConfiguration {
  const raw = process.env.ERPX_NATIVE_OPERATIONS_CONFIG;
  if (raw === undefined)
    throw new ApiError(503, 'NATIVE_OPERATIONS_UNAVAILABLE', 'The native Operations service is unavailable.');
  let value: NativeStaffWorkConfiguration;
  try {
    value = JSON.parse(raw) as NativeStaffWorkConfiguration;
  } catch {
    throw new ApiError(503, 'NATIVE_OPERATIONS_CONFIGURATION_INVALID', 'Operations transport is misconfigured.');
  }
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    throw new ApiError(503, 'NATIVE_OPERATIONS_CONFIGURATION_INVALID', 'Operations transport is misconfigured.');
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
    throw new ApiError(503, 'NATIVE_OPERATIONS_CONFIGURATION_INVALID', 'Operations transport is misconfigured.');
  return value;
}

export function nativeStaffWorkAvailable(): boolean {
  return process.env.ERPX_NATIVE_OPERATIONS_CONFIG !== undefined;
}

function forwarded(request: Request, json: boolean): Headers {
  const headers = new Headers(json ? { accept: 'application/json', 'content-type': 'application/json' } : { accept: 'application/json' });
  const cookie = request.headers.get('cookie');
  if (cookie) headers.set('cookie', cookie);
  const origin = request.headers.get('origin');
  if (origin) headers.set('origin', origin);
  for (const name of [
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

async function call(
  request: Request,
  holdingSlug: string,
  path: string,
  init: Readonly<{ method: 'GET' | 'POST'; body?: unknown; key?: string }>,
): Promise<unknown> {
  const config = configuration();
  const headers = forwarded(request, init.method === 'POST');
  if (init.key) headers.set('idempotency-key', init.key);
  let response: Response;
  try {
    response = await fetch(
      `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingSlug)}/staff-tasks${path}`,
      {
        method: init.method,
        headers,
        body: init.method === 'POST' ? JSON.stringify(init.body ?? {}) : undefined,
        cache: 'no-store',
        signal: AbortSignal.timeout(config.timeoutMs),
      },
    );
  } catch {
    throw new ApiError(503, 'NATIVE_OPERATIONS_UNAVAILABLE', 'The native Operations service is unavailable.');
  }
  const payload = (await response.json().catch(() => null)) as
    | { data?: unknown; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok)
    throw new ApiError(
      response.status,
      payload?.error?.code ?? 'NATIVE_OPERATIONS_FAILED',
      payload?.error?.message ?? 'The Operations request failed.',
    );
  return payload?.data;
}

export function nativeStaffWorkRead(
  request: Request,
  holdingSlug: string,
  path: string,
  projectId: string | null,
): Promise<unknown> {
  const suffix = projectId ? `?projectId=${encodeURIComponent(projectId)}` : '';
  return call(request, holdingSlug, `${path}${suffix}`, { method: 'GET' });
}

export function nativeStaffWorkCommand(
  request: Request,
  holdingSlug: string,
  path: string,
  body: unknown,
  key: string,
): Promise<unknown> {
  return call(request, holdingSlug, path, { method: 'POST', body, key });
}
