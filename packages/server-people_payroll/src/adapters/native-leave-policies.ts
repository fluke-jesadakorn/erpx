import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import { nativeRequest } from '@erpx/server-runtime/public/native-http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

/**
 * Native active leave-policy read. The axum owner executes the Company-scoped
 * `leave_policy_versions` query and returns the exact `{name,version,status}`
 * rows; this transport carries no SQL and no fallback.
 */
type Holding = Readonly<{ origin: string; host: string; holdingSlug: string; holdingId: string }>;
type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;
type Row = Readonly<{ name: string; version: number; status: string }>;

const ENV = 'ERPX_NATIVE_PEOPLE_PAYROLL_LEAVE_POLICIES_CONFIG';
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const unavailable = (): never => {
  throw new ApiError(503, 'PEOPLE_PAYROLL_LEAVE_POLICIES_UNAVAILABLE', 'The native leave-policy read is unavailable.');
};
const record = (value: unknown): value is Record<string, unknown> => value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 200): value is string => typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string => text(value) && !/[\u0000-\u001f/?#\\]/.test(value);
const slug = (value: unknown): value is string => text(value, 128) && /^[a-z0-9_-]+$/.test(value);

function configuration(raw = process.env[ENV]): Settings {
  if (raw === undefined) return unavailable();
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return unavailable(); }
  if (!record(value) || !text(value.privateBaseUrl, 512) || typeof value.timeoutMs !== 'number'
    || !Number.isSafeInteger(value.timeoutMs) || value.timeoutMs < 250 || value.timeoutMs > 30_000
    || !Array.isArray(value.holdings) || value.holdings.length < 1 || value.holdings.length > 64) return unavailable();
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return unavailable(); }
  if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port
    || reservedPorts.has(Number(base.port)) || (Number(base.port) >= 3200 && Number(base.port) <= 3299)
    || base.username || base.password || base.origin !== value.privateBaseUrl) return unavailable();
  const holdings = value.holdings.map((item) => {
    if (!record(item) || !text(item.origin, 512) || !identifier(item.holdingId) || !slug(item.holdingSlug)) return unavailable();
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return unavailable(); }
    if (origin.protocol !== 'https:' || origin.origin !== item.origin || origin.username || origin.password) return unavailable();
    return { origin: item.origin, host: origin.host, holdingSlug: item.holdingSlug, holdingId: item.holdingId };
  });
  return { privateBaseUrl: value.privateBaseUrl, timeoutMs: value.timeoutMs, holdings };
}


export async function nativeActiveLeavePolicySettings(
  actor: WorkContext,
  send: (url: URL, init: { method: string; headers: Record<string, string> }, timeoutMs: number) => Promise<Response> = nativeRequest,
): Promise<Row[]> {
  const settings = configuration();
  if (!identifier(actor.holdingId) || !slug(actor.holdingSlug) || !identifier(actor.companyId)) return unavailable();
  const holding = settings.holdings.find((item) => item.holdingId === actor.holdingId && item.holdingSlug === actor.holdingSlug);
  if (!holding) return unavailable();
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/people-payroll/active-leave-policies`, settings.privateBaseUrl);
  let response: Response;
  try {
    response = await send(url, {
      method: 'GET',
      headers: {
        host: holding.host,
        origin: holding.origin,
        accept: 'application/json',
        'cache-control': 'no-cache',
        'x-erpx-context-company': actor.companyId,
      },
    }, settings.timeoutMs);
  } catch { return unavailable(); }
  if (response.status !== 200) {
    if (response.status >= 400 && response.status < 500) {
      const payload = await response.json().catch((): unknown => null);
      const error = record(payload) && record(payload.error) ? payload.error : undefined;
      throw new ApiError(response.status, text(error?.code) ? String(error.code) : 'PEOPLE_PAYROLL_LEAVE_POLICIES_FAILED',
        text(error?.message) ? String(error.message) : 'The native leave-policy read was rejected.');
    }
    void response.body?.cancel().catch(() => undefined);
    return unavailable();
  }
  const body: unknown = await response.json().catch((): unknown => null);
  if (!Array.isArray(body)) return unavailable();
  return body.map((item) => {
    if (!record(item) || !text(item.name) || !Number.isSafeInteger(item.version) || !text(item.status)) return unavailable();
    return { name: item.name, version: item.version as number, status: item.status };
  });
}
