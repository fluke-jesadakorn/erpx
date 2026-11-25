import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

/**
 * Strict private transport for the lane 17 owner-local landed-cost reads.
 *
 * The axum route executes the statement, applies the Company witness and
 * projects the row array. This bridge emits no SQL, accepts no SQL from a
 * caller and fails closed with an ApiError(503, ...) whenever the native owner
 * is absent, unreachable or returns an unexpected envelope. A Holding target is
 * selected only by the caller's WorkContext (holdingId + holdingSlug); there is
 * no first-holding default. There is no TypeScript fallback reader.
 */
type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Config = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;
export type Lane17Actor = Pick<WorkContext, "holdingId" | "holdingSlug" | "companyId">;

const ENV = "ERPX_NATIVE_INVENTORY_LANE17_READ_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

export const LANE17_RD_LANDED_COST_SCOPED = "landed_cost_scoped";
export const LANE17_RD_LANDED_COSTS_FOR_ORDER = "landed_costs_for_order";
export const LANE17_RD_LANDED_COST_MOVED_STOCK = "landed_cost_moved_stock";
export const LANE17_RD_LANDED_COST_WORK_ROWS = "landed_cost_work_rows";
export const LANE17_RD_REORDER_POLICIES = "reorder_policies";
export const LANE17_RD_SERVICE_SPARES = "service_spares";

export type Lane17Kind =
  | typeof LANE17_RD_LANDED_COST_SCOPED
  | typeof LANE17_RD_LANDED_COSTS_FOR_ORDER
  | typeof LANE17_RD_LANDED_COST_MOVED_STOCK
  | typeof LANE17_RD_LANDED_COST_WORK_ROWS
  | typeof LANE17_RD_REORDER_POLICIES
  | typeof LANE17_RD_SERVICE_SPARES;
export type Lane17Value = string | number | null;

const KINDS: ReadonlySet<string> = new Set([
  LANE17_RD_LANDED_COST_SCOPED,
  LANE17_RD_LANDED_COSTS_FOR_ORDER,
  LANE17_RD_LANDED_COST_MOVED_STOCK,
  LANE17_RD_LANDED_COST_WORK_ROWS,
  LANE17_RD_REORDER_POLICIES,
  LANE17_RD_SERVICE_SPARES,
]);
const VALUE_COUNTS: Readonly<Record<Lane17Kind, number>> = {
  [LANE17_RD_LANDED_COST_SCOPED]: 4,
  [LANE17_RD_LANDED_COSTS_FOR_ORDER]: 4,
  [LANE17_RD_LANDED_COST_MOVED_STOCK]: 3,
  [LANE17_RD_LANDED_COST_WORK_ROWS]: 2,
  [LANE17_RD_REORDER_POLICIES]: 3,
  [LANE17_RD_SERVICE_SPARES]: 2,
};

const unavailable = (code = "NATIVE_INVENTORY_LANE17_UNAVAILABLE"): never => {
  throw new ApiError(503, code, "The native Inventory landed-cost reader is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === "string" && new TextEncoder().encode(value).length <= max;
const boundedId = (value: unknown): value is string =>
  text(value, 200) && /^[A-Za-z0-9_.:-]+$/.test(String(value));
const validValue = (value: unknown): value is Lane17Value => {
  if (value === null) return true;
  if (typeof value === "string") return text(value, 200) && value.length > 0;
  return typeof value === "number" && Number.isSafeInteger(value);
};

function configuration(environment: Readonly<Record<string, string | undefined>> = process.env): Config {
  const raw = environment[ENV];
  if (!raw) return unavailable("NATIVE_INVENTORY_LANE17_CONFIG_REQUIRED");
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable("NATIVE_INVENTORY_LANE17_CONFIG_INVALID");
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
    return unavailable("NATIVE_INVENTORY_LANE17_CONFIG_INVALID");
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable("NATIVE_INVENTORY_LANE17_CONFIG_INVALID");
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
    return unavailable("NATIVE_INVENTORY_LANE17_CONFIG_INVALID");

  const holdings: Holding[] = [];
  const targets = new Set<string>();
  for (const item of value.holdings) {
    if (
      !object(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      String(item.host).length === 0 ||
      !boundedId(item.holdingId) ||
      !text(item.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(String(item.holdingSlug)) ||
      (item.companyId !== undefined && !boundedId(item.companyId))
    )
      return unavailable("NATIVE_INVENTORY_LANE17_CONFIG_INVALID");
    let origin: URL;
    try {
      origin = new URL(String(item.origin));
    } catch {
      return unavailable("NATIVE_INVENTORY_LANE17_CONFIG_INVALID");
    }
    if (origin.protocol !== "https:" || origin.origin !== item.origin || origin.username || origin.password)
      return unavailable("NATIVE_INVENTORY_LANE17_CONFIG_INVALID");
    const key = `${item.holdingId}|${item.holdingSlug}`;
    if (targets.has(key)) return unavailable("NATIVE_INVENTORY_LANE17_CONFIG_INVALID");
    targets.add(key);
    holdings.push({
      origin: origin.origin,
      host: String(item.host),
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
      ...(item.companyId === undefined ? {} : { companyId: String(item.companyId) }),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

function holdingFor(config: Config, actor: Lane17Actor): Holding {
  if (!boundedId(actor.holdingId) || !boundedId(actor.holdingSlug) || !boundedId(actor.companyId))
    return unavailable("NATIVE_INVENTORY_LANE17_INPUT_INVALID");
  const target = config.holdings.find(
    (holding) => holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
  );
  if (!target || (target.companyId !== undefined && target.companyId !== actor.companyId))
    throw new ApiError(409, "WORK_CONTEXT_CHANGED", "The Holding context changed.");
  return target;
}

function validateRows(value: unknown): Record<string, unknown>[] {
  if (!object(value) || !Array.isArray(value.rows) || value.rows.length > 500)
    return unavailable("NATIVE_INVENTORY_LANE17_RESPONSE_INVALID");
  for (const row of value.rows) {
    if (!object(row) || Object.keys(row).length > 64)
      return unavailable("NATIVE_INVENTORY_LANE17_RESPONSE_INVALID");
    for (const [key, item] of Object.entries(row)) {
      if (!text(key, 100)) return unavailable("NATIVE_INVENTORY_LANE17_RESPONSE_INVALID");
      if (item !== null && typeof item !== "string" && typeof item !== "number" && typeof item !== "boolean")
        return unavailable("NATIVE_INVENTORY_LANE17_RESPONSE_INVALID");
      if (typeof item === "string" && !text(item, 8192))
        return unavailable("NATIVE_INVENTORY_LANE17_RESPONSE_INVALID");
      if (typeof item === "number" && !Number.isFinite(item))
        return unavailable("NATIVE_INVENTORY_LANE17_RESPONSE_INVALID");
    }
  }
  return value.rows as Record<string, unknown>[];
}

/** Execute one admitted landed-cost read on the native Inventory owner. The
 * Holding target comes only from the caller's WorkContext; both the kind and
 * the value count are closed before any request is emitted. */
export async function readLane17Rows<T = Record<string, unknown>>(
  kind: string,
  values: readonly unknown[],
  actor: Lane17Actor,
  environment: Readonly<Record<string, string | undefined>> = process.env,
): Promise<T[]> {
  const config = configuration(environment);
  const target = holdingFor(config, actor);
  if (!KINDS.has(kind) || !boundedId(actor.companyId))
    return unavailable("NATIVE_INVENTORY_LANE17_INPUT_INVALID");
  const admitted = kind as Lane17Kind;
  if (values.length !== VALUE_COUNTS[admitted] || !values.every(validValue))
    return unavailable("NATIVE_INVENTORY_LANE17_INPUT_INVALID");
  // The owner repeats the Company scope on every company-predicated statement.
  // The order-scoped moved-stock probe carries an owner-side Company witness.
  if (
    (admitted === LANE17_RD_LANDED_COST_SCOPED ||
      admitted === LANE17_RD_LANDED_COSTS_FOR_ORDER) &&
    values[1] !== actor.companyId
  )
    return unavailable("NATIVE_INVENTORY_LANE17_SCOPE_INVALID");
  if (
    (admitted === LANE17_RD_LANDED_COST_WORK_ROWS ||
      admitted === LANE17_RD_REORDER_POLICIES ||
      admitted === LANE17_RD_SERVICE_SPARES) &&
    values[0] !== actor.companyId
  )
    return unavailable("NATIVE_INVENTORY_LANE17_SCOPE_INVALID");

  const headers = new Headers({
    host: target.host,
    origin: target.origin,
    accept: "application/json",
    "content-type": "application/json",
    "cache-control": "no-cache",
    "x-erpx-context-company": actor.companyId,
  });
  const body = JSON.stringify({ kind: admitted, values });
  if (new TextEncoder().encode(body).length > 16 * 1024)
    return unavailable("NATIVE_INVENTORY_LANE17_INPUT_INVALID");
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/inventory/native-lane-17/read`,
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
    void response.body?.cancel().catch(() => undefined);
    const error = object(payload) && object(payload.error) ? payload.error : undefined;
    if (response.status >= 400 && response.status < 500)
      throw new ApiError(
        response.status,
        text(error?.code, 100) ? String(error!.code) : "NATIVE_INVENTORY_LANE17_READ_FAILED",
        text(error?.message, 300) ? String(error!.message) : "The native Inventory read was rejected.",
      );
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  return validateRows(payload) as unknown as T[];
}

/** Owner-local replenishment reads (reorder policies and service spares).
 * Shares the strict, WorkContext-selected transport; no SQL is authored here. */
export function readLane17ReplenishmentRows<T = Record<string, unknown>>(
  kind: typeof LANE17_RD_REORDER_POLICIES | typeof LANE17_RD_SERVICE_SPARES,
  values: readonly unknown[],
  actor: Lane17Actor,
  environment: Readonly<Record<string, string | undefined>> = process.env,
): Promise<T[]> {
  return readLane17Rows<T>(kind, values, actor, environment);
}

/** True when the native lane 17 read consumer switch is configured. */
export function nativeInventoryLane17Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
