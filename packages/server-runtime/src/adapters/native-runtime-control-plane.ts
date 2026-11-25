import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";

/**
 * Strict private transport for the native Runtime control-plane read port.
 *
 * The axum owner plans the exact statement for a registered read kind; this
 * bridge emits no SQL, accepts no SQL from a caller and executes no fallback.
 * Missing configuration, a transport failure, a wrong arity or an incomplete
 * reply fails closed with ApiError(503, ...).
 */
export const RD_MIGRATION_RECEIPTS = "runtime_rd_migration_receipts";
export const RD_MIGRATION_CHECKSUM = "runtime_rd_migration_checksum";
export const RD_RECEIPT_TABLE_PRESENT = "runtime_rd_receipt_table_present";
export const RD_COVERAGE_PROCEDURE_PRESENT = "runtime_rd_coverage_procedure_present";
export const RD_COVERAGE_REFRESH = "runtime_rd_coverage_refresh";
export const RD_RUNTIME_HEALTH = "runtime_rd_runtime_health";
export const RD_UTC_DATE = "runtime_rd_utc_date";
export const RD_OBJECT_LOCK = "runtime_rd_object_lock";
export const RD_OBJECT_UNLOCK = "runtime_rd_object_unlock";
export const RD_OBJECT_XACT_LOCK = "runtime_rd_object_xact_lock";
export const RD_PAIR_LOCK = "runtime_rd_pair_lock";
export const RD_PAIR_UNLOCK = "runtime_rd_pair_unlock";
export const RD_MIGRATION_LOCK = "runtime_rd_migration_lock";
export const RD_MIGRATION_UNLOCK = "runtime_rd_migration_unlock";
export const RD_LOCK_TIMEOUT_SET = "runtime_rd_lock_timeout_set";
export const RD_LOCK_TIMEOUT_RESTORE = "runtime_rd_lock_timeout_restore";

export type ControlPlanePlan = Readonly<{ sql: string; values: readonly unknown[] }>;
type Holding = Readonly<{ origin: string; host: string; holdingSlug: string; holdingId: string }>;
type Config = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;

const ENV = "ERPX_NATIVE_RUNTIME_CONTROL_PLANE_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const unavailable = (code = "NATIVE_RUNTIME_CONTROL_PLANE_UNAVAILABLE"): never => {
  throw new ApiError(503, code, "The native Runtime control-plane service is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === "string" && new TextEncoder().encode(value).length <= max;

/** Parse and bound the private native configuration; no network here. */
export function parseControlPlaneConfig(raw: string | undefined): Config {
  if (raw === undefined || raw === "") return unavailable("NATIVE_RUNTIME_CONTROL_PLANE_CONFIG_REQUIRED");
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable("NATIVE_RUNTIME_CONTROL_PLANE_CONFIG_INVALID");
  }
  if (
    !object(value) ||
    !text(value.privateBaseUrl, 512) ||
    !Number.isSafeInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable("NATIVE_RUNTIME_CONTROL_PLANE_CONFIG_INVALID");
  let base: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
  } catch {
    return unavailable("NATIVE_RUNTIME_CONTROL_PLANE_CONFIG_INVALID");
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
    return unavailable("NATIVE_RUNTIME_CONTROL_PLANE_CONFIG_INVALID");
  const holdings: Holding[] = [];
  for (const item of value.holdings) {
    if (
      !object(item) ||
      !text(item.origin, 512) ||
      !text(item.holdingId, 200) ||
      !text(item.holdingSlug, 128) ||
      !/^[a-z0-9_-]{1,128}$/.test(String(item.holdingSlug))
    )
      return unavailable("NATIVE_RUNTIME_CONTROL_PLANE_CONFIG_INVALID");
    let origin: URL;
    try {
      origin = new URL(String(item.origin));
    } catch {
      return unavailable("NATIVE_RUNTIME_CONTROL_PLANE_CONFIG_INVALID");
    }
    if (
      origin.protocol !== "https:" ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password
    )
      return unavailable("NATIVE_RUNTIME_CONTROL_PLANE_CONFIG_INVALID");
    holdings.push({
      origin: origin.origin,
      host: origin.host,
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs as number, holdings };
}

/** Plan one registered control-plane read. The SQL is owned by the axum port. */
export async function planRuntimeControlPlane(
  kind: string,
  values: readonly unknown[] = [],
  environment: Readonly<Record<string, string | undefined>> = process.env,
): Promise<ControlPlanePlan> {
  const cfg = parseControlPlaneConfig(environment[ENV]);
  if (
    ![
      RD_MIGRATION_RECEIPTS,
      RD_MIGRATION_CHECKSUM,
      RD_RECEIPT_TABLE_PRESENT,
      RD_COVERAGE_PROCEDURE_PRESENT,
      RD_COVERAGE_REFRESH,
      RD_RUNTIME_HEALTH,
      RD_UTC_DATE,
      RD_OBJECT_LOCK,
      RD_OBJECT_UNLOCK,
      RD_OBJECT_XACT_LOCK,
      RD_PAIR_LOCK,
      RD_PAIR_UNLOCK,
      RD_MIGRATION_LOCK,
      RD_MIGRATION_UNLOCK,
      RD_LOCK_TIMEOUT_SET,
      RD_LOCK_TIMEOUT_RESTORE,
    ].includes(kind) ||
    values.length > 8
  )
    return unavailable("RUNTIME_CONTROL_PLANE_READ_INVALID");
  const target = cfg.holdings[0]!;
  const body = JSON.stringify({ kind, values });
  if (new TextEncoder().encode(body).length > 16 * 1024)
    return unavailable("RUNTIME_CONTROL_PLANE_READ_INVALID");
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/runtime/control-plane-read-port`,
    cfg.base,
  );
  let response: Response;
  try {
    response = await fetch(url, {
      method: "POST",
      headers: {
        host: target.host,
        origin: target.origin,
        accept: "application/json",
        "content-type": "application/json",
        "cache-control": "no-cache",
      },
      body,
      cache: "no-store",
      redirect: "error",
      signal: AbortSignal.timeout(cfg.timeoutMs),
    });
  } catch {
    return unavailable();
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    return unavailable("RUNTIME_CONTROL_PLANE_READ_FAILED");
  }
  const payload: unknown = await response.json().catch(() => null);
  const data = object(payload) ? payload.data : undefined;
  if (
    !object(data) ||
    !text(data.sql, 1_048_576) ||
    !/^SELECT\s/i.test(data.sql) ||
    !Array.isArray(data.values) ||
    data.values.length !== values.length
  )
    return unavailable("NATIVE_RUNTIME_CONTROL_PLANE_RESPONSE_INVALID");
  return { sql: data.sql, values: data.values };
}

/** True when the native control-plane read transport is configured. */
export function controlPlaneReadConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
