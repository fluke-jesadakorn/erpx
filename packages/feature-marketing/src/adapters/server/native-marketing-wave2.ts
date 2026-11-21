import 'server-only';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/server-runtime/public/http';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

/**
 * Wave-2 private transport for the native Marketing command routes. The owner
 * axum router owns the campaign/candidate/asset SQL and the command ledger;
 * this adapter only forwards the exact registered Holding Host/Origin, one
 * deterministic Idempotency-Key and the caller context. No SQL, no fallback.
 */
type NativeConfig = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;

function nativeConfig(): NativeConfig {
  const raw = process.env.ERPX_NATIVE_MARKETING_WAVE2_CONFIG;
  if (raw === undefined)
    throw new ApiError(503, 'MARKETING_COMMAND_UNAVAILABLE', 'The native Marketing command service is not configured.');
  let value: {
    privateBaseUrl?: unknown; host?: unknown; holdingSlug?: unknown; origin?: unknown; timeoutMs?: unknown;
  };
  try { value = JSON.parse(raw) as typeof value; }
  catch { throw new ApiError(503, 'MARKETING_COMMAND_CONFIGURATION_INVALID', 'The native Marketing transport is misconfigured.'); }
  let base: URL, origin: URL;
  try { base = new URL(String(value.privateBaseUrl)); origin = new URL(String(value.origin)); }
  catch { throw new ApiError(503, 'MARKETING_COMMAND_CONFIGURATION_INVALID', 'The native Marketing transport is misconfigured.'); }
  if (
    base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    origin.protocol !== 'https:' || typeof value.host !== 'string' || value.host !== origin.host ||
    typeof value.holdingSlug !== 'string' || !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug) ||
    !Number.isInteger(value.timeoutMs) || (value.timeoutMs as number) < 250 || (value.timeoutMs as number) > 30_000
  ) throw new ApiError(503, 'MARKETING_COMMAND_CONFIGURATION_INVALID', 'The native Marketing transport is misconfigured.');
  return { base: base.origin, host: value.host, holdingSlug: value.holdingSlug, origin: origin.origin, timeoutMs: value.timeoutMs as number };
}

function idempotencyKey(a: WorkContext, path: string, body: object, key: string): string {
  return createHash('sha256').update('marketing.wave2').update(':').update(a.userId).update(':')
    .update(path).update(':').update(key).update(':').update(JSON.stringify(body)).digest('hex');
}

async function exposed<T>(action: () => Promise<T>): Promise<T> {
  try { return await action(); }
  catch (error) { if (error instanceof BusinessRuleError) throw new ApiError(error.status, error.code, error.message); throw error; }
}

async function post(a: WorkContext, key: string, path: string, body: Record<string, unknown>): Promise<Record<string, unknown>> {
  const config = nativeConfig();
  if (config.holdingSlug !== a.holdingSlug)
    throw new ApiError(403, 'MARKETING_HOLDING_MISMATCH', 'The native Marketing transport is bound to another Holding.');
  const permissions = a.iam?.permissions?.length ? a.iam.permissions.join(',') : 'records.write';
  const headers = new Headers({
    host: config.host,
    origin: config.origin,
    accept: 'application/json',
    'content-type': 'application/json',
    'idempotency-key': idempotencyKey(a, path, body, key),
    'x-erpx-context-user': a.userId,
    'x-erpx-context-company': a.companyId,
    'x-erpx-context-assignment': a.assignmentId,
    'x-erpx-context-assignment-version': String(a.assignments.find((x) => x.id === a.assignmentId)?.version ?? 1),
    'x-erpx-context-revision': String(a.authority?.revision ?? 0),
    'x-erpx-context-role': a.role,
    'x-erpx-context-permissions': permissions,
    'x-erpx-context-modules': a.modules.join(','),
    'x-erpx-context-expires-at': String(Date.now() + 60_000),
    'x-erpx-context-branch-id': a.branchId ?? '-',
    'x-erpx-context-department-id': a.departmentId ?? '-',
  });
  let response: Response;
  try {
    response = await fetch(`${config.base}/api/v1/holdings/${encodeURIComponent(config.holdingSlug)}${path}`,
      { method: 'POST', headers, body: JSON.stringify(body), cache: 'no-store', signal: AbortSignal.timeout(config.timeoutMs) });
  } catch {
    throw new ApiError(503, 'MARKETING_COMMAND_UNAVAILABLE', 'The native Marketing command service is unavailable.');
  }
  const value = await response.json().catch(() => null) as { data?: Record<string, unknown>; error?: { code?: string; message?: string } } | null;
  if (!response.ok)
    throw new ApiError(response.status, value?.error?.code ?? 'MARKETING_COMMAND_FAILED', value?.error?.message ?? 'The Marketing command could not be completed.');
  if (!value?.data || typeof value.data !== 'object')
    throw new ApiError(503, 'MARKETING_COMMAND_UNAVAILABLE', 'The native Marketing reply was invalid.');
  return value.data;
}

export const nativeCreateCampaign = (a: WorkContext, raw: unknown, key: string) =>
  exposed(async () => {
    const input = raw && typeof raw === 'object' ? raw as Record<string, unknown> : {};
    const body = { code: String(input.code ?? ''), name: String(input.name ?? '') };
    return post(a, key, '/marketing/campaigns', body);
  });

export const nativeCreateCandidate = (a: WorkContext, raw: unknown, key: string) =>
  exposed(async () => {
    const input = raw && typeof raw === 'object' ? raw as Record<string, unknown> : {};
    const attribution = input.attribution && typeof input.attribution === 'object' && !Array.isArray(input.attribution)
      ? Object.fromEntries(Object.entries(input.attribution).map(([k, v]) => [k, String(v)])) : {};
    const body = {
      formId: String(input.formId ?? ''), formVersion: Number(input.formVersion), branchId: String(input.branchId ?? a.branchId ?? ''),
      contactName: String(input.contactName ?? ''), contactDetail: String(input.contactDetail ?? ''),
      consentEvidence: String(input.consentEvidence ?? ''), attribution, brief: String(input.brief ?? ''),
    };
    return post(a, key, '/marketing/leads', body);
  });

export const nativeCreateAsset = (a: WorkContext, raw: unknown, key: string) =>
  exposed(async () => {
    const input = raw && typeof raw === 'object' ? raw as Record<string, unknown> : {};
    const body = {
      kind: String(input.kind ?? ''), name: String(input.name ?? ''),
      configuration: input.configuration && typeof input.configuration === 'object' ? input.configuration : {},
      preview: input.preview && typeof input.preview === 'object' ? input.preview : {},
    };
    return post(a, key, '/marketing/assets', body);
  });

export const nativeReviewAsset = (a: WorkContext, id: string, raw: unknown, key: string) =>
  exposed(async () => {
    const input = raw && typeof raw === 'object' ? raw as Record<string, unknown> : {};
    const body = { action: String(input.action ?? ''), expectedVersion: Number(input.expectedVersion), reason: String(input.reason ?? '') };
    return post(a, key, `/marketing/assets/${encodeURIComponent(id)}/changes`, body);
  });

export const nativeRecordEngagement = (a: WorkContext, id: string, raw: unknown, key: string) =>
  exposed(async () => {
    const input = raw && typeof raw === 'object' ? raw as Record<string, unknown> : {};
    const body = {
      action: 'record-engagement', expectedVersion: Number(input.expectedVersion), reason: String(input.reason ?? ''),
      eventType: String(input.eventType ?? ''), channel: String(input.channel ?? ''),
    };
    return post(a, key, `/marketing/leads/${encodeURIComponent(id)}/changes`, body);
  });
