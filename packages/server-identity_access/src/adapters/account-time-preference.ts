import 'server-only';
import { createHash } from 'node:crypto';
import { runtimeEnv } from '../../../../packages/server-runtime/src/public/runtime';
import { ApiError } from '@erpx/server-runtime/public/http';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type { TimePreference } from '@erpx/server-identity_access/domain/account-time-preference';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { currentSession } from '@erpx/server-identity_access/adapters/authorization';
import { defaultTimePreference } from '@erpx/server-identity_access/domain/account-time-preference';

/**
 * The write side of account time preferences is a native axum command (see the
 * `account_time_preference_http` router in the owner crate). This adapter is a
 * thin private transport: it forwards the exact registered Holding Host/Origin,
 * one deterministic Idempotency-Key and the caller identity to that endpoint.
 * No SQL, no fallback and no write path remain in TypeScript.
 */
type NativeConfig = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;

function nativeConfig(): NativeConfig {
  const raw = process.env.ERPX_NATIVE_ACCOUNT_TIME_PREFERENCE_CONFIG;
  if (raw === undefined)
    throw new ApiError(503, 'TIME_PREFERENCE_UNAVAILABLE', 'The native time preference service is not configured.');
  let value: {
    privateBaseUrl?: unknown;
    host?: unknown;
    holdingSlug?: unknown;
    origin?: unknown;
    timeoutMs?: unknown;
  };
  try {
    value = JSON.parse(raw) as typeof value;
  } catch {
    throw new ApiError(503, 'TIME_PREFERENCE_CONFIGURATION_INVALID', 'The native time preference transport is misconfigured.');
  }
  let base: URL, origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    throw new ApiError(503, 'TIME_PREFERENCE_CONFIGURATION_INVALID', 'The native time preference transport is misconfigured.');
  }
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    origin.protocol !== 'https:' ||
    typeof value.host !== 'string' || value.host !== origin.host ||
    typeof value.holdingSlug !== 'string' || !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug) ||
    !Number.isInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 || (value.timeoutMs as number) > 30_000
  )
    throw new ApiError(503, 'TIME_PREFERENCE_CONFIGURATION_INVALID', 'The native time preference transport is misconfigured.');
  return {
    base: base.origin,
    host: value.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs as number,
  };
}

function idempotencyKey(userId: string, input: Record<string, unknown>): string {
  return createHash('sha256')
    .update('account.time_preference.change')
    .update(':')
    .update(userId)
    .update(':')
    .update(JSON.stringify(input))
    .digest('hex');
}

async function exposed<T>(action: () => Promise<T>) {
  try { return await action(); }
  catch (error) { if (error instanceof BusinessRuleError) throw new ApiError(error.status, error.code, error.message); throw error; }
}

/** The retained Control read side. Reads only; it never creates or changes a row. */
export const accountTimePreference = (userId: string) => exposed(async () => {
  let db: SqlDatabase;
  const ready = async () => { db = (await runtimeEnv()).CONTROL_DB; };
  await ready();
  return await db!.prepare('SELECT mode,time_zone AS "timeZone",hour_cycle AS "hourCycle",version FROM account_time_preferences WHERE user_id=?')
    .bind(userId).first<TimePreference>() ?? defaultTimePreference;
});

/** Native command write side. The preference is committed by the axum router. */
export const changeAccountTimePreference = (userId: string, input: unknown) => exposed(async () => {
  const config = nativeConfig();
  const payload = input && typeof input === 'object' && !Array.isArray(input)
    ? input as Record<string, unknown>
    : {};
  const body = JSON.stringify({ userId, ...payload });
  const headers = new Headers({
    host: config.host,
    origin: config.origin,
    accept: 'application/json',
    'content-type': 'application/json',
    'idempotency-key': idempotencyKey(userId, payload),
    'x-erpx-context-user': userId,
  });
  let response: Response;
  try {
    response = await fetch(
      `${config.base}/api/v1/holdings/${encodeURIComponent(config.holdingSlug)}/account/time-preference`,
      { method: 'POST', headers, body, cache: 'no-store', signal: AbortSignal.timeout(config.timeoutMs) },
    );
  } catch {
    throw new ApiError(503, 'TIME_PREFERENCE_UNAVAILABLE', 'The native time preference service is unavailable.');
  }
  const value = await response.json().catch(() => null) as
    | { preference?: TimePreference; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok)
    throw new ApiError(
      response.status,
      value?.error?.code ?? 'TIME_PREFERENCE_FAILED',
      value?.error?.message ?? 'The time preference could not be saved.',
    );
  if (!value?.preference)
    throw new ApiError(503, 'TIME_PREFERENCE_UNAVAILABLE', 'The native time preference reply was invalid.');
  return value.preference;
});

/** Render-only preference seed. Never serialize a session or authority snapshot. */
export async function accountTimePreferenceSnapshot() {
  let key: string;
  let userId: string;
  try {
    const { session } = await currentSession();
    userId = session.user.id;
    key = createHash('sha256').update('time-preference:').update(userId).digest('hex');
  } catch (error) {
    if (error instanceof Error && error.message === 'UNAUTHENTICATED')
      return { key: 'anonymous', preference: defaultTimePreference };
    return null;
  }
  try { return { key, preference: await accountTimePreference(userId) }; }
  catch {
    // Keep the account identity during an optional preference outage. Never
    // discard unrelated drafts by remounting the application to retry a read.
    return { key, preference: undefined };
  }
}
