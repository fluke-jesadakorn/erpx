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
 * Strict transport for the native PMS cutover-import batch list. The axum
 * owner executes the Company/Property-scoped bounded read and projects the
 * row shape. No SQL, no fallback, and unknown Holding targets fail closed.
 */
export type ImportBatchRow = Readonly<{
  id: string;
  cutover_date: string;
  state: string;
  version: number;
  created_by: string;
  approved_by: string | null;
  opening_journal_id: string | null;
  report_json: string;
  created_at: number;
}>;

const unavailable = (code = "PMS_NATIVE_LANE_20_UNAVAILABLE", message = "The native PMS import batch service is unavailable."): never => {
  throw new ApiError(503, code, message);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === "string" && value.length <= max;
const nullableText = (value: unknown, max = 200): value is string | null =>
  value === null || text(value, max);

const KEYS = [
  "id",
  "cutover_date",
  "state",
  "version",
  "created_by",
  "approved_by",
  "opening_journal_id",
  "report_json",
  "created_at",
] as const;

function row(value: unknown): ImportBatchRow {
  if (
    !record(value) ||
    Object.keys(value).length !== KEYS.length ||
    !text(value.id, 200) ||
    !text(value.cutover_date, 200) ||
    !text(value.state, 40) ||
    typeof value.version !== "number" ||
    !Number.isSafeInteger(value.version) ||
    !text(value.created_by, 200) ||
    !nullableText(value.approved_by) ||
    !nullableText(value.opening_journal_id) ||
    !text(value.report_json) ||
    typeof value.created_at !== "number" ||
    !Number.isSafeInteger(value.created_at)
  )
    return unavailable("PMS_NATIVE_LANE_20_RESPONSE_INVALID");
  return {
    id: value.id,
    cutover_date: value.cutover_date,
    state: value.state,
    version: value.version,
    created_by: value.created_by,
    approved_by: value.approved_by,
    opening_journal_id: value.opening_journal_id,
    report_json: value.report_json,
    created_at: value.created_at,
  };
}

/**
 * Native-backed import batch list. `database` is retained for call-site
 * compatibility only; no SQL is issued on it.
 */
export async function importBatches(
  _database: SqlDatabase,
  actor: LaneActor,
  companyId: string,
  propertyId: string,
): Promise<readonly ImportBatchRow[] | null> {
  if (
    typeof companyId !== "string" ||
    !companyId ||
    companyId.length > 200 ||
    typeof propertyId !== "string" ||
    !propertyId ||
    propertyId.length > 200
  )
    return unavailable("PMS_NATIVE_LANE_20_REQUEST_INVALID");
  const cfg = nativeLane20Config();
  const target = nativeLane20SelectTarget(cfg, actor);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/pms/native-lane-20/import-batches`,
    cfg.base,
  );
  url.searchParams.set("property", propertyId);

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
      typeof error.message === "string" ? error.message : "The native PMS import batch read was rejected.",
    );
  }
  if (!record(value) || !("batches" in value))
    return unavailable("PMS_NATIVE_LANE_20_RESPONSE_INVALID");
  const batches = value.batches;
  if (batches === null || batches === undefined) return null;
  if (!Array.isArray(batches) || batches.length > 30)
    return unavailable("PMS_NATIVE_LANE_20_RESPONSE_INVALID");
  return batches.map(row);
}
