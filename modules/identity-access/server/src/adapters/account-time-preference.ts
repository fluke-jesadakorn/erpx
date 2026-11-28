import 'server-only';
import { createHash } from 'node:crypto';
import { headers } from 'next/headers';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import { nativeRequest } from '@erpx/platform-runtime-server/public/native-http';
import type { TimePreference } from '@erpx/identity-access-server/domain/account-time-preference';
import { currentSession } from '@erpx/identity-access-server/adapters/authorization';
import { defaultTimePreference } from '@erpx/identity-access-server/domain/account-time-preference';
import { nativeAccountConfiguration } from './native-session';

/** Test seam for the read oracle. Production always uses the native reader. */
let testRead: ((userId: string) => Promise<TimePreference | null>) | undefined;
export function setTimePreferenceReadForTest(reader?: (userId: string) => Promise<TimePreference | null>) { testRead = reader; }

function parsePreference(value: unknown): TimePreference {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new ApiError(503, 'TIME_PREFERENCE_UNAVAILABLE', 'The native time preference reply was invalid.');
  const row = value as Record<string, unknown>;
  if (
    (row.mode !== 'system' && row.mode !== 'manual') ||
    (row.timeZone !== null && typeof row.timeZone !== 'string') ||
    (row.hourCycle !== 'h12' && row.hourCycle !== 'h23') ||
    !Number.isSafeInteger(row.version) || (row.version as number) < 0
  )
    throw new ApiError(503, 'TIME_PREFERENCE_UNAVAILABLE', 'The native time preference reply was invalid.');
  return { mode: row.mode, timeZone: row.timeZone as string | null, hourCycle: row.hourCycle, version: row.version as number };
}

/** Native read companion of the command router. No SQL and no fallback: a
 * missing or malformed native runtime fails closed with an explicit 503. */
async function readNativeTimePreference(incoming: Pick<Headers, 'get'>): Promise<TimePreference> {
  const raw = process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG;
  const host = incoming.get('host');
  const cookie = incoming.get('cookie');
  let response: Response;
  try {
    if (!raw || !host || !cookie) throw new Error('TIME_PREFERENCE_SESSION_REQUIRED');
    const config = nativeAccountConfiguration(raw);
    const base = new URL(config.privateBaseUrl);
    if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || base.href !== base.origin + '/'
      || !Number.isInteger(config.timeoutMs) || config.timeoutMs < 250 || config.timeoutMs > 30_000) {
      throw new Error('TIME_PREFERENCE_CONFIGURATION_INVALID');
    }
    response = await nativeRequest(new URL('/api/v1/account/time-preference', base), {
      headers: { host, cookie, accept: 'application/json' },
    }, config.timeoutMs);
  } catch {
    throw new ApiError(503, 'TIME_PREFERENCE_UNAVAILABLE', 'The native time preference service is unavailable.');
  }
  if (!response.ok) {
    const value = await response.json().catch(() => null) as { error?: { code?: string; message?: string } } | null;
    throw new ApiError(
      response.status,
      value?.error?.code ?? 'TIME_PREFERENCE_FAILED',
      value?.error?.message ?? 'The time preference could not be read.',
    );
  }
  const value = await response.json().catch(() => null) as { data?: unknown } | null;
  return parsePreference(value?.data);
}

/** Native-only read side. The axum read router owns the SQL and projection. */
export async function accountTimePreference(userId: string, incoming?: Pick<Headers, 'get'>) {
  return testRead ? (await testRead(userId)) ?? defaultTimePreference : await readNativeTimePreference(incoming ?? await headers());
}

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
