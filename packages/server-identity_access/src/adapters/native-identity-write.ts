import 'server-only';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { currentHoldingId } from '@erpx/server-runtime/public/runtime/holding-scope';
import { nativeRequest } from '@erpx/server-runtime/public/native-http';

/**
 * Single native transport for identity_access owner writes.
 *
 * One axum command router executes the owner's allow-listed write kind against
 * a caller-owned serializable transaction (with receipt/audit/outbox). This
 * module sends only the kind and a full-parity positional value array; it never
 * sends or holds SQL and has no fallback. Missing or invalid configuration
 * fails closed.
 */
type Holding = Readonly<{
  host: string;
  holdingSlug: string;
  origin: string;
  companyId: string;
  holdingId?: string;
}>;
type Config = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;

function configuration(): Config {
  const raw = process.env.ERPX_NATIVE_IDENTITY_WRITE_CONFIG;
  if (!raw)
    throw new ApiError(
      503,
      'IDENTITY_OWNER_WRITE_UNAVAILABLE',
      'The native identity owner-write service is not configured.',
    );
  let value: Record<string, unknown>;
  try {
    value = JSON.parse(raw) as Record<string, unknown>;
  } catch {
    throw new ApiError(
      503,
      'IDENTITY_OWNER_WRITE_CONFIG_INVALID',
      'The native identity owner-write transport is misconfigured.',
    );
  }
  let base: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
  } catch {
    throw new ApiError(
      503,
      'IDENTITY_OWNER_WRITE_CONFIG_INVALID',
      'The native identity owner-write transport is misconfigured.',
    );
  }
  const timeoutMs = value.timeoutMs;
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !Number.isInteger(timeoutMs) ||
    (timeoutMs as number) < 250 ||
    (timeoutMs as number) > 30_000
  )
    throw new ApiError(
      503,
      'IDENTITY_OWNER_WRITE_CONFIG_INVALID',
      'The native identity owner-write transport is misconfigured.',
    );
  const entries = Array.isArray(value.holdings)
    ? value.holdings
    : value.host && value.holdingSlug && value.origin && value.companyId
      ? [value]
      : null;
  if (!entries || entries.length < 1 || entries.length > 64)
    throw new ApiError(
      503,
      'IDENTITY_OWNER_WRITE_CONFIG_INVALID',
      'The native identity owner-write transport is misconfigured.',
    );
  const holdings: Holding[] = [];
  for (const entry of entries) {
    if (!entry || typeof entry !== 'object') throw new ApiError(503, 'IDENTITY_OWNER_WRITE_CONFIG_INVALID', 'The native identity owner-write transport is misconfigured.');
    const item = entry as Record<string, unknown>;
    let origin: URL;
    try {
      origin = new URL(String(item.origin));
    } catch {
      throw new ApiError(503, 'IDENTITY_OWNER_WRITE_CONFIG_INVALID', 'The native identity owner-write transport is misconfigured.');
    }
    if (
      origin.protocol !== 'https:' ||
      typeof item.host !== 'string' ||
      item.host !== origin.host ||
      typeof item.holdingSlug !== 'string' ||
      !/^[a-z0-9_-]{1,128}$/.test(item.holdingSlug) ||
      typeof item.companyId !== 'string' ||
      !item.companyId ||
      item.companyId.length > 200 ||
      (item.holdingId !== undefined && (typeof item.holdingId !== 'string' || !item.holdingId))
    )
      throw new ApiError(503, 'IDENTITY_OWNER_WRITE_CONFIG_INVALID', 'The native identity owner-write transport is misconfigured.');
    holdings.push({
      host: item.host,
      holdingSlug: item.holdingSlug,
      origin: origin.origin,
      companyId: item.companyId,
      ...(item.holdingId ? { holdingId: item.holdingId as string } : {}),
    });
  }
  return { base: base.origin, timeoutMs: timeoutMs as number, holdings };
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
  const scoped = currentHoldingId();
  const target =
    (scoped ? config.holdings.find((holding) => holding.holdingId === scoped) : undefined) ??
    (companyId ? config.holdings.find((holding) => holding.companyId === companyId) : undefined) ??
    // The identity owner write always executes on the Control unit; the
    // registered Holding only selects the private route, so a control-level
    // recovery/invitation write can use the canonical target explicitly.
    config.holdings[0];
  if (!target)
    throw new ApiError(503, 'IDENTITY_OWNER_WRITE_AUTHORITY_INVALID', 'The native identity owner-write authority is unavailable.');
  const headers = new Headers({
    host: target.host,
    origin: target.origin,
    accept: 'application/json',
    'content-type': 'application/json',
    'idempotency-key': key ?? contentKey(kind, values),
  });
  const company = companyId ?? target.companyId;
  const body = JSON.stringify({ kind, values, companyId: company, now: Date.now() });
  let response: Response;
  try {
    response = await nativeRequest(
      new URL(
        `${config.base}/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/identity/owner-write`,
      ),
      {
        method: 'POST',
        headers: Object.fromEntries(headers),
        body,
        signal: AbortSignal.timeout(config.timeoutMs),
      },
      config.timeoutMs,
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
  if (!response.ok) {
    throw new ApiError(
      response.status,
      payload?.error?.code ?? 'IDENTITY_OWNER_WRITE_FAILED',
      payload?.error?.message ?? 'The identity write could not be completed.',
    );
  }
  if (typeof payload?.data?.kind !== 'string' || typeof payload.data.rows !== 'number')
    throw new ApiError(
      503,
      'IDENTITY_OWNER_WRITE_RESPONSE_INVALID',
      'The native identity owner-write reply was invalid.',
    );
  return { kind: payload.data.kind, rows: payload.data.rows };
}
