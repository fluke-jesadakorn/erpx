import 'server-only';
import { ApiError, errorResponse } from '@erpx/server-runtime/public/http';
import { requireWorkContext } from '@erpx/server-identity_access/public/authorization';
import { canonicalDestinationLinks } from '@erpx/feature-work_projection/lib/destination-links';

/**
 * Native transport for the sales customer review counts read. The Rust owner
 * owns the statement, executes it on its own unit and returns the complete
 * `{kind,count}` payload at
 * GET /api/v1/holdings/{slug}/customer-reviews/counts. This bridge emits no SQL
 * and keeps no TypeScript mirror or fallback; missing configuration fails
 * closed.
 */
type Holding = Readonly<{ origin: string; holdingSlug: string; companyId: string }>;
type Settings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;

export type SalesReviewCount = Readonly<{ kind: 'identity' | 'duplicates' | 'access'; count: number }>;

const REVIEW_KINDS = new Set(['identity', 'duplicates', 'access']);

const unavailable = (): never => {
  throw new ApiError(503, 'CUSTOMER_SALES_REVIEW_COUNTS_UNAVAILABLE', 'The native sales customer review counts read is unavailable.');
};
const invalid = (): never => {
  throw new ApiError(422, 'CUSTOMER_SALES_REVIEW_COUNTS_REQUEST_INVALID', 'Use one bounded sales customer review count read.');
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_CUSTOMER_AUTHORITY_READ_CONFIG;
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
        typeof item.companyId !== 'string') return unavailable();
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return unavailable(); }
    if (origin.protocol !== 'https:' || origin.username || origin.password || item.origin !== origin.origin) return unavailable();
    return { origin: item.origin, holdingSlug: item.holdingSlug, companyId: item.companyId };
  });
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs, holdings };
}

const validCount = (value: unknown): value is SalesReviewCount =>
  record(value) && Object.keys(value).length === 2 &&
  typeof value.kind === 'string' && REVIEW_KINDS.has(value.kind) &&
  typeof value.count === 'number' && Number.isSafeInteger(value.count) && value.count >= 0;

/** Read the pending review counts from the native owner route. */
export async function nativeSalesCustomerReviewCounts(
  holdingSlug: string,
  scope: Readonly<{ branchId?: string; departmentId?: string }> = {},
): Promise<readonly SalesReviewCount[]> {
  const config = configuration();
  const holding = config.holdings.find(h => h.holdingSlug === holdingSlug);
  if (!holding) return unavailable();
  if (scope.branchId !== undefined && (typeof scope.branchId !== 'string' || scope.branchId.length === 0 || scope.branchId.length > 200)) return unavailable();
  if (scope.departmentId !== undefined && (typeof scope.departmentId !== 'string' || scope.departmentId.length === 0 || scope.departmentId.length > 200)) return unavailable();
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/customer-reviews/counts`, config.privateBaseUrl);
  if (scope.branchId) url.searchParams.set('branch', scope.branchId);
  if (scope.departmentId) url.searchParams.set('department', scope.departmentId);
  let response: Response;
  try {
    response = await fetch(url, {
      headers: {
        host: new URL(holding.origin).host,
        origin: holding.origin,
        accept: 'application/json',
        'cache-control': 'no-cache',
        'x-erpx-context-company': holding.companyId,
      },
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch { return unavailable(); }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    const payload = await response.json().catch((): unknown => null);
    const err = record(payload) ? (payload as { error?: { code?: string; message?: string } }).error : undefined;
    if (response.status >= 400 && response.status < 500) {
      throw new ApiError(response.status, err?.code ?? 'CUSTOMER_SALES_REVIEW_COUNTS_FAILED', err?.message ?? 'The native sales customer review counts read was rejected.');
    }
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  const reviews = record(payload) ? (payload as { data?: { reviews?: unknown } }).data?.reviews : undefined;
  if (!Array.isArray(reviews) || reviews.length !== 3 || !reviews.every(validCount)) return unavailable();
  const kinds = new Set(reviews.map(item => item.kind));
  if (kinds.size !== 3 || ![...kinds].every(kind => REVIEW_KINDS.has(kind))) return unavailable();
  return reviews;
}

/** The app-owned GET re-export served by the mounted native owner read. */
export async function nativeSalesCustomerReviewCountsGet(
  request: Request,
  context: { params: Promise<{ holdingSlug: string }> },
): Promise<Response> {
  try {
    const { holdingSlug } = await context.params;
    if (!/^[a-z0-9][a-z0-9_-]{0,127}$/.test(holdingSlug)) return invalid();
    const actor = await requireWorkContext(holdingSlug);
    if (new URL(request.url).searchParams.size !== 0) return invalid();
    const reviews = await nativeSalesCustomerReviewCounts(holdingSlug, {
      branchId: actor.branchId ?? undefined,
      departmentId: actor.departmentId ?? undefined,
    });
    return Response.json(canonicalDestinationLinks({ data: { reviews } }), {
      headers: { 'Cache-Control': 'private, no-store' },
    });
  } catch (error) {
    return errorResponse(error);
  }
}
