import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";

/**
 * Strict private transport for the native POS transaction-evidence search.
 * The axum owner (lane 25) executes the SQL, projects the fixed row shape and
 * enforces the Company scope. This bridge emits no SQL, accepts no SQL and has
 * no local fallback reader: a missing, malformed or failing native service
 * fails closed with ApiError(503).
 *
 * Every registered Holding is an explicit config entry. The target is selected
 * only from the caller's WorkContext (`holdingId` + `holdingSlug`); there is no
 * default/first Holding and an unknown target fails closed with 409.
 */
type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  /** Optional operator metadata for the Holding's development Company. The
   * request Company is always the caller's current WorkContext company. */
  companyId?: string;
}>;
type Config = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;

export type NativePosEvidenceRow = Readonly<{
  id: string;
  source_id: string;
  source_type: "sale" | "refund" | "payment";
  sale_id: string;
  event_date: string;
  receipt_number: string;
  status: string;
  sale_status: string;
  currency: string;
  method: string | null;
  amount_minor: string;
  contribution_minor: string;
  refunded_minor: string;
  revision: string;
  sale_version: string;
}>;

/** The caller's current WorkContext fields used for owner target/scope. */
export type NativePosEvidenceActor = Readonly<{
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId?: string | null;
}>;

export type NativePosEvidenceFilters = Readonly<{
  metric: "pos-sales" | "net-pos-sales" | "pos-payment-mix";
  from: string;
  to: string;
  currency: string;
  query: string;
  limit: number;
  eventDate?: string;
  id?: string;
}>;

const ENV = "ERPX_NATIVE_POS_LANE25_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const METRICS = new Set(["pos-sales", "net-pos-sales", "pos-payment-mix"]);
const ROW_KEYS = [
  "id", "source_id", "source_type", "sale_id", "event_date", "receipt_number",
  "status", "sale_status", "currency", "method", "amount_minor",
  "contribution_minor", "refunded_minor", "revision", "sale_version",
] as const;

const unavailable = (code = "NATIVE_POS_LANE25_UNAVAILABLE", status = 503): never => {
  throw new ApiError(status, code, "The native POS evidence service is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 4096): value is string =>
  typeof value === "string" && new TextEncoder().encode(value).length <= max;
const validSlug = (value: unknown): value is string =>
  text(value, 128) && /^[a-z0-9_-]+$/.test(value as string);
const validId = (value: unknown): value is string =>
  text(value, 200) && /^[A-Za-z0-9._:-]+$/.test(value as string);
const validHost = (value: unknown): value is string =>
  text(value, 253) && /^[a-z0-9.-]+$/.test(value as string);

/** Validate 1..64 explicitly registered Holdings. No business SQL is accepted. */
export function nativePosEvidenceConfiguration(raw: string | undefined): Config {
  if (!raw) return unavailable("NATIVE_POS_LANE25_CONFIGURATION_REQUIRED");
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable("NATIVE_POS_LANE25_CONFIGURATION_INVALID");
  }
  if (
    !object(value) ||
    !text(value.privateBaseUrl, 512) ||
    typeof value.timeoutMs !== "number" ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable("NATIVE_POS_LANE25_CONFIGURATION_INVALID");
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable("NATIVE_POS_LANE25_CONFIGURATION_INVALID");
  }
  const port = Number(base.port);
  if (
    base.protocol !== "http:" ||
    !["127.0.0.1", "[::1]", "localhost"].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl
  )
    return unavailable("NATIVE_POS_LANE25_CONFIGURATION_INVALID");

  const holdings: Holding[] = [];
  const holdingIds = new Set<string>();
  for (const item of value.holdings) {
    if (
      !object(item) ||
      !text(item.origin, 512) ||
      !validHost(item.host) ||
      !validId(item.holdingId) ||
      !validSlug(item.holdingSlug) ||
      (item.companyId !== undefined && !validId(item.companyId))
    )
      return unavailable("NATIVE_POS_LANE25_CONFIGURATION_INVALID");
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return unavailable("NATIVE_POS_LANE25_CONFIGURATION_INVALID");
    }
    if (
      origin.protocol !== "https:" ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      origin.host !== item.host
    )
      return unavailable("NATIVE_POS_LANE25_CONFIGURATION_INVALID");
    if (
      holdingIds.has(item.holdingId as string)
      || holdings.some((h) => h.host === item.host && h.holdingSlug === item.holdingSlug)
    )
      return unavailable("NATIVE_POS_LANE25_CONFIGURATION_INVALID");
    holdingIds.add(item.holdingId as string);
    holdings.push({
      origin: origin.origin,
      host: item.host as string,
      holdingId: item.holdingId as string,
      holdingSlug: item.holdingSlug as string,
      ...(item.companyId !== undefined ? { companyId: item.companyId as string } : {}),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

function validRow(value: unknown): value is NativePosEvidenceRow {
  if (!object(value)) return false;
  const keys = Object.keys(value);
  if (keys.length !== ROW_KEYS.length || !ROW_KEYS.every((key) => key in value)) return false;
  if (!text(value.id, 240) || !text(value.source_id, 240) || !text(value.sale_id, 240)) return false;
  if (value.source_type !== "sale" && value.source_type !== "refund" && value.source_type !== "payment")
    return false;
  if (!value.id.startsWith(`${value.source_type}:`)) return false;
  if (
    !text(value.event_date, 10) ||
    !/^\d{4}-\d{2}-\d{2}$/.test(value.event_date as string) ||
    !text(value.receipt_number, 200) ||
    !text(value.status, 40) ||
    !text(value.sale_status, 40) ||
    !/^[A-Z]{3}$/.test(value.currency as string) ||
    !(value.method === null || text(value.method, 40))
  )
    return false;
  return [
    "amount_minor", "contribution_minor", "refunded_minor", "revision", "sale_version",
  ].every((key) => text(value[key], 40) && /^-?\d+(?:\.\d+)?$/.test(value[key] as string));
}

function actorValid(actor: NativePosEvidenceActor): boolean {
  return (
    validId(actor.holdingId) &&
    validSlug(actor.holdingSlug) &&
    validId(actor.companyId) &&
    (actor.branchId === undefined || actor.branchId === null || validId(actor.branchId))
  );
}

function filtersValid(filters: NativePosEvidenceFilters): boolean {
  return (
    METRICS.has(filters.metric) &&
    text(filters.from, 10) &&
    /^\d{4}-\d{2}-\d{2}$/.test(filters.from) &&
    text(filters.to, 10) &&
    /^\d{4}-\d{2}-\d{2}$/.test(filters.to) &&
    /^[A-Z]{3}$/.test(filters.currency) &&
    text(filters.query, 100) &&
    Number.isSafeInteger(filters.limit) &&
    filters.limit >= 1 &&
    filters.limit <= 50 &&
    (filters.eventDate === undefined || text(filters.eventDate, 10)) &&
    (filters.id === undefined || text(filters.id, 240))
  );
}

/** Request the bounded, Company-scoped evidence page from the native owner. */
export async function nativePosEvidenceRows(
  actor: NativePosEvidenceActor,
  filters: NativePosEvidenceFilters,
): Promise<NativePosEvidenceRow[]> {
  const config = nativePosEvidenceConfiguration(process.env[ENV]);
  if (!actorValid(actor) || !filtersValid(filters))
    return unavailable("NATIVE_POS_LANE25_REQUEST_INVALID", 422);
  // Select only the Holding named by the caller's current WorkContext. There
  // is no first-Holding/default fallback; an unknown pair fails closed.
  const target = config.holdings.find(
    (holding) => holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
  );
  if (!target) return unavailable("WORK_CONTEXT_CHANGED", 409);

  const headers = new Headers({
    host: target.host,
    origin: target.origin,
    accept: "application/json",
    "content-type": "application/json",
    "cache-control": "no-cache",
    "x-erpx-context-company": actor.companyId,
  });
  if (actor.branchId) headers.set("x-erpx-workspace-branch", actor.branchId);
  const envelope: Record<string, unknown> = {
    metric: filters.metric,
    from: filters.from,
    to: filters.to,
    currency: filters.currency,
    query: filters.query,
    limit: filters.limit,
    cursor:
      filters.eventDate !== undefined && filters.id !== undefined
        ? { eventDate: filters.eventDate, id: filters.id }
        : null,
  };
  const body = JSON.stringify(envelope);
  if (new TextEncoder().encode(body).length > 16 * 1024)
    return unavailable("NATIVE_POS_LANE25_REQUEST_INVALID", 422);

  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/pos/evidence/search`,
    config.base,
  );
  let response: Response;
  try {
    response = await fetch(url, {
      method: "POST",
      headers: Object.fromEntries(headers),
      body,
      cache: "no-store",
      redirect: "error",
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    return unavailable();
  }
  if (response.status !== 200) {
    const payload = await response.json().catch((): unknown => null);
    const error = object(payload) && object(payload.error) ? payload.error : undefined;
    if (response.status >= 400 && response.status < 500)
      throw new ApiError(
        response.status,
        text(error?.code, 100) ? String(error!.code) : "NATIVE_POS_LANE25_READ_FAILED",
        text(error?.message, 300) ? String(error!.message) : "The native POS evidence read was rejected.",
      );
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  const rows = object(payload) ? payload.rows : undefined;
  if (!Array.isArray(rows) || !rows.every(validRow))
    return unavailable("NATIVE_POS_LANE25_RESPONSE_INVALID");
  return rows as NativePosEvidenceRow[];
}

/** True when the native POS evidence consumer switch is configured. */
export function nativePosEvidenceConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
