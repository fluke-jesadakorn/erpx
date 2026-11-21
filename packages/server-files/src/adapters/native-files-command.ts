import 'server-only';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlResult, SqlStatement } from '@erpx/server-runtime/public/runtime/types';

/**
 * Strict owner transport for the ONE native Files axum command router.
 *
 * The axum owner EXECUTES each write in its own serializable Holding unit and
 * returns the full command payload. This bridge never emits SQL, never accepts
 * SQL from a caller, and fails closed with no TypeScript fallback.
 */
type Settings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: ReadonlyArray<Readonly<{ origin: string; holdingSlug: string; holdingId: string; companyId: string }>>;
}>;

export type NativeResult = Readonly<{ rows: number; row?: unknown; rowsAffected?: number }>;
export type NativeContext = Readonly<{
  companyId?: string;
  key?: string;
  now?: number;
  holdingId?: string;
  holdingSlug?: string;
}>;

const RESERVED = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_FILES_CONFIG;
  if (!raw) throw new ApiError(503, 'NATIVE_FILES_UNAVAILABLE', 'The native Files service is unavailable.');
  let value: unknown;
  try { value = JSON.parse(raw); } catch { throw new ApiError(503, 'NATIVE_FILES_CONFIGURATION_INVALID', 'Files transport is misconfigured.'); }
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new ApiError(503, 'NATIVE_FILES_CONFIGURATION_INVALID', 'Files transport is misconfigured.');
  const candidate = value as Record<string, unknown>;
  let base: URL;
  try { base = new URL(String(candidate.privateBaseUrl)); } catch { throw new ApiError(503, 'NATIVE_FILES_CONFIGURATION_INVALID', 'Files transport is misconfigured.'); }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
    RESERVED.has(port) || (port >= 3200 && port <= 3299) || base.username || base.password ||
    String(candidate.privateBaseUrl) !== base.origin ||
    !Number.isInteger(candidate.timeoutMs) || Number(candidate.timeoutMs) < 250 || Number(candidate.timeoutMs) > 30_000 ||
    !Array.isArray(candidate.holdings) || candidate.holdings.length === 0
  ) throw new ApiError(503, 'NATIVE_FILES_CONFIGURATION_INVALID', 'Files transport is misconfigured.');
  const holdings = candidate.holdings.map((item: unknown) => {
    if (!item || typeof item !== 'object' || Array.isArray(item)) throw new ApiError(503, 'NATIVE_FILES_CONFIGURATION_INVALID', 'Files transport is misconfigured.');
    const h = item as Record<string, unknown>;
    if (typeof h.origin !== 'string' || !/^https:\/\/[^/?#@:]+$/.test(h.origin) ||
        typeof h.holdingSlug !== 'string' || !/^[a-z0-9_-]{1,128}$/.test(h.holdingSlug) ||
        typeof h.holdingId !== 'string' || !h.holdingId ||
        typeof h.companyId !== 'string' || !h.companyId)
      throw new ApiError(503, 'NATIVE_FILES_CONFIGURATION_INVALID', 'Files transport is misconfigured.');
    return { origin: h.origin, holdingSlug: h.holdingSlug, holdingId: h.holdingId, companyId: h.companyId };
  });
  return { privateBaseUrl: base.origin, timeoutMs: Number(candidate.timeoutMs), holdings };
}

function effective(kind: string, values: unknown[], context?: NativeContext) {
  const config = configuration();
  const holding = (context?.holdingId && context?.holdingSlug)
    ? config.holdings.find(h => h.holdingId === context.holdingId && h.holdingSlug === context.holdingSlug)
    : config.holdings.find(h => h.companyId === context?.companyId);
  const selected = holding ?? config.holdings[0]!;
  return {
    config,
    holding: selected,
    companyId: context?.companyId || selected.companyId,
    key: context?.key || `files-${createHash('sha256').update(JSON.stringify([kind, values])).digest('hex').slice(0, 32)}`,
    now: context?.now || Date.now(),
  };
}

async function dispatch(kind: string, values: unknown[], context?: NativeContext): Promise<NativeResult> {
  const { config, holding, companyId, key, now } = effective(kind, values, context);
  if (!/^[a-zA-Z0-9:_-]{1,180}$/.test(key)) throw new ApiError(503, 'NATIVE_FILES_COMMAND_INVALID', 'The native Files command key is invalid.');
  const headers: Record<string, string> = {
    'host': new URL(holding.origin).host,
    'origin': holding.origin,
    'idempotency-key': key,
    'x-erpx-context-company': companyId,
  };
  let response: Response;
  try {
    response = await fetch(`${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/files/commands`, {
      method: 'POST',
      headers,
      body: JSON.stringify({ kind, values, now, companyId }),
      cache: 'no-store',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch { throw new ApiError(503, 'NATIVE_FILES_UNAVAILABLE', 'The native Files service is unavailable.'); }
  const payload = (await response.json().catch(() => null)) as
    | { data?: NativeResult; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok || !payload?.data || typeof payload.data.rows !== 'number')
    throw new ApiError(response.status, payload?.error?.code ?? 'NATIVE_FILES_FAILED', payload?.error?.message ?? 'The native Files request failed.');
  const data = payload.data;
  if (data.rows < 0 || (data.row !== undefined && (typeof data.row !== 'object' || data.row === null)))
    throw new ApiError(503, 'NATIVE_FILES_RESPONSE_INVALID', 'The native Files response was invalid.');
  return data;
}

function result<T>(rows: number, row?: unknown): SqlResult<T> {
  return { results: row === undefined ? [] : [row as T], success: true, meta: { changes: rows, duration: 0 } };
}

/**
 * Synchronous, SqlStatement-shaped native-write descriptor. It satisfies the
 * retained synchronous SocialHomeRepository<SqlStatement> interface and caller
 * batches, but carries NO SQL — only a typed owner kind and positional values.
 * The shared native-effects resolver recognizes `__nativeWrite` and executes
 * run() through the one axum Files command in the batch's Company context.
 */
export type NativeWrite = SqlStatement & {
  readonly __nativeWrite: true;
  readonly kind: string;
  readonly values: unknown[];
  context?: NativeContext;
  withContext(context: NativeContext): NativeWrite;
};

export function nativeWrite(kind: string, values: unknown[]): NativeWrite {
  const descriptor: NativeWrite = {
    __nativeWrite: true,
    kind,
    values,
    context: undefined,
    withContext(context) { descriptor.context = { ...descriptor.context, ...context }; return descriptor; },
    bind(...extra) {
      (descriptor as unknown as { values: unknown[] }).values = [...descriptor.values, ...extra];
      return descriptor;
    },
    async run<T>() {
      const outcome = await dispatch(kind, descriptor.values, descriptor.context);
      return result<T>(outcome.rowsAffected ?? outcome.rows, outcome.row);
    },
    async first<T>() {
      const outcome = await dispatch(kind, descriptor.values, descriptor.context);
      return (outcome.row as T) ?? null;
    },
    async all<T>() {
      const outcome = await dispatch(kind, descriptor.values, descriptor.context);
      return result<T>(outcome.rowsAffected ?? outcome.rows, outcome.row);
    },
    async raw<T>() { return [] as T[]; },
  };
  return descriptor;
}

export function isNativeWrite(value: unknown): value is NativeWrite {
  return !!value && typeof value === 'object' && (value as NativeWrite).__nativeWrite === true;
}

/** Execute native descriptors with the current command context; keep real statements. */
export async function resolveNativeEffects<T>(
  effects: T[],
  context: NativeContext,
): Promise<{ statements: Exclude<T, NativeWrite>[]; native: SqlResult[] }> {
  const statements: Exclude<T, NativeWrite>[] = [];
  const native: SqlResult[] = [];
  for (const effect of effects) {
    if (isNativeWrite(effect)) {
      const outcome = await effect.withContext(context).run();
      native.push(outcome as SqlResult);
    } else statements.push(effect as Exclude<T, NativeWrite>);
  }
  return { statements, native };
}
