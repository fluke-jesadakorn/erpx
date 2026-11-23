import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
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

export type NativeResult = Readonly<{ rows: number; row?: unknown; rowsAffected?: number; batch?: unknown; all?: unknown[] }>;
export type NativeContext = Readonly<{
  companyId?: string;
  key?: string;
  now?: number;
  holdingId?: string;
  holdingSlug?: string;
  /** Caller unit: when present, commands are planned and executed locally. */
  db?: import('@erpx/server-runtime/public/runtime/types').SqlDatabase;
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

/** POST with an explicit Host header; fetch forbids overriding Host. */
function post(
  url: URL,
  headers: Record<string, string>,
  body: string,
  timeoutMs: number,
): Promise<{ status: number; value: unknown }> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const outgoing = httpRequest(
      url,
      {
        method: 'POST',
        agent,
        headers: { ...headers, 'content-length': String(Buffer.byteLength(body)) },
        signal: AbortSignal.timeout(timeoutMs),
        maxHeaderSize: 8192,
      },
      incoming => {
        const chunks: Buffer[] = [];
        incoming.on('data', chunk => chunks.push(Buffer.from(chunk)));
        incoming.on('end', () => {
          agent.destroy();
          const text = Buffer.concat(chunks).toString('utf8');
          let value: unknown = null;
          try { value = text ? JSON.parse(text) : null; } catch { value = null; }
          resolve({ status: incoming.statusCode ?? 502, value });
        });
        incoming.on('error', error => { agent.destroy(); reject(error); });
      },
    );
    outgoing.on('error', error => { agent.destroy(); reject(error); });
    outgoing.end(body);
  });
}

/** Plan owner SQL in Rust and execute it on the caller's open unit. */
async function dispatchPlan(
  config: { privateBaseUrl: string; timeoutMs: number },
  holding: { origin: string; holdingSlug: string },
  companyId: string,
  key: string,
  now: number,
  kind: string,
  values: unknown[],
  db: import('@erpx/server-runtime/public/runtime/types').SqlDatabase,
): Promise<NativeResult> {
  let response: { status: number; value: unknown };
  try {
    response = await post(
      new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/files/command-plan`, config.privateBaseUrl),
      {
        host: new URL(holding.origin).host,
        origin: holding.origin,
        'idempotency-key': key,
        'x-erpx-context-company': companyId,
      },
      JSON.stringify({ kind, values, now, companyId }),
      config.timeoutMs,
    );
  } catch { throw new ApiError(503, 'NATIVE_FILES_UNAVAILABLE', 'The native Files service is unavailable.'); }
  const payload = response.value as
    | { data?: { statements?: { sql?: unknown; values?: unknown }[] }; error?: { code?: string; message?: string } }
    | null;
  if (response.status !== 200 || !Array.isArray(payload?.data?.statements))
    throw new ApiError(response.status, payload?.error?.code ?? 'NATIVE_FILES_FAILED', payload?.error?.message ?? 'The native Files request failed.');
  const statements = payload.data.statements.map(statement => {
    if (typeof statement.sql !== 'string' || !Array.isArray(statement.values))
      throw new ApiError(503, 'NATIVE_FILES_RESPONSE_INVALID', 'The native Files plan was invalid.');
    return db.prepare(statement.sql).bind(...statement.values);
  });
  const results = await db.batch(statements);
  return {
    rows: Number(results[0]?.meta?.changes ?? 0),
    row: results[0]?.results?.[0],
    all: results.flatMap(result => (result.results ?? []) as unknown[]),
  };
}

/**
 * Execute an owner-owned read plan on the caller's already-open unit.
 *
 * No SQL is emitted here: the native Files owner plans the exact statement for
 * the read kind and this transport only executes it with routing-scoped values.
 * Read kinds carry no receipt/audit/outbox write ledger. Missing native
 * configuration or any configured failure fails closed with no TS fallback.
 */
export async function nativeFilesRead<T>(
  kind: string,
  values: unknown[],
  db: import('@erpx/server-runtime/public/runtime/types').SqlDatabase,
  companyId?: string,
): Promise<T | null> {
  const { config, holding, companyId: resolved } = effective(kind, values, { companyId, db });
  const key = `files-read-${createHash('sha256').update(JSON.stringify([kind, values, resolved])).digest('hex').slice(0, 32)}`;
  const outcome = await dispatchPlan(config, holding, resolved, key, Date.now(), kind, values, db);
  return (outcome.row as T) ?? null;
}

/**
 * Execute an owner-owned array read plan on the caller's already-open unit and
 * return every row in the caller's `SqlResult` shape. Used by reads whose
 * retained consumer reads `.results` (for example a variable-cardinality `IN`
 * list carried as a single JSON-array bind). SQL stays in the Rust owner; a
 * missing or failing native transport fails closed with no TypeScript fallback.
 */
export async function nativeFilesReadAll<T>(
  kind: string,
  values: unknown[],
  db: import('@erpx/server-runtime/public/runtime/types').SqlDatabase,
  companyId?: string,
): Promise<SqlResult<T>> {
  const { config, holding, companyId: resolved } = effective(kind, values, { companyId, db });
  const key = `files-read-${createHash('sha256').update(JSON.stringify([kind, values, resolved])).digest('hex').slice(0, 32)}`;
  const outcome = await dispatchPlan(config, holding, resolved, key, Date.now(), kind, values, db);
  return { results: (outcome.all ?? []) as T[], success: true, meta: { changes: 0, duration: 0 } };
}

async function dispatch(kind: string, values: unknown[], context?: NativeContext): Promise<NativeResult> {
  const { config, holding, companyId, key, now } = effective(kind, values, context);
  if (!/^[a-zA-Z0-9:_-]{1,180}$/.test(key)) throw new ApiError(503, 'NATIVE_FILES_COMMAND_INVALID', 'The native Files command key is invalid.');
  if (context?.db) return dispatchPlan(config, holding, companyId, key, now, kind, values, context.db);
  const headers: Record<string, string> = {
    'host': new URL(holding.origin).host,
    'origin': holding.origin,
    'idempotency-key': key,
    'x-erpx-context-company': companyId,
  };
  const body = JSON.stringify({ kind, values, now, companyId });
  let response: { status: number; value: unknown };
  try {
    response = await post(
      new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/files/commands`, config.privateBaseUrl),
      headers,
      body,
      config.timeoutMs,
    );
  } catch { throw new ApiError(503, 'NATIVE_FILES_UNAVAILABLE', 'The native Files service is unavailable.'); }
  const payload = response.value as
    | { data?: NativeResult; error?: { code?: string; message?: string } }
    | null;
  if (response.status !== 200 || !payload?.data || typeof payload.data.rows !== 'number')
    throw new ApiError(response.status, payload?.error?.code ?? 'NATIVE_FILES_FAILED', payload?.error?.message ?? 'The native Files request failed.');
  const data = payload.data;
  if (data.rows < 0 || (data.row !== undefined && (typeof data.row !== 'object' || data.row === null))
    || (data.batch !== undefined && !Array.isArray(data.batch)))
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
      if (Array.isArray(outcome.batch))
        return { results: outcome.batch as T[], success: true, meta: { changes: outcome.rows, duration: 0 } };
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
