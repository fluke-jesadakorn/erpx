import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

/**
 * Strict private transport for the lane 37 Commercial seat-history owner read.
 * The axum owner executes all four retired TypeScript SELECTs with sqlx,
 * projects the typed rows and derives the Holding seat meter. This bridge emits
 * no SQL, accepts no SQL from a caller and never falls back to a local reader:
 * a missing/malformed configuration, transport failure, non-200 response or
 * malformed payload fails closed with an explicit 503.
 */
export type SeatInterval = [number, number];
export type HoldingSeatMeter = {
  coverage: "measured" | "unavailable";
  users: { userId: string; intervals: SeatInterval[] }[];
};
export type SeatHistoryRows = {
  historyStartedAt: number | null;
  assignments: {
    readonly eventId: number;
    readonly assignmentId: string;
    readonly capturedAt: number;
    readonly present: boolean;
    readonly stateJson: string | null;
  }[];
  members: {
    readonly eventId: number;
    readonly companyId: string;
    readonly userId: string;
    readonly capturedAt: number;
    readonly present: boolean;
    readonly stateJson: string | null;
  }[];
  roles: {
    readonly eventId: number;
    readonly companyId: string;
    readonly roleId: string;
    readonly capturedAt: number;
    readonly present: boolean;
    readonly stateJson: string | null;
  }[];
};
export type NativeSeatHistory = { rows: SeatHistoryRows; meter: HoldingSeatMeter };

type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Config = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;

/** The caller's trusted Holding identity. A target is never inferred from a
 * bare slug or from the first configured Holding: both ids must match. */
export type Lane37Actor = Pick<WorkContext, "holdingId" | "holdingSlug"> &
  Partial<Pick<WorkContext, "companyId">>;

const ENV = "ERPX_NATIVE_LANE37_COMMERCIAL_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const unavailable = (code = "LANE37_READ_UNAVAILABLE"): never => {
  throw new ApiError(503, code, "The native Commercial seat-history read is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === "string" && new TextEncoder().encode(value).length <= max;
const integer = (value: unknown, max = 9_007_199_254_740_991): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0 && value <= max;
const stateJson = (value: unknown): value is string | null => value === null || text(value, 65536);
const slug = (value: unknown): value is string => text(value, 128) && /^[a-z0-9_-]+$/.test(value as string);

function configuration(): Config {
  const raw = process.env[ENV];
  if (!raw) return unavailable("LANE37_CONFIGURATION_REQUIRED");
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable("LANE37_CONFIGURATION_INVALID");
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
    return unavailable("LANE37_CONFIGURATION_INVALID");
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable("LANE37_CONFIGURATION_INVALID");
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
    return unavailable("LANE37_CONFIGURATION_INVALID");
  const holdings: Holding[] = [];
  for (const item of value.holdings) {
    if (
      !object(item) ||
      !text(item.origin, 512) ||
      !text(item.holdingId, 200) ||
      item.holdingId.length === 0 ||
      !slug(item.holdingSlug) ||
      !(item.companyId === undefined || text(item.companyId, 200))
    )
      return unavailable("LANE37_CONFIGURATION_INVALID");
    let origin: URL;
    try {
      origin = new URL(String(item.origin));
    } catch {
      return unavailable("LANE37_CONFIGURATION_INVALID");
    }
    if (origin.protocol !== "https:" || origin.origin !== item.origin || origin.username || origin.password)
      return unavailable("LANE37_CONFIGURATION_INVALID");
    holdings.push({
      origin: origin.origin,
      host: origin.host,
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
      ...(item.companyId === undefined ? {} : { companyId: String(item.companyId) }),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

function event(value: unknown): value is { eventId: number; capturedAt: number; present: boolean; stateJson: string | null } {
  return (
    object(value) &&
    integer(value.eventId) &&
    integer(value.capturedAt) &&
    typeof value.present === "boolean" &&
    stateJson(value.stateJson)
  );
}

function parseRows(value: unknown): value is SeatHistoryRows {
  if (
    !object(value) ||
    !(value.historyStartedAt === null || integer(value.historyStartedAt)) ||
    !Array.isArray(value.assignments) ||
    !Array.isArray(value.members) ||
    !Array.isArray(value.roles)
  )
    return false;
  if (
    !value.assignments.every(
      (row) =>
        object(row) &&
        text(row.assignmentId, 200) &&
        event(row),
    ) ||
    !value.members.every(
      (row) =>
        object(row) &&
        text(row.companyId, 200) &&
        text(row.userId, 200) &&
        event(row),
    ) ||
    !value.roles.every(
      (row) =>
        object(row) &&
        text(row.companyId, 200) &&
        text(row.roleId, 200) &&
        event(row),
    )
  )
    return false;
  return true;
}

function parseMeter(value: unknown): value is HoldingSeatMeter {
  if (!object(value) || !["measured", "unavailable"].includes(String(value.coverage)) || !Array.isArray(value.users))
    return false;
  return value.users.every(
    (user) =>
      object(user) &&
      text(user.userId, 200) &&
      Array.isArray(user.intervals) &&
      user.intervals.every(
        (interval) => Array.isArray(interval) && interval.length === 2 && interval.every((point) => integer(point)),
      ),
  );
}

/** Read the owner-owned seat history and derived Holding meter. No SQL is
 * emitted, accepted or executed here and there is no local fallback. */
export async function readNativeSeatHistory(
  actor: Lane37Actor,
  start: number,
  end: number,
  hasCutoffDependency: boolean,
): Promise<NativeSeatHistory> {
  if (
    !object(actor) ||
    !text(actor.holdingId, 200) ||
    actor.holdingId.length === 0 ||
    !slug(actor.holdingSlug) ||
    !(actor.companyId === undefined || text(actor.companyId, 200)) ||
    !integer(start) ||
    !integer(end) ||
    start >= end ||
    typeof hasCutoffDependency !== "boolean"
  )
    return unavailable("LANE37_INPUT_INVALID");
  const cfg = configuration();
  const target = cfg.holdings.find(
    (holding) =>
      holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
  );
  if (!target) return unavailable("LANE37_TARGET_UNAVAILABLE");
  const companyId = actor.companyId ?? target.companyId;
  if (companyId !== undefined && !text(companyId, 200)) return unavailable("LANE37_INPUT_INVALID");

  let response: Response;
  try {
    const url = new URL(
      `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/commercial/seat-history?start=${start}&end=${end}`,
      cfg.base,
    );
    response = await fetch(url, {
      method: "GET",
      headers: new Headers({
        host: target.host,
        origin: target.origin,
        accept: "application/json",
        "cache-control": "no-cache",
        "x-erpx-context-holding": target.holdingId,
        ...(companyId === undefined ? {} : { "x-erpx-context-company": companyId }),
        "x-erpx-lane37-cutoff-dependency": hasCutoffDependency ? "true" : "false",
      }),
      cache: "no-store",
      redirect: "error",
      signal: AbortSignal.timeout(cfg.timeoutMs),
    });
  } catch {
    return unavailable();
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    return unavailable();
  }
  const payload: unknown = await response.json().catch((): unknown => null);
  if (
    !object(payload) ||
    !parseRows(payload.rows) ||
    !parseMeter(payload.meter)
  )
    return unavailable("LANE37_RESPONSE_INVALID");
  return { rows: payload.rows, meter: payload.meter };
}

/** True when the native lane 37 consumer switch is configured. */
export function nativeLane37Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
