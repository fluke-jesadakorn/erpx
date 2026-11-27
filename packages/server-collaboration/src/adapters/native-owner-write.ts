import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import { currentHoldingId } from '@erpx/server-runtime/public/runtime/holding-scope';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';

type Settings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: ReadonlyArray<Readonly<{ origin: string; holdingSlug: string; holdingId: string }>>;
}>;
type Statement = Readonly<{ sql: string; values: unknown[] }>;

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_COLLABORATION_CONFIG;
  if (!raw) throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.');
  try { return JSON.parse(raw) as Settings; }
  catch { throw new ApiError(503, 'NATIVE_COLLABORATION_CONFIGURATION_INVALID', 'Collaboration transport is misconfigured.'); }
}

function statement(db: SqlDatabase, response: Response, body: unknown): SqlStatement {
  const payload = body as { data?: Statement; error?: { code?: string; message?: string } } | null;
  if (!response.ok || !payload?.data)
    throw new ApiError(response.status, payload?.error?.code ?? 'NATIVE_COLLABORATION_FAILED', payload?.error?.message ?? 'The Collaboration request failed.');
  if (typeof payload.data.sql !== 'string' || !Array.isArray(payload.data.values))
    throw new ApiError(503, 'NATIVE_COLLABORATION_RESPONSE_INVALID', 'The native owner write was invalid.');
  return db.prepare(payload.data.sql).bind(...payload.data.values);
}

/** Ask the native Collaboration owner for an authoritative write statement in
 * the current browser WorkContext. The caller executes it in its own unit. */
export type OwnerWriteFragments = Readonly<{
  target?: 'room' | 'team' | 'social_draft';
  predicate?: string;
}>;

export async function nativeOwnerWrite(
  db: SqlDatabase,
  actor: WorkContext,
  kind: string,
  values: unknown[],
  key: string,
  now = Date.now(),
  fragments?: OwnerWriteFragments,
): Promise<SqlStatement> {
  const config = configuration();
  const target = config.holdings.find(
    holding => holding.holdingSlug === actor.holdingSlug && holding.holdingId === actor.holdingId,
  );
  if (!target) throw new ApiError(503, 'NATIVE_COLLABORATION_AUTHORITY_INVALID', 'The native Collaboration authority is unavailable.');
  const headers = new Headers({
    host: new URL(target.origin).host,
    origin: target.origin,
    'content-type': 'application/json',
    'idempotency-key': key,
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority?.revision ?? 0),
    'x-erpx-context-assignment-version': String(actor.authority?.assignmentVersion ?? 0),
  });
  if (actor.branchId) headers.set('x-erpx-workspace-branch', actor.branchId);
  if (actor.departmentId) headers.set('x-erpx-workspace-department', actor.departmentId);
  let response: Response;
  try {
    response = await fetch(
      `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/collaboration/owner-write`,
      { method: 'POST', headers, body: JSON.stringify({ kind, values, now, target: fragments?.target, predicate: fragments?.predicate }), cache: 'no-store', signal: AbortSignal.timeout(config.timeoutMs) },
    );
  } catch { throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.'); }
  return statement(db, response, await response.json().catch(() => null));
}

/** Company-keyed system variant for workers without a browser WorkContext. */
export async function nativeOwnerWriteSystem(
  db: SqlDatabase,
  companyId: string,
  kind: string,
  values: unknown[],
  key: string,
  now = Date.now(),
  fragments?: OwnerWriteFragments,
): Promise<SqlStatement> {
  const config = configuration();
  const headers = new Headers({
    'content-type': 'application/json',
    'idempotency-key': key,
    'x-erpx-context-company': companyId,
  });
  let response: Response;
  try {
    response = await fetch(
      `${config.privateBaseUrl}/api/v1/companies/${encodeURIComponent(companyId)}/collaboration/owner-write`,
      { method: 'POST', headers, body: JSON.stringify({ kind, values, now, target: fragments?.target, predicate: fragments?.predicate }), cache: 'no-store', signal: AbortSignal.timeout(config.timeoutMs) },
    );
  } catch { throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.'); }
  return statement(db, response, await response.json().catch(() => null));
}

/** Private system route for self-guarded facts writes with no Company context.
 * Only the owner allow-listed system kinds are accepted; a missing transport
 * fails closed with 503 and there is no TS SQL fallback. */
export async function nativeOwnerWriteSystemUnscoped(
  db: SqlDatabase,
  kind: string,
  values: unknown[],
  key: string,
  now = Date.now(),
  fragments?: OwnerWriteFragments,
): Promise<SqlStatement> {
  const config = configuration();
  const holding = currentHoldingId();
  const headers = new Headers({
    'content-type': 'application/json',
    'x-erpx-context-company': 'owner-system',
    'idempotency-key': key,
    ...(holding ? { 'x-erpx-context-holding': holding } : {}),
  });
  let response: Response;
  try {
    response = await fetch(
      `${config.privateBaseUrl}/api/v1/system/collaboration/owner-write`,
      { method: 'POST', headers, body: JSON.stringify({ kind, values, now, target: fragments?.target, predicate: fragments?.predicate }), cache: 'no-store', signal: AbortSignal.timeout(config.timeoutMs) },
    );
  } catch { throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.'); }
  return statement(db, response, await response.json().catch(() => null));
}

/**
 * Synchronous SqlStatement-shaped native-write descriptor for repositories that
 * must build effects synchronously and pass them through `db.batch([...])`.
 * It carries only an owner kind and positional values — never SQL, a JSON mirror
 * or a TypeScript fallback. The workflow command batch resolver recognizes
 * `__nativeWrite` and calls `run()`/`first()`/`all()`; those execute the owner
 * SQL returned by the axum service on the caller's own serializable unit so
 * receipt/audit/outbox stay in the same transaction.
 */
export type NativeOwnerWrite = SqlStatement & {
  readonly __nativeWrite: true;
  readonly kind: string;
  readonly values: readonly unknown[];
  context?: { key?: string; now?: number };
  withContext(context: { key?: string; now?: number }): NativeOwnerWrite;
};

export function nativeOwnerWriteDescriptor(
  db: SqlDatabase,
  kind: string,
  values: readonly unknown[],
  fragments?: OwnerWriteFragments,
): NativeOwnerWrite {
  if (!/^[a-z0-9_]{3,90}$/.test(kind) || values.length < 1 || values.length > 64)
    throw new ApiError(422, 'COLLABORATION_OWNER_WRITE_INVALID', 'Review the owner write request.');
  let args = [...values];
  const descriptor: NativeOwnerWrite = {
    __nativeWrite: true,
    kind,
    get values() { return args; },
    context: undefined,
    withContext(context) {
      descriptor.context = { ...descriptor.context, ...context };
      return descriptor;
    },
    bind(...extra: unknown[]) {
      args = [...args, ...extra];
      return descriptor;
    },
    async run<T>() {
      const sql = await resolve();
      return sql.run<T>();
    },
    async first<T>(column?: string) {
      const sql = await resolve();
      return column === undefined ? sql.first<T>() : sql.first<T>(column);
    },
    async all<T>() {
      const sql = await resolve();
      return sql.all<T>();
    },
    async raw<T>() {
      const sql = await resolve();
      return sql.raw<T>();
    },
  };
  const resolve = async (): Promise<SqlStatement> => {
    const config = configuration();
    const key = descriptor.context?.key ?? `sh-owner-${kind}-${Date.now()}`;
    const now = descriptor.context?.now ?? Date.now();
    const headers = new Headers({
      'content-type': 'application/json',
      'idempotency-key': key,
      'x-erpx-context-company': 'owner-system',
    });
    let response: Response;
    try {
      response = await fetch(
        `${config.privateBaseUrl}/api/v1/system/collaboration/owner-write`,
        {
          method: 'POST',
          headers,
          body: JSON.stringify({ kind, values: args, now, target: fragments?.target, predicate: fragments?.predicate }),
          cache: 'no-store',
          signal: AbortSignal.timeout(config.timeoutMs),
        },
      );
    } catch {
      throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.');
    }
    return statement(db, response, await response.json().catch(() => null));
  };
  return descriptor;
}

/** Whether an effect is a native Collaboration write descriptor. */
export function isNativeOwnerWrite(value: unknown): value is NativeOwnerWrite {
  return !!value && typeof value === 'object' && (value as NativeOwnerWrite).__nativeWrite === true;
}
