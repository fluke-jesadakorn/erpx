import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import { nativeRequest } from '@erpx/platform-runtime-server/public/native-http';
import type { AccountProfile } from '@erpx/identity-access-server/domain/account-preferences';

/**
 * Native-only transport for the Account profile read. The axum owner router
 * (`account_profile_read_router`) owns the SQL, the transaction and the exact
 * `{ data: profile }` projection. There is no TypeScript query, plan or
 * fallback: a missing or malformed native runtime fails closed with 503.
 */
type NativeConfig = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  timeoutMs: number;
}>;

function nativeConfig(): NativeConfig {
  const raw = process.env.ERPX_NATIVE_ACCOUNT_PROFILE_CONFIG;
  if (raw === undefined)
    throw new ApiError(503, 'ACCOUNT_PROFILE_UNAVAILABLE', 'The native Account profile service is not configured.');
  let value: {
    privateBaseUrl?: unknown;
    host?: unknown;
    holdingSlug?: unknown;
    timeoutMs?: unknown;
  };
  try {
    value = JSON.parse(raw) as typeof value;
  } catch {
    throw new ApiError(503, 'ACCOUNT_PROFILE_CONFIGURATION_INVALID', 'The native Account profile transport is misconfigured.');
  }
  let base: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
  } catch {
    throw new ApiError(503, 'ACCOUNT_PROFILE_CONFIGURATION_INVALID', 'The native Account profile transport is misconfigured.');
  }
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    typeof value.host !== 'string' || value.host.length === 0 ||
    typeof value.holdingSlug !== 'string' || !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug) ||
    !Number.isInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 || (value.timeoutMs as number) > 30_000
  )
    throw new ApiError(503, 'ACCOUNT_PROFILE_CONFIGURATION_INVALID', 'The native Account profile transport is misconfigured.');
  return { base: base.origin, host: value.host, holdingSlug: value.holdingSlug, timeoutMs: value.timeoutMs as number };
}

/** Validates the native reply field-by-field against the retired owner payload. */
export function parseAccountProfile(value: unknown): AccountProfile {
  if (!value || typeof value !== 'object' || Array.isArray(value))
    throw new ApiError(503, 'ACCOUNT_PROFILE_UNAVAILABLE', 'The native Account profile reply was invalid.');
  const row = value as Record<string, unknown>;
  if (typeof row.name !== 'string' || typeof row.email !== 'string' ||
      !row.preferences || typeof row.preferences !== 'object' || Array.isArray(row.preferences))
    throw new ApiError(503, 'ACCOUNT_PROFILE_UNAVAILABLE', 'The native Account profile reply was invalid.');
  const preferences = row.preferences as Record<string, unknown>;
  if ((preferences.language !== 'en' && preferences.language !== 'th') ||
      (preferences.appearance !== 'system' && preferences.appearance !== 'light' && preferences.appearance !== 'dark') ||
      !Number.isSafeInteger(preferences.version) || (preferences.version as number) < 0)
    throw new ApiError(503, 'ACCOUNT_PROFILE_UNAVAILABLE', 'The native Account profile reply was invalid.');
  return {
    name: row.name,
    email: row.email,
    preferences: {
      language: preferences.language,
      appearance: preferences.appearance,
      version: preferences.version as number,
    },
  };
}

/** Read the profile from the native owner. No TS SQL and no fallback. */
export async function nativeAccountProfile(userId: string): Promise<AccountProfile> {
  const config = nativeConfig();
  const headers = new Headers({
    host: config.host,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-user': userId,
  });
  let response: Response;
  try {
    response = await nativeRequest(
      new URL(`/api/v1/holdings/${encodeURIComponent(config.holdingSlug)}/account/profile`, config.base),
      { method: 'GET', headers: Object.fromEntries(headers.entries()) },
      config.timeoutMs,
    );
  } catch {
    throw new ApiError(503, 'ACCOUNT_PROFILE_UNAVAILABLE', 'The native Account profile service is unavailable.');
  }
  if (!response.ok) {
    const value = await response.json().catch(() => null) as { error?: { code?: string; message?: string } } | null;
    throw new ApiError(
      response.status,
      value?.error?.code ?? 'ACCOUNT_PROFILE_FAILED',
      value?.error?.message ?? 'The Account profile could not be read.',
    );
  }
  const value = await response.json().catch(() => null) as { data?: unknown } | null;
  return parseAccountProfile(value?.data);
}
