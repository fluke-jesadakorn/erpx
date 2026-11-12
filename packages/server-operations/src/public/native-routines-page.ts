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
  return (await nativeRoutineEnvelope(request, holdingSlug, resource, path, body, key)).data;
}

export type NativeRoutineRunReceipt = Readonly<{
  id: string;
  state: 'draft' | 'start_approval' | 'ready';
  version: number;
}>;

function contextChanged(request: Request, context: unknown): boolean {
  if (!context || typeof context !== 'object' || Array.isArray(context))
    return true;
  const value = context as Record<string, unknown>;
  const expected: Array<[string, unknown]> = [
    ['companyId', request.headers.get('x-erpx-context-company')],
    ['assignmentId', request.headers.get('x-erpx-context-assignment')],
    ['revision', request.headers.get('x-erpx-context-revision')],
    ['assignmentVersion', request.headers.get('x-erpx-context-assignment-version')],
  ];
  return expected.some(([name, header]) => {
    if (header === null) return typeof value[name] !== 'string' && typeof value[name] !== 'number';
    return String(value[name]) !== header;
  });
}

/** Forward the manual run and validate the exact retained receipt and selected
 * work context. A malformed acknowledgement never reaches the caller. */
export async function nativeRoutineRun(
  request: Request,
  holdingSlug: string,
  templateId: string,
  body: unknown,
  key: string,
): Promise<NativeRoutineRunReceipt> {
  const envelope = await nativeRoutineEnvelope(
    request,
    holdingSlug,
    'routine-templates',
    `/${encodeURIComponent(templateId)}/run`,
    body,
    key,
  );
  const data = envelope.data;
  if (
    !data ||
    typeof data !== 'object' ||
    Array.isArray(data) ||
    !('id' in data) ||
    !('state' in data) ||
    !('version' in data) ||
    data.id !== `task-${key}` ||
    data.version !== 1 ||
    typeof data.state !== 'string' ||
    !['draft', 'start_approval', 'ready'].includes(data.state)
  )
    throw new ApiError(
      503,
      'NATIVE_ROUTINES_RESPONSE_INVALID',
      'The native Operations routine response is invalid.',
    );
  if (contextChanged(request, envelope.context))
    throw new ApiError(
      503,
      'NATIVE_ROUTINES_RESPONSE_INVALID',
      'The native Operations routine response is invalid.',
    );
  return data as NativeRoutineRunReceipt;
}

async function nativeRoutineEnvelope(
  request: Request,
  holdingSlug: string,
  resource: 'routine-templates' | 'routine-schedules',
  path: string,
  body: unknown,
  key: string,
): Promise<{ data: unknown; context: unknown }> {
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
    | { data?: unknown; context?: unknown; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok)
    throw new ApiError(
      response.status,
      payload?.error?.code ?? 'NATIVE_ROUTINES_FAILED',
      payload?.error?.message ?? 'The routine command failed.',
    );
  return { data: payload?.data, context: payload?.context };
}
