import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";
import { nativeRequest } from "@erpx/server-runtime/public/native-http";

/**
 * Strict private transport for the native PMS home hotel-metrics read. The
 * mounted axum owner router plans no statement: it executes the retained
 * `hotelMetrics` SQL itself, scopes the Company/branch/property and returns the
 * complete twelve-field payload. This bridge emits no SQL, accepts no SQL from
 * a caller, and fails closed with 503 when the owner, the registered Holding or
 * the response is malformed. There is no TypeScript fallback and no default
 * Holding: the Company selects exactly one registered target.
 */
export type NativeHomeMetricsRow = Readonly<{
  id: string;
  name: string;
  branch_id: string;
  business_date: string;
  state: string;
  currency: string | null;
  room_count: number;
  inspected: number;
  capacity: number;
  sold: number;
  arrivals: number;
  departures: number;
}>;

export type NativeHomeMetricsHolding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
}>;

export type NativeHomeMetricsConfig = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly NativeHomeMetricsHolding[];
}>;

export type NativeHomeMetricsRequest = Readonly<{
  companyId: string;
  branchId?: string | null;
  propertyId?: string | null;
}>;

type Settings = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly NativeHomeMetricsHolding[];
}>;
type Fetch = (input: string | URL, init?: RequestInit) => Promise<Response>;

const ENV = "ERPX_NATIVE_HOME_METRICS_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const ROW_FIELDS = [
  "id", "name", "branch_id", "business_date", "state", "currency",
  "room_count", "inspected", "capacity", "sold", "arrivals", "departures",
] as const;

const unavailable = (
  code = "PMS_NATIVE_HOME_METRICS_UNAVAILABLE",
  message = "The native PMS hotel-metrics read is unavailable.",
): never => {
  throw new ApiError(503, code, message);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 200): value is string =>
  typeof value === "string" && value.length > 0 && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown, max = 200): value is string =>
  text(value, max) && !/[\u0000-\u001f/?#\\]/.test(value);
const slug = (value: unknown): value is string =>
  text(value, 128) && /^[a-z0-9_-]+$/.test(value);
const hostname = (value: unknown): value is string =>
  text(value, 253) && /^[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?$/.test(value);

export function parseNativeHomeMetricsConfig(value: unknown): Settings {
  if (
    !record(value) ||
    !text(value.privateBaseUrl, 512) ||
    typeof value.timeoutMs !== "number" ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable("PMS_NATIVE_HOME_METRICS_CONFIGURATION_INVALID");
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable("PMS_NATIVE_HOME_METRICS_CONFIGURATION_INVALID");
  }
  const port = Number(base.port);
  if (
    base.protocol !== "http:" ||
    !["127.0.0.1", "[::1]"].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl
  )
    return unavailable("PMS_NATIVE_HOME_METRICS_CONFIGURATION_INVALID");

  const holdings: NativeHomeMetricsHolding[] = [];
  const companies = new Set<string>();
  const targets = new Set<string>();
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.origin, 512) ||
      !hostname(item.host) ||
      !identifier(item.holdingId) ||
      !slug(item.holdingSlug) ||
      !identifier(item.companyId)
    )
      return unavailable("PMS_NATIVE_HOME_METRICS_CONFIGURATION_INVALID");
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return unavailable("PMS_NATIVE_HOME_METRICS_CONFIGURATION_INVALID");
    }
    if (
      origin.protocol !== "https:" ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      origin.host !== item.host ||
      companies.has(item.companyId) ||
      targets.has(`${item.host}\u0000${item.holdingSlug}`)
    )
      return unavailable("PMS_NATIVE_HOME_METRICS_CONFIGURATION_INVALID");
    companies.add(item.companyId);
    targets.add(`${item.host}\u0000${item.holdingSlug}`);
    holdings.push({
      origin: origin.origin,
      host: item.host,
      holdingId: item.holdingId,
      holdingSlug: item.holdingSlug,
      companyId: item.companyId,
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Pick exactly one registered Holding by Company. No first-holding default
 * is permitted and a Company must map to a single registered target. */
function selectTarget(settings: Settings, companyId: string): NativeHomeMetricsHolding {
  if (!identifier(companyId))
    return unavailable("PMS_HOME_METRICS_REQUEST_INVALID", "Use one bounded hotel metrics read.");
  const target = settings.holdings.find((holding) => holding.companyId === companyId);
  return target ?? unavailable("PMS_NATIVE_HOME_METRICS_TARGET_UNAVAILABLE");
}

function validateRows(payload: unknown): readonly NativeHomeMetricsRow[] {
  if (!record(payload) || !Array.isArray(payload.hotels))
    return unavailable("PMS_NATIVE_HOME_METRICS_RESPONSE_INVALID");
  return payload.hotels.map((item: unknown) => {
    if (!record(item)) return unavailable("PMS_NATIVE_HOME_METRICS_RESPONSE_INVALID");
    for (const field of ROW_FIELDS)
      if (!(field in item)) return unavailable("PMS_NATIVE_HOME_METRICS_RESPONSE_INVALID");
    if (
      !identifier(item.id) ||
      !text(item.name, 512) ||
      !identifier(item.branch_id) ||
      !text(item.business_date, 32) ||
      !identifier(item.state) ||
      !(item.currency === null || text(item.currency, 16)) ||
      !Number.isSafeInteger(item.room_count) ||
      !Number.isSafeInteger(item.inspected) ||
      !Number.isSafeInteger(item.capacity) ||
      !Number.isSafeInteger(item.sold) ||
      !Number.isSafeInteger(item.arrivals) ||
      !Number.isSafeInteger(item.departures)
    )
      return unavailable("PMS_NATIVE_HOME_METRICS_RESPONSE_INVALID");
    return item as unknown as NativeHomeMetricsRow;
  });
}

/** Create a client with an injectable transport for contract tests. */
export function createNativeHomeMetricsClient(
  config: unknown,
  fetchImpl: Fetch = (input, init = {}) => nativeRequest(
    typeof input === "string" ? new URL(input) : input,
    {
      method: init.method,
      headers: Object.fromEntries(new Headers(init.headers).entries()),
      body: typeof init.body === "string" ? init.body : undefined,
      signal: init.signal ?? undefined,
    },
    15_000,
  ),
) {
  const settings = parseNativeHomeMetricsConfig(config);
  return {
    async hotelMetrics(request: NativeHomeMetricsRequest): Promise<readonly NativeHomeMetricsRow[]> {
      const target = selectTarget(settings, request.companyId);
      const headers = new Headers({
        host: target.host,
        origin: target.origin,
        accept: "application/json",
        "cache-control": "no-cache",
        "x-erpx-context-company": request.companyId,
      });
      if (request.branchId !== undefined && request.branchId !== null) {
        if (!identifier(request.branchId))
          return unavailable("PMS_HOME_METRICS_REQUEST_INVALID", "Use one bounded hotel metrics read.");
        headers.set("x-erpx-context-branch", request.branchId);
      }
      if (request.propertyId !== undefined && request.propertyId !== null) {
        if (!identifier(request.propertyId))
          return unavailable("PMS_HOME_METRICS_REQUEST_INVALID", "Use one bounded hotel metrics read.");
        headers.set("x-erpx-context-property", request.propertyId);
      }
      const url = new URL(
        `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/pms/home-metrics`,
        settings.base,
      );
      let response: Response;
      try {
        response = await fetchImpl(url, {
          method: "GET",
          headers: Object.fromEntries(headers),
          cache: "no-store",
          redirect: "error",
          signal: AbortSignal.timeout(settings.timeoutMs),
        });
      } catch {
        return unavailable();
      }
      if (response.status !== 200) {
        const payload = await response.json().catch((): unknown => null);
        const error = record(payload) && record(payload.error) ? payload.error : undefined;
        if (response.status >= 400 && response.status < 500)
          throw new ApiError(
            response.status,
            text(error?.code, 100) ? String(error!.code) : "PMS_NATIVE_HOME_METRICS_FAILED",
            text(error?.message, 300) ? String(error!.message) : "The native PMS hotel-metrics read was rejected.",
          );
        return unavailable();
      }
      const payload = await response.json().catch((): unknown => null);
      return validateRows(payload);
    },
  };
}

/** Env-bound entry point used by the owner adapter. */
export async function nativeHomeMetrics(
  request: NativeHomeMetricsRequest,
): Promise<readonly NativeHomeMetricsRow[]> {
  const raw = process.env[ENV];
  if (!raw) return unavailable("PMS_NATIVE_HOME_METRICS_CONFIGURATION_REQUIRED");
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return unavailable("PMS_NATIVE_HOME_METRICS_CONFIGURATION_INVALID");
  }
  return createNativeHomeMetricsClient(parsed).hotelMetrics(request);
}
