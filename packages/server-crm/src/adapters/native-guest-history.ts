import 'server-only';
import { ApiError, errorResponse } from '@erpx/server-runtime/public/http';
import { nativeRequest } from '@erpx/server-runtime/public/native-http';
import { requireWorkContext } from '@erpx/server-identity_access/public/authorization';
import { canonicalDestinationLinks } from '@erpx/feature-work_projection/lib/destination-links';

/**
 * Strict transport for the mounted native CRM contact guest-history GET.
 *
 * The axum owner at GET /api/v1/holdings/{slug}/crm?view=guest-history&id=...
 * executes the real SQL and returns the complete {contact,stays,hasMore}
 * payload. This bridge carries no SQL, accepts no SQL from a caller and never
 * falls back to the retired TypeScript authority: an absent/invalid
 * configuration, an unregistered Holding, a malformed query/reply or any owner
 * failure fails closed.
 */
type Holding = Readonly<{ host: string; origin: string; holdingSlug: string; holdingId: string; companyId: string }>;
type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;

const MAX_COOKIE = 8192;
const MAX_STAYS = 200;
const VIEW = 'guest-history';
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const text = (v: unknown, max: number): v is string => typeof v === 'string' && v.length > 0 && new TextEncoder().encode(v).length <= max;
const slugValue = (v: unknown): v is string => text(v, 128) && /^[a-z0-9_-]+$/.test(v);

const unavailable = (): never => {
  throw new ApiError(503, 'CRM_NATIVE_GUEST_HISTORY_UNAVAILABLE', 'The native CRM guest-history read is unavailable.');
};
const invalid = (): never => {
  throw new ApiError(422, 'CRM_GUEST_HISTORY_REQUEST_INVALID', 'Use one bounded CRM contact guest-history read.');
};

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
        !slugValue(item.holdingSlug) || !text(item.companyId, 200) || !text(item.holdingId, 200)) return unavailable();
    return { host: origin.host, origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId, companyId: item.companyId };
  });
  if (holdings.length < 1 || holdings.length > 64) return unavailable();
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs, holdings };
}

const stayFields = ['guest_id', 'reservation_id', 'reservation_state', 'stay_id', 'stay_state', 'arrival', 'departure', 'property_id'];
function stay(value: unknown): boolean {
  if (!record(value)) return false;
  if (!['guest_id', 'reservation_id', 'property_id'].every(key => text(value[key], 200))) return false;
  return stayFields.every(key => key in value && (value[key] === null || text(value[key], 2000)));
}
function payloadView(value: unknown): boolean {
  if (!record(value) || typeof value.hasMore !== 'boolean' || !Array.isArray(value.stays) || value.stays.length > MAX_STAYS) return false;
  if (!record(value.contact) || !text(value.contact.id, 200) || !text(value.contact.name, 2000) || !text(value.contact.account_id, 200)) return false;
  return value.stays.every(stay);
}

/** The app-owned GET re-export served by the mounted native owner read. */
export async function nativeGuestHistoryGet(request: Request, context: { params: Promise<{ holdingSlug: string }> }): Promise<Response> {
  try {
    const slug = (await context.params).holdingSlug;
    if (!slugValue(slug)) return invalid();
    await requireWorkContext(slug);
    const query = new URL(request.url).searchParams;
    if (query.size !== 2 || query.getAll('view').length !== 1 || query.get('view') !== VIEW ||
        query.getAll('id').length !== 1 || !text(query.get('id'), 200) || /[\u0000-\u001f\u007f]/.test(query.get('id')!)) return invalid();
    const config = configuration();
    const host = request.headers.get('host');
    const holding = config.holdings.find(h => h.host === host && h.holdingSlug === slug);
    if (!holding) return unavailable();

    const headers = new Headers({ host: holding.host, origin: holding.origin, accept: 'application/json', 'cache-control': 'no-cache' });
    const cookie = request.headers.get('cookie');
    if (cookie) {
      if (new TextEncoder().encode(cookie).length > MAX_COOKIE) return invalid();
      headers.set('cookie', cookie);
    }
    for (const key of ['x-erpx-workspace-branch', 'x-erpx-workspace-department']) {
      const value = request.headers.get(key);
      if (value !== null) {
        if (!slugValue(value)) return invalid();
        headers.set(key, value);
      }
    }

    const url = new URL(`/api/v1/holdings/${encodeURIComponent(slug)}/crm`, config.privateBaseUrl);
    url.searchParams.set('view', VIEW);
    url.searchParams.set('id', query.get('id')!);
    let response: Response;
    try {
      response = await nativeRequest(url, { method: 'GET', headers: Object.fromEntries(headers.entries()) }, config.timeoutMs);
    } catch { return unavailable(); }
    if (response.status >= 400 && response.status < 500) {
      void response.body?.cancel().catch(() => undefined);
      const body: unknown = await response.json().catch((): unknown => null);
      const error = record(body) && record(body.error) ? body.error : undefined;
      throw new ApiError(response.status, text(error?.code, 200) ? error!.code as string : 'CRM_NATIVE_GUEST_HISTORY_FAILED', text(error?.message, 2000) ? error!.message as string : 'The native CRM guest-history read was rejected.');
    }
    if (response.status !== 200 || response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
      void response.body?.cancel().catch(() => undefined);
      return unavailable();
    }
    const body: unknown = await response.json().catch((): unknown => null);
    if (!record(body) || !payloadView(body.data)) return unavailable();
    return Response.json(canonicalDestinationLinks({ data: body.data }), { headers: { 'Cache-Control': 'private, no-store' } });
  } catch (error) {
    return errorResponse(error);
  }
}
