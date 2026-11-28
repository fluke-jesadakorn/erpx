import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { SqlDatabase, SqlResult, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';

/**
 * Synchronous typed native-write descriptor for Workflow repository effects.
 * It carries a kind and positional values only — never SQL, a mirror or a TS
 * fallback. It is structurally a SqlStatement so retained repository contracts
 * stay unchanged; execution happens in Rust through the owner axum command
 * router when run()/first()/all() is invoked or via resolveNativeEffects.
 */
type Settings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  systemHoldingId?: string;
}>;

export type NativeWorkflowContext = {
  holdingId?: string;
  companyId?: string;
  key?: string;
  now?: number;
  db?: SqlDatabase;
};

export type NativeWorkflowWrite = SqlStatement & {
  readonly __nativeWrite: true;
  readonly kind: string;
  readonly values: readonly unknown[];
  context?: NativeWorkflowContext;
  withContext(context: NativeWorkflowContext): NativeWorkflowWrite;
};

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_WORKFLOW_WAVE3_CONFIG;
  if (!raw)
    throw new ApiError(503, 'NATIVE_WORKFLOW_WAVE3_UNAVAILABLE', 'The native Workflow service is unavailable.');
  try { return JSON.parse(raw) as Settings; }
  catch { throw new ApiError(503, 'NATIVE_WORKFLOW_WAVE3_CONFIG_INVALID', 'Native Workflow transport is misconfigured.'); }
}

async function executeNative(
  descriptor: NativeWorkflowWrite,
): Promise<{ row?: unknown; changes: number }> {
  const config = configuration();
  const ctx = descriptor.context ?? {};
  // Assertions are pre-write guards: they must observe the caller's open
  // unit, so plan the owner SQL natively and execute it on the caller's
  // connection instead of a separate server transaction.
  if (descriptor.kind === 'wf_native_command_assertions_3' && ctx.companyId && ctx.db) {
    const now = ctx.now || Date.now();
    const key = ctx.key || `wf-owner-${descriptor.kind}-${now}`;
    let response: Response;
    try {
      response = await fetch(
        `${config.privateBaseUrl}/api/v1/companies/${encodeURIComponent(ctx.companyId)}/workflow/wave3-command`,
        {
          method: 'POST',
          headers: {
            'content-type': 'application/json',
            'idempotency-key': key,
            'x-erpx-context-company': ctx.companyId,
          },
          body: JSON.stringify({ kind: descriptor.kind, values: descriptor.values, now }),
          cache: 'no-store',
          signal: AbortSignal.timeout(config.timeoutMs),
        },
      );
    } catch {
      throw new ApiError(503, 'NATIVE_WORKFLOW_WAVE3_UNAVAILABLE', 'The native Workflow service is unavailable.');
    }
    const payload = (await response.json().catch(() => null)) as
      | { data?: { sql?: unknown; values?: unknown }; error?: { code?: string; message?: string } }
      | null;
    if (!response.ok || !payload?.data || typeof payload.data.sql !== 'string' || !Array.isArray(payload.data.values))
      throw new ApiError(response.status, payload?.error?.code ?? 'NATIVE_WORKFLOW_WAVE3_FAILED', payload?.error?.message ?? 'The native Workflow write failed.');
    const result = await ctx.db.prepare(payload.data.sql).bind(...payload.data.values).run();
    return { changes: Number(result.meta?.changes ?? 1) };
  }
  const holdingId = ctx.holdingId || config.systemHoldingId;
  if (!holdingId)
    throw new ApiError(503, 'NATIVE_WORKFLOW_WAVE3_AUTHORITY_INVALID', 'No native Workflow authority is configured.');
  const key = ctx.key || `wf-owner-${descriptor.kind}-${Date.now()}`;
  const now = ctx.now || Date.now();
  const headers = new Headers({
    'content-type': 'application/json',
    'idempotency-key': key,
    'x-erpx-context-holding': holdingId,
  });
  let response: Response;
  try {
    response = await fetch(
      `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingId)}/workflow/wave3-execute`,
      {
        method: 'POST', headers,
        body: JSON.stringify({ kind: descriptor.kind, values: descriptor.values, now }),
        cache: 'no-store', signal: AbortSignal.timeout(config.timeoutMs),
      },
    );
  } catch {
    throw new ApiError(503, 'NATIVE_WORKFLOW_WAVE3_UNAVAILABLE', 'The native Workflow service is unavailable.');
  }
  if (!response.ok) {
    const payload = (await response.json().catch(() => null)) as { error?: { code?: string; message?: string } } | null;
    throw new ApiError(response.status, payload?.error?.code ?? 'NATIVE_WORKFLOW_WAVE3_FAILED', payload?.error?.message ?? 'The native Workflow write failed.');
  }
  const body = (await response.json().catch(() => null)) as { data?: { row?: unknown; changes?: number } } | null;
  return { row: body?.data?.row, changes: body?.data?.changes ?? 1 };
}

function result<T>(changes: number, row?: unknown): SqlResult<T> {
  return { results: row === undefined ? [] : [row as T], success: true, meta: { changes, duration: 0 } };
}

export function nativeWorkflowWrite(kind: string, values: readonly unknown[]): NativeWorkflowWrite {
  if (!/^[a-z0-9_]{3,80}$/.test(kind) || values.length === 0 || values.length > 64)
    throw new ApiError(422, 'WORKFLOW_WAVE3_WRITE_INVALID', 'Review the Workflow owner write request.');
  const descriptor: NativeWorkflowWrite = {
    __nativeWrite: true,
    kind,
    values,
    context: undefined,
    withContext(context) { descriptor.context = context; return descriptor; },
    bind(...extra) {
      (descriptor as unknown as { values: unknown[] }).values = [...descriptor.values, ...extra];
      return descriptor;
    },
    async run<T>() {
      const outcome = await executeNative(descriptor);
      return result<T>(outcome.changes);
    },
    async first<T>() {
      const outcome = await executeNative(descriptor);
      return (outcome.row as T) ?? null;
    },
    async all<T>() {
      const outcome = await executeNative(descriptor);
      return result<T>(outcome.changes, outcome.row);
    },
    async raw<T>() { return [] as T[]; },
  };
  return descriptor;
}

export function isNativeWorkflowWrite(value: unknown): value is NativeWorkflowWrite {
  return !!value && typeof value === 'object' && (value as NativeWorkflowWrite).__nativeWrite === true;
}

/** Resolve descriptors at a caller-owned batch boundary; keep real statements. */
export async function resolveNativeEffects<T>(
  effects: T[],
  context: NativeWorkflowContext,
): Promise<{ statements: Exclude<T, NativeWorkflowWrite>[]; native: SqlResult[] }> {
  const statements: Exclude<T, NativeWorkflowWrite>[] = [];
  const native: SqlResult[] = [];
  for (const effect of effects) {
    if (isNativeWorkflowWrite(effect)) native.push(await effect.withContext(context).run());
    else statements.push(effect as Exclude<T, NativeWorkflowWrite>);
  }
  return { statements, native };
}
