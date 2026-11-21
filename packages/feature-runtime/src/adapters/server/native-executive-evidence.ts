import 'server-only';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/server-runtime/public/http';
import { BusinessRuleError } from '@erpx/server-shared/index';

/**
 * The write side of Executive evidence access is a native axum command (see the
 * `executive_evidence_http` router in the runtime owner crate). These adapters
 * are thin private transports: they forward the exact registered Holding
 * Host/Origin, one deterministic Idempotency-Key and the caller identity to
 * that endpoint. No SQL, no fallback and no write path remain in TypeScript.
 */
type NativeConfig = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;

function nativeConfig(): NativeConfig {
  const raw = process.env.ERPX_NATIVE_EXECUTIVE_EVIDENCE_CONFIG;
  if (raw === undefined)
    throw new ApiError(503, 'EXECUTIVE_EVIDENCE_UNAVAILABLE', 'The native Executive evidence service is not configured.');
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
    throw new ApiError(503, 'EXECUTIVE_EVIDENCE_CONFIGURATION_INVALID', 'The native Executive evidence transport is misconfigured.');
  }
  let base: URL, origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    throw new ApiError(503, 'EXECUTIVE_EVIDENCE_CONFIGURATION_INVALID', 'The native Executive evidence transport is misconfigured.');
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
    throw new ApiError(503, 'EXECUTIVE_EVIDENCE_CONFIGURATION_INVALID', 'The native Executive evidence transport is misconfigured.');
  return {
    base: base.origin,
    host: value.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs as number,
  };
}

export type ExecutiveEvidenceAccessInput = Readonly<{
  userId: string;
  companyId: string;
  assignmentId: string;
  provider: string;
  metric: string;
  action: 'search' | 'open';
  fieldIds: string;
  queryHash: string;
  records: ReadonlyArray<{ recordId: string; recordVersion: number }>;
}>;

function idempotencyKey(input: ExecutiveEvidenceAccessInput): string {
  return createHash('sha256')
    .update('executive.evidence.access')
    .update(':')
    .update(input.userId)
    .update(':')
    .update(JSON.stringify({
      provider: input.provider,
      metric: input.metric,
      action: input.action,
      fieldIds: input.fieldIds,
      queryHash: input.queryHash,
      records: input.records,
    }))
    .digest('hex');
}

async function exposed<T>(action: () => Promise<T>): Promise<T> {
  try { return await action(); }
  catch (error) { if (error instanceof BusinessRuleError) throw new ApiError(error.status, error.code, error.message); throw error; }
}

/** Append one Executive evidence-access audit row through the native command. */
export const recordExecutiveEvidenceAccess = (input: ExecutiveEvidenceAccessInput) => exposed(async () => {
  const config = nativeConfig();
  const body = JSON.stringify({
    userId: input.userId,
    companyId: input.companyId,
    assignmentId: input.assignmentId,
    provider: input.provider,
    metric: input.metric,
    action: input.action,
    fieldIds: input.fieldIds,
    queryHash: input.queryHash,
    records: input.records,
  });
  const headers = new Headers({
    host: config.host,
    origin: config.origin,
    accept: 'application/json',
    'content-type': 'application/json',
    'idempotency-key': idempotencyKey(input),
    'x-erpx-context-user': input.userId,
  });
  let response: Response;
  try {
    response = await fetch(
      `${config.base}/api/v1/holdings/${encodeURIComponent(config.holdingSlug)}/executive/evidence-access`,
      { method: 'POST', headers, body, cache: 'no-store', signal: AbortSignal.timeout(config.timeoutMs) },
    );
  } catch {
    throw new ApiError(503, 'EXECUTIVE_EVIDENCE_UNAVAILABLE', 'The native Executive evidence service is unavailable.');
  }
  const value = await response.json().catch(() => null) as
    | { data?: { count?: number }; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok)
    throw new ApiError(
      response.status,
      value?.error?.code ?? 'EXECUTIVE_EVIDENCE_FAILED',
      value?.error?.message ?? 'The evidence access could not be recorded.',
    );
  if (typeof value?.data?.count !== 'number')
    throw new ApiError(503, 'EXECUTIVE_EVIDENCE_UNAVAILABLE', 'The native Executive evidence reply was invalid.');
});
