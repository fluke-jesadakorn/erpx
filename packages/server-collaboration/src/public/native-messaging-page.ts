import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';

/** Strict owner transport for the native Collaboration messaging reads. */
export type NativeMessagingConfiguration = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
}>;

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): NativeMessagingConfiguration {
  const raw = process.env.ERPX_NATIVE_COLLABORATION_CONFIG;
  if (raw === undefined)
    throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.');
  let value: NativeMessagingConfiguration;
  try {
    value = JSON.parse(raw) as NativeMessagingConfiguration;
  } catch {
    throw new ApiError(503, 'NATIVE_COLLABORATION_CONFIGURATION_INVALID', 'Collaboration transport is misconfigured.');
  }
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    throw new ApiError(503, 'NATIVE_COLLABORATION_CONFIGURATION_INVALID', 'Collaboration transport is misconfigured.');
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
    throw new ApiError(503, 'NATIVE_COLLABORATION_CONFIGURATION_INVALID', 'Collaboration transport is misconfigured.');
  return value;
}

export function nativeMessagingAvailable(): boolean {
  return process.env.ERPX_NATIVE_COLLABORATION_CONFIG !== undefined;
}

async function read(request: Request, holdingSlug: string, path: string): Promise<unknown> {
  const config = configuration();
  const headers = new Headers({ accept: 'application/json' });
  const cookie = request.headers.get('cookie');
  if (cookie) headers.set('cookie', cookie);
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
  let response: Response;
  try {
    response = await fetch(
      `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingSlug)}/messaging${path}`,
      { method: 'GET', headers, cache: 'no-store', signal: AbortSignal.timeout(config.timeoutMs) },
    );
  } catch {
    throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.');
  }
  const payload = (await response.json().catch(() => null)) as
    | { data?: unknown; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok)
    throw new ApiError(
      response.status,
      payload?.error?.code ?? 'NATIVE_COLLABORATION_FAILED',
      payload?.error?.message ?? 'The Collaboration request failed.',
    );
  return payload?.data;
}

export function nativeMessagingOverview(request: Request, holdingSlug: string) {
  return read(request, holdingSlug, '');
}

export function nativeMessagingMembers(
  request: Request,
  holdingSlug: string,
  id: string,
  isTeam: boolean,
) {
  return read(
    request,
    holdingSlug,
    `/${isTeam ? 'teams' : 'rooms'}/${encodeURIComponent(id)}/members`,
  );
}
