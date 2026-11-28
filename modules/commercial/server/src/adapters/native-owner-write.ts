import 'server-only';
import type { SqlResult, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';

/** Thin fail-closed transport to the single native Commercial owner axum
 * command router. `values` is a positional data array, never SQL text. */
type NativeConfig = Readonly<{ base: string; host: string; holdingSlug: string; origin: string; timeoutMs: number }>;

function config(): NativeConfig {
  const raw = process.env.ERPX_NATIVE_COMMERCIAL_OWNER_WRITE_CONFIG;
  if (raw === undefined) throw new NativeCommercialOwnerWriteError('COMMERCIAL_OWNER_WRITE_UNAVAILABLE', 503, 'The native Commercial service is not configured.');
  let value: { privateBaseUrl?: unknown; host?: unknown; holdingSlug?: unknown; origin?: unknown; timeoutMs?: unknown };
  try { value = JSON.parse(raw) as typeof value; } catch { throw new NativeCommercialOwnerWriteError('COMMERCIAL_OWNER_WRITE_CONFIG_INVALID', 503, 'The native Commercial transport is misconfigured.'); }
  let base: URL, origin: URL;
  try { base = new URL(String(value.privateBaseUrl)); origin = new URL(String(value.origin)); }
  catch { throw new NativeCommercialOwnerWriteError('COMMERCIAL_OWNER_WRITE_CONFIG_INVALID', 503, 'The native Commercial transport is misconfigured.'); }
  if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || origin.protocol !== 'https:' ||
      typeof value.host !== 'string' || value.host !== origin.host ||
      typeof value.holdingSlug !== 'string' || !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug) ||
      !Number.isInteger(value.timeoutMs) || (value.timeoutMs as number) < 250 || (value.timeoutMs as number) > 30000)
    throw new NativeCommercialOwnerWriteError('COMMERCIAL_OWNER_WRITE_CONFIG_INVALID', 503, 'The native Commercial transport is misconfigured.');
  return { base: base.origin, host: value.host, holdingSlug: value.holdingSlug, origin: origin.origin, timeoutMs: value.timeoutMs as number };
}

export class NativeCommercialOwnerWriteError extends Error {
  readonly code: string; readonly status: number;
  constructor(code: string, status: number, message: string) { super(message); this.name = 'NativeCommercialOwnerWriteError'; this.code = code; this.status = status; }
}
const isRecord = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);

export type CommercialOwnerWriteKind =
  | 'review.evidence' | 'grant.revision-anchor' | 'change-set.receipt'
  | 'usage.snapshot-ensure' | 'usage.snapshot-finish' | 'usage.meter'
  | 'usage.meter-prune' | 'usage.seat' | 'usage.seat-prune' | 'purchase.assertion';

export async function nativeCommercialOwnerWrite(
  kind: CommercialOwnerWriteKind, values: readonly unknown[], recordId: string,
  actorId: string, idempotencyKey: string, holdId?: string,
): Promise<{ id: string; kind: string }> {
  const cfg = config();
  const headers = new Headers({
    host: cfg.host, origin: cfg.origin, accept: 'application/json', 'content-type': 'application/json',
    'idempotency-key': idempotencyKey, 'x-erpx-context-user': actorId,
  });
  let response: Response;
  try {
    response = await fetch(
      `${cfg.base}/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/commercial/owner-write`,
      { method: 'POST', headers, body: JSON.stringify({ kind, values, recordId, companyId: holdId ?? recordId }), cache: 'no-store', signal: AbortSignal.timeout(cfg.timeoutMs) },
    );
  } catch { throw new NativeCommercialOwnerWriteError('COMMERCIAL_OWNER_WRITE_UNAVAILABLE', 503, 'The native Commercial service is unavailable.'); }
  const value: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const error = isRecord(value) && isRecord(value.error) ? value.error : {};
    throw new NativeCommercialOwnerWriteError(typeof error.code === 'string' ? error.code : 'COMMERCIAL_OWNER_WRITE_FAILED', response.status, typeof error.message === 'string' ? error.message : 'The Commercial write could not be completed.');
  }
  if (!isRecord(value) || !isRecord(value.data) || typeof value.data.id !== 'string' || typeof value.data.kind !== 'string')
    throw new NativeCommercialOwnerWriteError('COMMERCIAL_OWNER_WRITE_RESPONSE_INVALID', 503, 'The native Commercial acknowledgement was invalid.');
  return { id: value.data.id, kind: value.data.kind };
}


/** A caller-transaction-compatible statement whose run() performs the write
 * through the native axum command route. Carries no SQL text. */
export function nativeOwnerStatement(
  kind: CommercialOwnerWriteKind,
  values: readonly unknown[],
  recordId: string,
  actorId: string,
  idempotencyKey: string,
  holdId?: string,
): SqlStatement {
  const statement: SqlStatement = {
    bind() { return statement; },
    async run<T>(): Promise<SqlResult<T>> {
      await nativeCommercialOwnerWrite(kind, values, recordId, actorId, idempotencyKey, holdId);
      return { results: [], success: true, meta: { changes: 1, duration: 0 } };
    },
    async first<T>(): Promise<T | null> { throw new NativeCommercialOwnerWriteError('COMMERCIAL_OWNER_WRITE_READ_UNSUPPORTED', 503, 'Write-only.'); },
    async all<T>(): Promise<SqlResult<T>> { throw new NativeCommercialOwnerWriteError('COMMERCIAL_OWNER_WRITE_READ_UNSUPPORTED', 503, 'Write-only.'); },
    async raw<T>(): Promise<T[]> { throw new NativeCommercialOwnerWriteError('COMMERCIAL_OWNER_WRITE_READ_UNSUPPORTED', 503, 'Write-only.'); },
  };
  return statement;
}
