import 'server-only';
import { headers } from 'next/headers';
import { ApiError } from '@erpx/server-runtime/public/http';
import { nativeRequest } from '@erpx/server-runtime/public/native-http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { Row } from '@erpx/server-commerce/public/commerce';

/**
 * Strict owner transport for the native CRM case-event history GET route.
 *
 * The axum owner executes the `crm_case_events` SELECT and returns the full
 * immutable row projection. This bridge carries no SQL, accepts no SQL from a
 * caller and never falls back to the retired TypeScript reader: missing or
 * invalid configuration, an unregistered Holding or a malformed reply fails
 * closed with 503.
 */
type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId: string; companyId: string }>;
type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;

const MAX_ROWS = 5000;
const unavailable = (): never => {
  throw new ApiError(503, 'CRM_NATIVE_HISTORY_UNAVAILABLE', 'The native CRM history service is unavailable.');
};
const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const identifier = (v: unknown): v is string => typeof v === 'string' && v.length > 0 && new TextEncoder().encode(v).length <= 200;
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_CRM_CONFIG;
  if (!raw) return unavailable();
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return unavailable(); }
  if (!record(value) || typeof value.privateBaseUrl !== 'string' || !Array.isArray(value.holdings)) return unavailable();
  if (typeof value.timeoutMs !== 'number' || !Number.isSafeInteger(value.timeoutMs) || value.timeoutMs < 250 || value.timeoutMs > 30_000) return unavailable();
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return unavailable(); }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
    reservedPorts.has(port) || (port >= 3200 && port <= 3299) || base.username || base.password ||
    value.privateBaseUrl !== base.origin
  ) return unavailable();
  const holdings = value.holdings.map((item: unknown) => {
    if (!record(item) || typeof item.origin !== 'string' || typeof item.holdingSlug !== 'string' ||
        typeof item.holdingId !== 'string' || typeof item.companyId !== 'string') return unavailable();
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return unavailable(); }
    if (origin.protocol !== 'https:' || item.origin !== origin.origin || origin.username || origin.password ||
        !identifier(item.companyId) || !/^[a-z0-9_-]+$/.test(item.holdingSlug)) return unavailable();
    return { origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId, companyId: item.companyId };
  });
  if (holdings.length < 1 || holdings.length > 64) return unavailable();
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs, holdings };
}

function row(value: unknown): Row {
  if (!record(value) ||
      typeof value.id !== 'string' || typeof value.lead_id !== 'string' ||
      (typeof value.opportunity_id !== 'string' && value.opportunity_id !== null) ||
      typeof value.action !== 'string' || typeof value.actor_id !== 'string' || typeof value.reason !== 'string' ||
      typeof value.version !== 'number' || !Number.isSafeInteger(value.version) ||
      typeof value.created_at !== 'number' || !Number.isSafeInteger(value.created_at) ||
      !('snapshot_json' in value)) return unavailable();
  return value as unknown as Row;
}

/** Read one lead's immutable case-event history from the native owner GET. */
export async function nativeCrmCaseEventHistory(actor: WorkContext, leadId: string): Promise<Row[]> {
  const config = configuration();
  const holding = config.holdings.find(h => h.holdingSlug === actor.holdingSlug && h.holdingId === actor.holdingId && h.companyId === actor.companyId);
  if (!holding || typeof leadId !== 'string' || !leadId || leadId.length > 200 || /[\u0000-\u001f\u007f]/.test(leadId)) {
    throw new ApiError(422, 'CRM_HISTORY_LEAD_INVALID', 'Choose one bounded CRM lead.');
  }
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/crm/case-events?${new URLSearchParams({ leadId })}`, config.privateBaseUrl);
  const cookie = (await headers()).get('cookie');
  if (!cookie) throw new ApiError(401, 'UNAUTHENTICATED', 'Sign in to read CRM history.');
  const context: Record<string, string> = {
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-holding': actor.holdingId,
  };
  if (actor.authority) {
    context['x-erpx-context-assignment'] = actor.assignmentId;
    context['x-erpx-context-revision'] = String(actor.authority.revision);
    context['x-erpx-context-assignment-version'] = String(actor.authority.assignmentVersion);
  }
  if (actor.scope.kind !== 'holding' && actor.scope.branchSlug)
    context['x-erpx-workspace-branch'] = actor.scope.branchSlug;
  if (actor.scope.kind === 'department')
    context['x-erpx-workspace-department'] = actor.scope.departmentSlug;
  let response: Response;
  try {
    response = await nativeRequest(url, {
      method: 'GET',
      headers: {
        host: new URL(holding.origin).host,
        origin: holding.origin,
        cookie,
        accept: 'application/json',
        'cache-control': 'no-cache',
        ...context,
      },
    }, config.timeoutMs);
  } catch { return unavailable(); }
  if (response.status >= 400 && response.status < 500) {
    const payload = await response.json().catch((): unknown => null);
    const err = record(payload) ? (payload as { error?: { code?: string; message?: string } }).error : undefined;
    throw new ApiError(response.status, err?.code ?? 'CRM_NATIVE_HISTORY_FAILED', err?.message ?? 'The native CRM history read was rejected.');
  }
  if (response.status !== 200 || response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return unavailable();
  }
  const payload: unknown = await response.json().catch((): unknown => null);
  if (!record(payload) || !Array.isArray(payload.data) || payload.data.length > MAX_ROWS) return unavailable();
  return payload.data.map(row);
}
