import 'server-only';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';

/**
 * Single native transport for identity_access owner writes.
 *
 * One axum command router executes the owner's allow-listed write kind against
 * a caller-owned serializable transaction (with receipt/audit/outbox). This
 * module sends only the kind and a full-parity positional value array; it never
 * sends or holds SQL and has no fallback. Missing or invalid configuration
 * fails closed.
 */
type Config = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
  companyId: string;
}>;

function configuration(): Config {
  const raw = process.env.ERPX_NATIVE_IDENTITY_WRITE_CONFIG;
  if (!raw)
    throw new ApiError(
      503,
      'IDENTITY_OWNER_WRITE_UNAVAILABLE',
      'The native identity owner-write service is not configured.',
    );
  let value: {
    privateBaseUrl?: unknown;
    host?: unknown;
    holdingSlug?: unknown;
    origin?: unknown;
    timeoutMs?: unknown;
    companyId?: unknown;
  };
  try {
    value = JSON.parse(raw) as typeof value;
  } catch {
    throw new ApiError(
      503,
      'IDENTITY_OWNER_WRITE_CONFIG_INVALID',
      'The native identity owner-write transport is misconfigured.',
    );
  }
  let base: URL, origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    throw new ApiError(
      503,
      'IDENTITY_OWNER_WRITE_CONFIG_INVALID',
      'The native identity owner-write transport is misconfigured.',
    );
  }
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    origin.protocol !== 'https:' ||
    typeof value.host !== 'string' ||
    value.host !== origin.host ||
    typeof value.holdingSlug !== 'string' ||
    !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug) ||
    typeof value.companyId !== 'string' ||
    !value.companyId ||
    value.companyId.length > 200 ||
    !Number.isInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000
  )
    throw new ApiError(
      503,
      'IDENTITY_OWNER_WRITE_CONFIG_INVALID',
      'The native identity owner-write transport is misconfigured.',
    );
  return {
    base: base.origin,
    host: value.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs as number,
    companyId: value.companyId,
  };
}

/** Deterministic key for callers without their own idempotency key. */
function contentKey(kind: string, values: unknown[]): string {
  return createHash('sha256')
    .update('identity.owner.write')
    .update(':')
    .update(kind)
    .update(':')
    .update(JSON.stringify(values))
    .digest('hex');
}

/**
 * Drop-in `prepare()` shim for legacy write statements. The SQL is gone: the
 * caller supplies only an owner kind and positional values. The returned
 * object is a thenable so it can replace a statement inside `Promise.all` and
 * still expose `run`/`first`/`all` for the retained application call sites.
 */
export function nativePrepare(kind: string) {
  const values: unknown[] = [];
  const run = async (): Promise<void> => {
    await nativeIdentityWrite(kind, values);
  };
  const first = async <T,>(): Promise<T | null> => {
    await nativeIdentityWrite(kind, values);
    return null;
  };
  const all = async <T,>(): Promise<{ results: T[]; success: boolean }> => {
    await nativeIdentityWrite(kind, values);
    return { results: [], success: true };
  };
  const bound = {
    run,
    first,
    all,
    then<TResult1 = void, TResult2 = never>(
      onfulfilled?: ((value: void) => TResult1 | PromiseLike<TResult1>) | null,
      onrejected?: ((reason: unknown) => TResult2 | PromiseLike<TResult2>) | null,
    ): PromiseLike<TResult1 | TResult2> {
      return run().then(onfulfilled, onrejected);
    },
  };
  return {
    bind(...args: unknown[]) {
      values.push(...args);
      return bound;
    },
    run,
    first,
    all,
  } as unknown as SqlStatement & PromiseLike<void>;
}

export async function nativeIdentityWrite(
  kind: string,
  values: unknown[],
  key?: string,
  companyId?: string,
): Promise<{ kind: string; rows: number }> {
  const config = configuration();
  const headers = new Headers({
    host: config.host,
    origin: config.origin,
    accept: 'application/json',
    'content-type': 'application/json',
    'idempotency-key': key ?? contentKey(kind, values),
  });
  const company = companyId ?? config.companyId;
  const body = JSON.stringify({ kind, values, companyId: company, now: Date.now() });
  let response: Response;
  try {
    response = await fetch(
      `${config.base}/api/v1/holdings/${encodeURIComponent(config.holdingSlug)}/identity/owner-write`,
      {
        method: 'POST',
        headers,
        body,
        cache: 'no-store',
        signal: AbortSignal.timeout(config.timeoutMs),
      },
    );
  } catch {
    throw new ApiError(
      503,
      'IDENTITY_OWNER_WRITE_UNAVAILABLE',
      'The native identity owner-write service is unavailable.',
    );
  }
  const payload = (await response.json().catch(() => null)) as
    | {
        data?: { kind?: string; rows?: number };
        error?: { code?: string; message?: string };
      }
    | null;
  if (!response.ok)
    throw new ApiError(
      response.status,
      payload?.error?.code ?? 'IDENTITY_OWNER_WRITE_FAILED',
      payload?.error?.message ?? 'The identity write could not be completed.',
    );
  if (typeof payload?.data?.kind !== 'string' || typeof payload.data.rows !== 'number')
    throw new ApiError(
      503,
      'IDENTITY_OWNER_WRITE_RESPONSE_INVALID',
      'The native identity owner-write reply was invalid.',
    );
  return { kind: payload.data.kind, rows: payload.data.rows };
}
