import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';

type Config = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
  timeoutMs: number;
}>;

function configuration(): Config {
  const raw = process.env.ERPX_NATIVE_IAM_CONFIG;
  if (raw === undefined)
    throw new ApiError(503, 'NATIVE_IAM_UNAVAILABLE', 'The native IAM service is unavailable.');
  let value: Config;
  try {
    value = JSON.parse(raw) as Config;
  } catch {
    throw new ApiError(503, 'NATIVE_IAM_CONFIGURATION_INVALID', 'IAM transport is misconfigured.');
  }
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    throw new ApiError(503, 'NATIVE_IAM_CONFIGURATION_INVALID', 'IAM transport is misconfigured.');
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
    value.holdings.length < 1
  )
    throw new ApiError(503, 'NATIVE_IAM_CONFIGURATION_INVALID', 'IAM transport is misconfigured.');
  return value;
}

export function nativeIamAvailable(): boolean {
  return process.env.ERPX_NATIVE_IAM_CONFIG !== undefined;
}

/** Dispatch one Holding IAM command to the mounted native router. */
export async function nativeIamCommand(
  request: Request,
  holdingSlug: string,
  path: string,
  body: unknown,
): Promise<unknown> {
  const config = configuration();
  const key = request.headers.get('Idempotency-Key');
  if (!key)
    throw new ApiError(422, 'COMMAND_KEY_REQUIRED', 'Idempotency-Key is required.');
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
      `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingSlug)}/iam/${path}`,
      { method: 'POST', headers, body: JSON.stringify(body), cache: 'no-store', signal: AbortSignal.timeout(config.timeoutMs) },
    );
  } catch {
    throw new ApiError(503, 'NATIVE_IAM_UNAVAILABLE', 'The native IAM service is unavailable.');
  }
  const payload = (await response.json().catch(() => null)) as
    | { data?: unknown; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok)
    throw new ApiError(
      response.status,
      payload?.error?.code ?? 'NATIVE_IAM_FAILED',
      payload?.error?.message ?? 'The IAM request failed.',
    );
  return payload?.data;
}
