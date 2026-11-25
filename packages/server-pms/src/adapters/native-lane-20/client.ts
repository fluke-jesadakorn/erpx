import "server-only";
import { Agent, request as httpRequest } from "node:http";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

/**
 * Strict transport for the native PMS property-readiness read. The axum owner
 * executes the SQL, enforces Company scope and computes the response; this
 * bridge carries no SQL, accepts no SQL from a caller and has no fallback
 * reader. A missing, malformed or failing native runtime fails closed.
 *
 * Every registered Holding has its own origin, Host, identity and database
 * pool. The target is selected from the caller's WorkContext; an unknown or
 * ambiguous target never falls back to a default Holding.
 */
export type PropertyReadiness = Readonly<{
  hasRoomTypes: boolean;
  hasActiveRooms: boolean;
  hasActiveRates: boolean;
  hasStays: boolean;
}>;

/** Bounded identity carried from the caller's WorkContext. */
export type LaneActor = Readonly<{
  holdingId?: string;
  holdingSlug?: string;
  origin?: string;
  companyId?: string;
}>;

type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Config = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;

/** Shared multi-Holding config/selection surface for native lane 20 clients. */
export type Lane20Config = Config;
export type Lane20Holding = Holding;
export function nativeLane20Config(): Lane20Config {
  return config();
}
export function nativeLane20SelectTarget(cfg: Lane20Config, actor: LaneActor): Lane20Holding {
  return selectTarget(cfg, actor);
}
export function nativeLane20Get(
  url: URL,
  headers: Record<string, string>,
  timeoutMs: number,
): Promise<{ status: number; value: unknown }> {
  return get(url, headers, timeoutMs);
}

const ENV = "ERPX_NATIVE_LANE_20_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const unavailable = (code = "PMS_NATIVE_LANE_20_UNAVAILABLE", message = "The native PMS property readiness service is unavailable."): never => {
  throw new ApiError(503, code, message);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const slug = (value: unknown): value is string =>
  typeof value === "string" && /^[a-z0-9_-]{1,128}$/.test(value);
const identifier = (value: unknown): value is string =>
  typeof value === "string" && value.length > 0 && value.length <= 200 &&
  // eslint-disable-next-line no-control-regex
  !/[\u0000-\u001f/?#\\]/.test(value);
const bool = (value: unknown): value is boolean => typeof value === "boolean";

function config(): Config {
  const raw = process.env[ENV];
  if (raw === undefined) return unavailable("PMS_NATIVE_LANE_20_CONFIG_REQUIRED");
  let value: Record<string, unknown>;
  try {
    value = JSON.parse(raw) as Record<string, unknown>;
  } catch {
    return unavailable("PMS_NATIVE_LANE_20_CONFIG_INVALID");
  }
  if (!record(value)) return unavailable("PMS_NATIVE_LANE_20_CONFIG_INVALID");
  let base: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
  } catch {
    return unavailable("PMS_NATIVE_LANE_20_CONFIG_INVALID");
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
    base.origin !== String(value.privateBaseUrl) ||
    typeof value.timeoutMs !== "number" ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable("PMS_NATIVE_LANE_20_CONFIG_INVALID");

  const origins = new Set<string>();
  const hosts = new Set<string>();
  const ids = new Set<string>();
  const slugs = new Set<string>();
  const holdings: Holding[] = [];
  for (const item of value.holdings as unknown[]) {
    if (!record(item)) return unavailable("PMS_NATIVE_LANE_20_CONFIG_INVALID");
    let origin: URL;
    try {
      origin = new URL(String(item.origin));
    } catch {
      return unavailable("PMS_NATIVE_LANE_20_CONFIG_INVALID");
    }
    if (
      origin.protocol !== "https:" ||
      origin.username ||
      origin.password ||
      origin.origin !== String(item.origin) ||
      typeof item.host !== "string" ||
      item.host !== origin.host ||
      !identifier(item.holdingId) ||
      !slug(item.holdingSlug) ||
      (item.companyId !== undefined && !identifier(item.companyId)) ||
      origins.has(origin.origin) ||
      hosts.has(item.host) ||
      ids.has(item.holdingId) ||
      slugs.has(item.holdingSlug)
    )
      return unavailable("PMS_NATIVE_LANE_20_CONFIG_INVALID");
    origins.add(origin.origin);
    hosts.add(item.host);
    ids.add(item.holdingId);
    slugs.add(item.holdingSlug);
    holdings.push({
      origin: origin.origin,
      host: item.host,
      holdingId: item.holdingId,
      holdingSlug: item.holdingSlug,
      ...(item.companyId === undefined ? {} : { companyId: item.companyId }),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Resolve exactly one registered Holding from the caller's WorkContext. */
function selectTarget(cfg: Config, actor: LaneActor): Holding {
  if (!record(actor)) return unavailable("PMS_NATIVE_LANE_20_TARGET_UNAVAILABLE");
  let matches: readonly Holding[];
  if (typeof actor.origin === "string" && actor.origin) {
    matches = cfg.holdings.filter(holding => holding.origin === actor.origin);
  } else if (identifier(actor.holdingId) && slug(actor.holdingSlug)) {
    matches = cfg.holdings.filter(
      holding => holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
    );
  } else if (identifier(actor.companyId)) {
    matches = cfg.holdings.filter(holding => holding.companyId === actor.companyId);
  } else {
    return unavailable("PMS_NATIVE_LANE_20_TARGET_UNAVAILABLE");
  }
  if (matches.length !== 1) return unavailable("PMS_NATIVE_LANE_20_TARGET_UNAVAILABLE");
  return matches[0]!;
}

/** GET with an explicit Host header; fetch forbids overriding Host. */
function get(url: URL, headers: Record<string, string>, timeoutMs: number): Promise<{ status: number; value: unknown }> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import("node:http").ProxyEnv });
    const outgoing = httpRequest(
      url,
      {
        method: "GET",
        agent,
        headers: { ...headers },
        signal: AbortSignal.timeout(timeoutMs),
        maxHeaderSize: 8192,
      },
      incoming => {
        const chunks: Buffer[] = [];
        incoming.on("data", chunk => chunks.push(Buffer.from(chunk)));
        incoming.on("end", () => {
          agent.destroy();
          const text = Buffer.concat(chunks).toString("utf8");
          let value: unknown = null;
          try {
            value = text ? JSON.parse(text) : null;
          } catch {
            value = null;
          }
          resolve({ status: incoming.statusCode ?? 502, value });
        });
        incoming.on("error", error => {
          agent.destroy();
          reject(error);
        });
      },
    );
    outgoing.on("error", error => {
      agent.destroy();
      reject(error);
    });
    outgoing.end();
  });
}

/**
 * Native-backed property readiness. `database` is retained for call-site
 * compatibility only; no SQL is issued on it. The native owner executes the
 * Company-scoped read and returns the complete bounded response shape.
 */
export async function propertyReadinessFacts(
  _database: SqlDatabase,
  actor: LaneActor,
  propertyId: string,
): Promise<PropertyReadiness | null> {
  if (typeof propertyId !== "string" || !propertyId || propertyId.length > 200)
    return unavailable("PMS_NATIVE_LANE_20_REQUEST_INVALID");
  const cfg = config();
  const target = selectTarget(cfg, actor);
  const companyId = actor.companyId ?? target.companyId;
  if (!identifier(companyId)) return unavailable("PMS_NATIVE_LANE_20_REQUEST_INVALID");
  let response: { status: number; value: unknown };
  try {
    response = await get(
      new URL(
        `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/pms/native-lane-20/property-readiness`,
        cfg.base,
      ),
      {
        host: target.host,
        origin: target.origin,
        accept: "application/json",
        "x-erpx-context-company": companyId,
        "x-erpx-context-property": propertyId,
      },
      cfg.timeoutMs,
    );
  } catch {
    return unavailable();
  }
  const value = response.value;
  if (response.status !== 200) {
    if (response.status >= 500) return unavailable();
    const error = record(value) && record(value.error) ? value.error : {};
    throw new ApiError(
      response.status,
      typeof error.code === "string" ? error.code : "PMS_NATIVE_LANE_20_FAILED",
      typeof error.message === "string" ? error.message : "The native PMS property readiness read was rejected.",
    );
  }
  if (!record(value) || !("facts" in value)) return unavailable("PMS_NATIVE_LANE_20_RESPONSE_INVALID");
  const facts = value.facts;
  if (facts === null || facts === undefined) return null;
  if (
    !record(facts) ||
    !bool(facts.hasRoomTypes) ||
    !bool(facts.hasActiveRooms) ||
    !bool(facts.hasActiveRates) ||
    !bool(facts.hasStays) ||
    Object.keys(facts).length !== 4
  )
    return unavailable("PMS_NATIVE_LANE_20_RESPONSE_INVALID");
  return {
    hasRoomTypes: facts.hasRoomTypes,
    hasActiveRooms: facts.hasActiveRooms,
    hasActiveRates: facts.hasActiveRates,
    hasStays: facts.hasStays,
  };
}

/** True when the owner consumer switch is configured. */
export function nativeLane20Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
