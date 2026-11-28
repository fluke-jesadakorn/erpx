import 'server-only';
import { randomUUID } from 'node:crypto';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import { nativeRequest } from '@erpx/platform-runtime-server/public/native-http';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import type { SqlValueRow } from '@erpx/platform-runtime-server/public/runtime/sql';

/**
 * Native POS owner-write transport for the owner-wide retirement slice, also
 * serving the retired owner read literals.
 *
 * The SQL for every retired write and for the retired read-only approval-queue
 * kinds lives in the Rust owner registry
 * (`erpx-server-pos::adapters::owner_writes`). This bridge asks the one axum
 * owner router for the authoritative statement by typed `kind`, then binds
 * it on the caller's existing SqlDatabase. The caller executes it inside the
 * same transaction that carries the POS receipt/audit/outbox effects: full
 * parity with the retired `run(db, sql, ...)` / `all(db, sql, ...)` helpers,
 * with no TypeScript SQL, no mirror and no fallback writer/reader. An
 * unconfigured or failing native service fails closed; it never writes or
 * reads locally. Read kinds return rows only (enforced by a Rust unit test);
 * permission checks stay in the TypeScript application and transport never
 * substitutes for the check.
 */
type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number }>;
type Statement = Readonly<{ sql: string; values: unknown[] }>;
type StatementResponse = { data?: Statement; error?: { code?: string; message?: string } };
type ErrorResponse = { error?: { code?: string; message?: string } };

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_POS_OWNER_CONFIG;
  if (!raw)
    throw new ApiError(503, 'NATIVE_POS_OWNER_UNAVAILABLE', 'The native POS owner service is unavailable.');
  try {
    const value = JSON.parse(raw) as Partial<Settings>;
    const base = new URL(String(value.privateBaseUrl ?? ''));
    if (
      base.protocol !== 'http:' ||
      !['127.0.0.1', '[::1]', 'localhost'].includes(base.hostname) ||
      !base.port ||
      typeof value.timeoutMs !== 'number' ||
      value.timeoutMs < 250 ||
      value.timeoutMs > 30_000
    )
      throw new Error('invalid');
    return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs };
  } catch {
    throw new ApiError(503, 'NATIVE_POS_OWNER_CONFIGURATION_INVALID', 'Native POS owner transport is misconfigured.');
  }
}

async function statement(db: SqlDatabase, company: string, kind: string, values: unknown[]): Promise<SqlStatement> {
  const config = configuration();
  // Company-free POS drains (companyId null) still need a transport scope. The
  // owner statement registry ignores the Company, so the explicit `system`
  // scope selects the same owner plan without inventing a real Company.
  const scope = company || 'system';
  let response: Response;
  try {
    response = await nativeRequest(
      new URL(`/api/v1/companies/${encodeURIComponent(scope)}/pos/owner-write`, config.privateBaseUrl),
      {
        method: 'POST',
        headers: {
          'content-type': 'application/json',
          'idempotency-key': randomUUID(),
          'x-erpx-context-company': scope,
        },
        body: JSON.stringify({ company: scope, kind, values }),
      },
      config.timeoutMs,
    );
  } catch {
    throw new ApiError(503, 'NATIVE_POS_OWNER_UNAVAILABLE', 'The native POS owner service is unavailable.');
  }
  let payload: StatementResponse | null = null;
  try { payload = (await response.json()) as StatementResponse; } catch { /* bounded failure below */ }
  if (!response.ok || !payload?.data) {
    throw new ApiError(response.status, payload?.error?.code ?? 'NATIVE_POS_OWNER_FAILED', payload?.error?.message ?? 'The POS owner write failed.');
  }
  if (typeof payload.data.sql !== 'string' || !Array.isArray(payload.data.values))
    throw new ApiError(503, 'NATIVE_POS_OWNER_RESPONSE_INVALID', 'The native POS owner write was invalid.');
  return db.prepare(payload.data.sql).bind(...payload.data.values);
}

/** Execute an owner write on the caller's unit (retired `run` parity). */
export async function nativePosRun(db: SqlDatabase, company: string, kind: string, values: unknown[]) {
  return (await statement(db, company, kind, values)).run();
}

/** Execute a `... RETURNING` owner write on the caller's unit (retired `one` parity). */
export async function nativePosOne<T = Record<string, unknown>>(db: SqlDatabase, company: string, kind: string, values: unknown[]): Promise<T | null> {
  return (await statement(db, company, kind, values)).first<T>();
}

/** Execute a read-only owner statement on the caller's unit (retired `all` parity). */
export async function nativePosAll(db: SqlDatabase, company: string, kind: string, values: unknown[]): Promise<SqlValueRow[]> {
  return (await (await statement(db, company, kind, values)).all<SqlValueRow>()).results;
}

/** Return a bound owner statement for callers that batch multiple effects. */
export async function nativePosStatement(db: SqlDatabase, company: string, kind: string, values: unknown[]) {
  return statement(db, company, kind, values);
}

/**
 * Synchronous native-write descriptor for repository effects that are built
 * synchronously and later resolved into a batch. Carries a typed owner kind
 * and positional values only — never SQL, a mirror or a TypeScript fallback.
 * The shared native-effects resolver recognizes `__nativeWrite` and executes
 * `run()` through this axum owner command in the batch's Company context.
 */
export type PosNativeWrite = {
  readonly __nativeWrite: true;
  readonly kind: string;
  readonly values: unknown[];
  context?: { companyId: string; key: string; now: number };
  withContext(context: { companyId: string; key: string; now: number }): PosNativeWrite;
  run(): Promise<{ rows: number; row?: unknown; rowsAffected?: number }>;
  first<T = unknown>(): Promise<T | undefined>;
  all<T = unknown>(): Promise<T[]>;
};

export function nativePosWrite(kind: string, values: unknown[]): PosNativeWrite {
  const descriptor: PosNativeWrite = {
    __nativeWrite: true,
    kind,
    values,
    withContext(context) { descriptor.context = context; return descriptor; },
    async run() {
      const config = configuration();
      const supplied = descriptor.context;
      // Migration batches may supply an empty Company; the owner unit is still
      // the caller's Holding write database, so a stable system Company label
      // is sufficient for the private route's Company-keyed backend lookup.
      const context = {
        companyId: supplied?.companyId || 'pos-system',
        key: supplied?.key || 'pos-owner-' + randomUUID(),
        now: supplied?.now || Date.now(),
      };
      let response: Response;
      try {
        response = await fetch(
          `${config.privateBaseUrl}/api/v1/companies/${encodeURIComponent(context.companyId)}/pos/owner-write`,
          {
            method: 'POST',
            headers: {
              'content-type': 'application/json',
              'idempotency-key': context.key,
              'x-erpx-context-company': context.companyId,
            },
            body: JSON.stringify({ company: context.companyId, kind, values, now: context.now }),
            cache: 'no-store',
            signal: AbortSignal.timeout(config.timeoutMs),
          },
        );
      } catch {
        throw new ApiError(503, 'NATIVE_POS_OWNER_UNAVAILABLE', 'The native POS owner service is unavailable.');
      }
      let payload: ErrorResponse | null = null;
      try { payload = (await response.json()) as ErrorResponse; } catch { /* bounded failure below */ }
      if (!response.ok)
        throw new ApiError(response.status, payload?.error?.code ?? 'NATIVE_POS_OWNER_FAILED', payload?.error?.message ?? 'The POS owner write failed.');
      return { rows: 1, rowsAffected: 1 };
    },
    async first<T = unknown>() {
      return (await descriptor.run()).row as T | undefined;
    },
    async all<T = unknown>() {
      const result = await descriptor.run();
      return Array.from({ length: result.rowsAffected ?? result.rows }, () => ({} as T));
    },
  };
  return descriptor;
}
