import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';

export type NativeTalentConfiguration = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
}>;

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): NativeTalentConfiguration {
  const raw = process.env.ERPX_NATIVE_TALENT_CONFIG;
  if (raw === undefined)
    throw new ApiError(503, 'NATIVE_TALENT_UNAVAILABLE', 'The native Talent service is unavailable.');
  let value: NativeTalentConfiguration;
  try {
    value = JSON.parse(raw) as NativeTalentConfiguration;
  } catch {
    throw new ApiError(503, 'NATIVE_TALENT_CONFIGURATION_INVALID', 'Talent transport is misconfigured.');
  }
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    throw new ApiError(503, 'NATIVE_TALENT_CONFIGURATION_INVALID', 'Talent transport is misconfigured.');
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
    throw new ApiError(503, 'NATIVE_TALENT_CONFIGURATION_INVALID', 'Talent transport is misconfigured.');
  return value;
}

export function nativeTalentLifecycleAvailable(): boolean {
  return process.env.ERPX_NATIVE_TALENT_CONFIG !== undefined;
}

function nativeRoute(input: Record<string, unknown>): { path: string; id: string | null } | null {
  const entity = input.entity;
  const id = typeof input.id === 'string' && input.id.length > 0 ? input.id : null;
  if (entity === 'candidate') return { path: '/talent/candidates', id };
  if (entity === 'candidate-privacy') return { path: '', id };
  if (entity === 'application') return { path: '/talent/applications', id };
  if (entity === 'proposal') return { path: '/talent/proposals', id };
  return null;
}

export async function nativeTalentLifecycleCommand(
  request: Request,
  holdingSlug: string,
  input: Record<string, unknown>,
  key: string,
): Promise<unknown | undefined> {
  const route = nativeRoute(input);
  if (!route) return undefined;
  const config = configuration();
  const entity = input.entity;
  let path: string;
  if (entity === 'candidate-privacy') {
    const id = route.id;
    if (!id) return undefined;
    path = `/talent/candidates/${encodeURIComponent(id)}/privacy`;
  } else {
    path = route.id
      ? `${route.path}/${encodeURIComponent(route.id)}`
      : route.path;
  }
  const { entity: _entity, id: _id, ...body } = input;
  void _entity;
  void _id;
  const headers = new Headers({ accept: 'application/json', 'content-type': 'application/json' });
  const cookie = request.headers.get('cookie');
  const origin = request.headers.get('origin');
  if (cookie) headers.set('cookie', cookie);
  if (origin) headers.set('origin', origin);
  headers.set('idempotency-key', key);
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
      `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingSlug)}${path}`,
      {
        method: 'POST',
        headers,
        body: JSON.stringify(body),
        cache: 'no-store',
        signal: AbortSignal.timeout(config.timeoutMs),
      },
    );
  } catch {
    throw new ApiError(503, 'NATIVE_TALENT_UNAVAILABLE', 'The native Talent service is unavailable.');
  }
  const payload = (await response.json().catch(() => null)) as
    | { data?: unknown; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok)
    throw new ApiError(
      response.status,
      payload?.error?.code ?? 'NATIVE_TALENT_FAILED',
      payload?.error?.message ?? 'The Talent request failed.',
    );
  return payload?.data;
}
