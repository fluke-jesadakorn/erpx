import "server-only";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { nativePosEvidenceConfiguration } from "./client";

/**
 * Strict private transport for the native POS workspace day-close report list
 * (wave 3). The axum owner executes the byte-identical retired `readPosDayCloses1`
 * SQL with sqlx, owns the text projection and enforces Company/branch scope.
 * This bridge carries no SQL and has no fallback reader; every failure fails
 * closed with ApiError(503). The target Holding is selected only from the
 * caller's current WorkContext; an unknown pair fails closed with 409.
 */
type DayClosesActor = Readonly<{
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId?: string | null;
}>;
export type NativePosDayClosesRow = Readonly<Record<string, string | null>>;

const ENV = "ERPX_NATIVE_POS_LANE25_CONFIG";
const KEYS = [
  "id", "company_id", "branch_id", "business_date", "currency",
  "state", "prepared_by", "confirmed_by", "version", "snapshot_json",
];

const unavailable = (code = "NATIVE_POS_LANE25_DAY_CLOSES_UNAVAILABLE", status = 503): never => {
  throw new ApiError(status, code, "The native POS day-close service is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const id = (value: unknown, max = 240): value is string =>
  typeof value === "string" && value.length > 0 && value.length <= max
  && /^[A-Za-z0-9._:-]+$/.test(value);
const date = (value: unknown): value is string =>
  typeof value === "string" && /^\d{4}-\d{2}-\d{2}$/.test(value);

function validRow(value: unknown): value is NativePosDayClosesRow {
  if (!object(value)) return false;
  const names = Object.keys(value);
  if (names.length !== KEYS.length || !KEYS.every((key) => names.includes(key))) return false;
  if (!id(value.id) || !date(value.business_date) || !/^[A-Z]{3}$/.test(String(value.currency))) return false;
  return KEYS.every((key) => {
    const cell = value[key];
    return cell === null || typeof cell === "string";
  }) && typeof value.snapshot_json === "string";
}

/** Request the bounded, Company/branch-scoped day-close report list from the owner. */
export async function nativePosDayCloses(
  actor: DayClosesActor,
): Promise<NativePosDayClosesRow[]> {
  const config = nativePosEvidenceConfiguration(process.env[ENV]);
  if (
    !id(actor.holdingId) || !id(actor.holdingSlug, 128) || !id(actor.companyId)
    || (actor.branchId !== undefined && actor.branchId !== null && !id(actor.branchId))
  )
    return unavailable("NATIVE_POS_LANE25_DAY_CLOSES_REQUEST_INVALID", 422);

  const target = config.holdings.find(
    (holding) => holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
  );
  if (!target) return unavailable("WORK_CONTEXT_CHANGED", 409);

  const headers = new Headers({
    host: target.host,
    origin: target.origin,
    accept: "application/json",
    "cache-control": "no-cache",
    "x-erpx-context-company": actor.companyId,
  });
  if (actor.branchId) headers.set("x-erpx-workspace-branch", actor.branchId);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/pos/day-closes`,
    config.base,
  );
  let response: Response;
  try {
    response = await fetch(url, {
      method: "POST",
      headers: Object.fromEntries(headers),
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
        typeof error?.code === "string" && error.code.length <= 100 ? error.code : "NATIVE_POS_LANE25_DAY_CLOSES_READ_FAILED",
        typeof error?.message === "string" && error.message.length <= 300 ? error.message : "The native POS day-close read was rejected.",
      );
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  const rows = object(payload) ? payload.rows : undefined;
  if (!Array.isArray(rows) || !rows.every(validRow))
    return unavailable("NATIVE_POS_LANE25_DAY_CLOSES_RESPONSE_INVALID");
  return rows as NativePosDayClosesRow[];
}
