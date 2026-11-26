import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import { nativeRequest } from '@erpx/server-runtime/public/native-http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

/**
 * Fail-closed transport for the mounted native `GET /push` settings read.
 * The owner owns preference/session-count SQL and the public key; this bridge
 * sends the authenticated Company/user/session and no fallback read exists.
 */
type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId: string }>;
type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;
export type NativePushSettings = Readonly<{
  available: boolean;
  publicKey: string | null;
  preferences: unknown;
  deviceCount: number;
}>;
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const unavailable = (): never => {
  throw new ApiError(503, 'NATIVE_PUSH_SETTINGS_UNAVAILABLE', 'The native push settings read is unavailable.');
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown): value is string =>
  typeof value === 'string' && value.length > 0 && value.length <= 2000;

function settings(): Settings {
  const raw = process.env.ERPX_NATIVE_COLLABORATION_CONFIG;
  if (raw === undefined) return unavailable();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable();
  }
  if (
    !record(value) ||
    typeof value.privateBaseUrl !== 'string' ||
    typeof value.timeoutMs !== 'number' ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings)
  )
    return unavailable();
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable();
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
    value.privateBaseUrl !== base.origin
  )
    return unavailable();
  const holdings: Holding[] = [];
  for (const item of value.holdings) {
    if (!record(item) || !text(item.origin) || !text(item.holdingSlug) || !text(item.holdingId))
      return unavailable();
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return unavailable();
    }
    if (origin.protocol !== 'https:' || origin.username || origin.password) return unavailable();
    holdings.push({ origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId });
  }
  return { privateBaseUrl: value.privateBaseUrl, timeoutMs: value.timeoutMs, holdings };
}

function view(value: unknown): NativePushSettings {
  if (
    !record(value) ||
    typeof value.available !== 'boolean' ||
    !(value.publicKey === null || typeof value.publicKey === 'string') ||
    !('preferences' in value) ||
    typeof value.deviceCount !== 'number' ||
    !Number.isSafeInteger(value.deviceCount) ||
    value.deviceCount < 0
  )
    return unavailable();
  return {
    available: value.available,
    publicKey: value.publicKey as string | null,
    preferences: value.preferences,
    deviceCount: value.deviceCount,
  };
}

/** Read the current Account push settings for the selected Holding. */
export async function nativePushSettings(actor: WorkContext): Promise<NativePushSettings> {
  const config = settings();
  const holding = config.holdings.find((item) => item.holdingId === actor.holdingId);
  if (!holding || holding.holdingSlug !== actor.holdingSlug) return unavailable();
  const session = actor.authority?.sessionId;
  if (!text(session)) return unavailable();
  let response: Response;
  try {
    response = await nativeRequest(
      new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/push`, config.privateBaseUrl),
      {
        method: 'GET',
        headers: {
          host: new URL(holding.origin).host,
          origin: holding.origin,
          accept: 'application/json',
          'x-erpx-context-company': actor.companyId,
          'x-erpx-context-user': actor.userId,
          'x-erpx-context-session': session,
        },
      },
      config.timeoutMs,
    );
  } catch {
    return unavailable();
  }
  const payload: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const error = record(payload) && record(payload.error) ? payload.error : undefined;
    throw new ApiError(
      response.status,
      text(error?.code) ? (error.code as string) : 'NATIVE_PUSH_SETTINGS_FAILED',
      text(error?.message) ? (error.message as string) : 'The native push settings read was rejected.',
    );
  }
  if (!record(payload) || !('data' in payload)) return unavailable();
  return view(payload.data);
}
