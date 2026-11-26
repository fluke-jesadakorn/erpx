import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

/**
 * Strict server-side transport for the native Records reference-search route.
 * It carries no SQL and offers no TypeScript fallback: the axum owner executes
 * the `workspace_records` read and returns the exact legacy `data` array. A
 * missing transport, malformed reply or rejected request fails closed.
 */
type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId: string; companyId: string }>;
type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;
type Reply = Readonly<{ data?: unknown; error?: { code?: string; message?: string } }>;

const unavailable = (): never => {
  throw new ApiError(503, 'NATIVE_RECORDS_SEARCH_UNAVAILABLE', 'The native Records search is unavailable.');
};
const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_RECORDS_CONFIG;
  if (!raw) return unavailable();
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return unavailable(); }
  if (!record(value) || typeof value.privateBaseUrl !== 'string' || !Array.isArray(value.holdings)) return unavailable();
  const timeoutMs = value.timeoutMs;
  if (typeof timeoutMs !== 'number' || !Number.isSafeInteger(timeoutMs) || timeoutMs < 250 || timeoutMs > 30000) return unavailable();
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return unavailable(); }
  const port = Number(base.port);
  if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port || reservedPorts.has(port) || (port >= 3200 && port <= 3299) || base.username || base.password || value.privateBaseUrl !== base.origin) return unavailable();
  const holdings = value.holdings.map((item: unknown) => {
    if (!record(item) || typeof item.origin !== 'string' || typeof item.holdingSlug !== 'string' || typeof item.holdingId !== 'string' || typeof item.companyId !== 'string') return unavailable();
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return unavailable(); }
    if (origin.protocol !== 'https:' || item.origin !== origin.origin || origin.username || origin.password || !/^[a-z0-9_-]+$/.test(item.holdingSlug)) return unavailable();
    return { origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId, companyId: item.companyId };
  });
  if (holdings.length < 1 || holdings.length > 64) return unavailable();
  return { privateBaseUrl: value.privateBaseUrl, holdings, timeoutMs };
}

export function nativeRecordSearchAvailable(): boolean {
  return process.env.ERPX_NATIVE_RECORDS_CONFIG !== undefined;
}

/** Returns the exact owner `data` array; the caller adds presentation links. */
export async function nativeRecordSearch(
  actor: WorkContext,
  q: string,
): Promise<Record<string, unknown>[]> {
  if (typeof q !== 'string' || q.length < 1 || q.length > 800 || q.includes('\0')) return unavailable();
  const config = configuration();
  const holding = config.holdings.find(h => h.holdingSlug === actor.holdingSlug && h.holdingId === actor.holdingId);
  if (!holding || !actor.companyId) return unavailable();
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/records/native-search`, config.privateBaseUrl);
  url.searchParams.set('q', q);
  const headers: Record<string, string> = {
    host: new URL(holding.origin).host,
    origin: holding.origin,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-user': actor.userId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-modules': actor.modules.join(','),
  };
  if (actor.branchId) headers['x-erpx-workspace-branch'] = actor.branchId;
  if (actor.departmentId) headers['x-erpx-workspace-department'] = actor.departmentId;

  let response: Response;
  try {
    response = await fetch(url, { method: 'GET', headers, cache: 'no-store', redirect: 'error', signal: AbortSignal.timeout(config.timeoutMs) });
  } catch { return unavailable(); }
  if (response.status !== 200 || response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    if (response.status >= 400 && response.status < 500) {
      const payload = await response.json().catch((): unknown => null);
      const err = record(payload) ? (payload as Reply).error : undefined;
      throw new ApiError(response.status, err?.code ?? 'NATIVE_RECORDS_SEARCH_FAILED', err?.message ?? 'The native Records search was rejected.');
    }
    void response.body?.cancel().catch(() => undefined);
    return unavailable();
  }
  const body: unknown = await response.json().catch((): unknown => null);
  if (!record(body) || !Array.isArray(body.data)) return unavailable();
  return body.data as Record<string, unknown>[];
}
