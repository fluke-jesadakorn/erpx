import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {
  nativeLane20Config,
  nativeLane20Get,
  nativeLane20SelectTarget,
  type LaneActor,
} from "./client";

/**
 * Strict transport for the native PMS room-summary read. The axum owner
 * executes the Company-scoped aggregate and returns the complete bounded
 * response shape. This bridge emits no SQL and has no fallback reader.
 */
export type RoomSummary = Readonly<{
  total: number;
  active: number;
  dirty: number;
  clean: number;
  inspected: number;
  dnd: number;
  occupied: number;
}>;

export type RoomSummaryFilters = Readonly<{
  propertyId: string;
  status: string;
  q: string;
  building: string;
  floor: string;
  typeId: string;
}>;

const unavailable = (code = "PMS_NATIVE_LANE_20_UNAVAILABLE", message = "The native PMS room summary service is unavailable."): never => {
  throw new ApiError(503, code, message);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 200): value is string =>
  typeof value === "string" && value.length <= max;
const count = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0;

const KEYS = ["total", "active", "dirty", "clean", "inspected", "dnd", "occupied"] as const;

/**
 * Native-backed room summary. `database` is retained for call-site
 * compatibility only; no SQL is issued on it.
 */
export async function roomSummaryFacts(
  _database: SqlDatabase,
  actor: LaneActor,
  filters: RoomSummaryFilters,
): Promise<RoomSummary | null> {
  if (
    !text(filters.propertyId, 200) ||
    !filters.propertyId ||
    !text(filters.status) ||
    !text(filters.q) ||
    !text(filters.building) ||
    !text(filters.floor) ||
    !text(filters.typeId)
  )
    return unavailable("PMS_NATIVE_LANE_20_REQUEST_INVALID");
  const cfg = nativeLane20Config();
  const target = nativeLane20SelectTarget(cfg, actor);
  const companyId = actor.companyId ?? target.companyId;
  if (!text(companyId, 200) || !companyId)
    return unavailable("PMS_NATIVE_LANE_20_REQUEST_INVALID");

  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/pms/native-lane-20/room-summary`,
    cfg.base,
  );
  for (const [key, value] of [
    ["property", filters.propertyId],
    ["status", filters.status],
    ["q", filters.q],
    ["building", filters.building],
    ["floor", filters.floor],
    ["type", filters.typeId],
  ] as const)
    url.searchParams.set(key, value);

  let response: { status: number; value: unknown };
  try {
    response = await nativeLane20Get(
      url,
      {
        host: target.host,
        origin: target.origin,
        accept: "application/json",
        "x-erpx-context-company": companyId,
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
      typeof error.message === "string" ? error.message : "The native PMS room summary was rejected.",
    );
  }
  if (!record(value) || !("summary" in value))
    return unavailable("PMS_NATIVE_LANE_20_RESPONSE_INVALID");
  const summary = value.summary;
  if (summary === null || summary === undefined) return null;
  if (
    !record(summary) ||
    Object.keys(summary).length !== KEYS.length ||
    !KEYS.every(key => count(summary[key]))
  )
    return unavailable("PMS_NATIVE_LANE_20_RESPONSE_INVALID");
  return {
    total: summary.total as number,
    active: summary.active as number,
    dirty: summary.dirty as number,
    clean: summary.clean as number,
    inspected: summary.inspected as number,
    dnd: summary.dnd as number,
    occupied: summary.occupied as number,
  };
}
