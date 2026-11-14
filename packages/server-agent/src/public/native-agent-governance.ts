import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';

/** Strict owner transport for the native Agent governance router. */
export type NativeAgentGovernanceConfiguration = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
}>;

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): NativeAgentGovernanceConfiguration {
  const raw = process.env.ERPX_NATIVE_AGENT_CONFIG;
  if (raw === undefined)
    throw new ApiError(503, 'NATIVE_AGENT_UNAVAILABLE', 'The native Agent service is unavailable.');
  let value: NativeAgentGovernanceConfiguration;
  try {
    value = JSON.parse(raw) as NativeAgentGovernanceConfiguration;
  } catch {
    throw new ApiError(503, 'NATIVE_AGENT_CONFIGURATION_INVALID', 'Agent transport is misconfigured.');
  }
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    throw new ApiError(503, 'NATIVE_AGENT_CONFIGURATION_INVALID', 'Agent transport is misconfigured.');
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
    throw new ApiError(503, 'NATIVE_AGENT_CONFIGURATION_INVALID', 'Agent transport is misconfigured.');
  return value;
}

export function nativeAgentGovernanceAvailable(): boolean {
  return process.env.ERPX_NATIVE_AGENT_CONFIG !== undefined;
}

/** Dispatch one Agent governance command to the mounted native router. */
export async function nativeAgentCommand(
  request: Request,
  holdingSlug: string,
  body: unknown,
  key: string,
): Promise<unknown> {
  const config = configuration();
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
      `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingSlug)}/agents`,
      {
        method: 'POST',
        headers,
        body: JSON.stringify(body),
        cache: 'no-store',
        signal: AbortSignal.timeout(config.timeoutMs),
      },
    );
  } catch {
    throw new ApiError(503, 'NATIVE_AGENT_UNAVAILABLE', 'The native Agent service is unavailable.');
  }
  const payload = (await response.json().catch(() => null)) as
    | { data?: unknown; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok)
    throw new ApiError(
      response.status,
      payload?.error?.code ?? 'NATIVE_AGENT_FAILED',
      payload?.error?.message ?? 'The Agent request failed.',
    );
  return payload?.data;
}
