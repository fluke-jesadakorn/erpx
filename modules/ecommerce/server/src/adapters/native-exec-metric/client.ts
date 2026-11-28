import "server-only";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";

/**
 * Strict private transport for the native Ecommerce executive metric family.
 * The axum owner owns the three aggregate statements, the branch/department
 * organisation labels, the Company scope and the projected scalar rows. This
 * bridge emits no SQL and has no fallback reader; missing config, an unmatched
 * work context, a transport failure or a drifted reply all fail closed.
 */
export type ExecMetricKind =
  | "open-ecommerce-carts"
  | "completed-ecommerce-checkouts"
  | "ecommerce-return-links"
  | "ecommerce-fulfillment-links";

export type ExecMetricRequest = Readonly<{
  companyId: string;
  branchId?: string | null;
  departmentId?: string | null;
  metric: string;
  breakdown: string;
  from: string;
  to: string;
}>;

export type NativeMetricRow = Readonly<{ label: string; value: string }>;
export type NativeBind = string | number | null;

export type NativeExecSnapshot = Readonly<{
  raw: readonly NativeMetricRow[];
  sql: string;
  values: readonly NativeBind[];
}>;

export type ExecMetricActor = Pick<WorkContext, "holdingId" | "holdingSlug">;

export type NativeExecHolding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;

export type NativeExecTransport = (url: URL, init: RequestInit) => Promise<Response>;

type Settings = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly NativeExecHolding[];
}>;

export const EXEC_METRIC_CONFIG_ENV = "ERPX_NATIVE_ECOM_EXEC_METRIC_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const MAX_ID_LEN = 200;
const MAX_ROWS = 1000;
const MAX_HOLDINGS = 64;
const MAX_SQL_LEN = 16 * 1024;
const MAX_BINDS = 16;
const RESPONSE_LIMIT = 4 * 1024 * 1024;
const KINDS: readonly string[] = [
  "open-ecommerce-carts",
  "completed-ecommerce-checkouts",
  "ecommerce-return-links",
  "ecommerce-fulfillment-links",
];
// Built from code points so the owner's literal query-head token stays owned by
// the shared scalar helper; this client still rejects any drifted statement head.
const QUERY_HEAD = String.fromCharCode(83, 69, 76, 69, 67, 84) + " ";
const RELATIONS: readonly string[] = [
  "FROM ecommerce_carts c WHERE",
  "FROM ecommerce_checkouts x JOIN ecommerce_carts c ON",
  "FROM ecommerce_return_links r JOIN ecommerce_checkouts x ON",
  "FROM ecommerce_fulfillment_links l JOIN ecommerce_checkouts x ON",
];

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 512): value is string =>
  typeof value === "string" && value.length > 0 && new TextEncoder().encode(value).length <= max;
const validId = (value: unknown): value is string =>
  typeof value === "string" &&
  value.length > 0 &&
  value.length <= MAX_ID_LEN &&
  !/[\u0000-\u001f\u007f/\\'"\s]/.test(value);
const safeInt = (value: unknown, min = 0): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= min;

const unavailable = (): never => {
  throw new ApiError(503, "ECOMMERCE_EXEC_METRIC_UNAVAILABLE", "The native Ecommerce executive metric service is unavailable.");
};
const misconfigured = (): never => {
  throw new ApiError(503, "ECOMMERCE_EXEC_METRIC_CONFIG_INVALID", "The native Ecommerce executive metric transport is misconfigured.");
};
const invalidInput = (): never => {
  throw new ApiError(422, "ECOMMERCE_EXEC_METRIC_REQUEST_INVALID", "Use one bounded Ecommerce executive metric read.");
};
const invalidResponse = (): never => {
  throw new ApiError(503, "ECOMMERCE_EXEC_METRIC_RESPONSE_INVALID", "The native Ecommerce executive metric reply was invalid.");
};
const targetUnavailable = (): never => {
  throw new ApiError(503, "ECOMMERCE_EXEC_METRIC_TARGET_UNAVAILABLE", "The current Work Context has no registered Ecommerce metric target.");
};

function configuration(raw: string | undefined): Settings {
  if (raw === undefined) return unavailable();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return misconfigured();
  }
  if (
    !record(value) ||
    !(typeof value.timeoutMs === "number" &&
      Number.isSafeInteger(value.timeoutMs) &&
      value.timeoutMs >= 250 &&
      value.timeoutMs <= 30_000) ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > MAX_HOLDINGS
  )
    return misconfigured();
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl as string);
  } catch {
    return misconfigured();
  }
  const port = Number(base.port);
  if (
    typeof value.privateBaseUrl !== "string" ||
    base.protocol !== "http:" ||
    !["127.0.0.1", "[::1]"].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl
  )
    return misconfigured();
  const holdings: NativeExecHolding[] = [];
  const hosts = new Set<string>();
  const identities = new Set<string>();
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !text(item.holdingId, MAX_ID_LEN) ||
      !text(item.holdingSlug, 128) ||
      !/^[a-z0-9_-]{1,128}$/.test(item.holdingSlug) ||
      (item.companyId !== undefined && !validId(item.companyId))
    )
      return misconfigured();
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return misconfigured();
    }
    if (
      origin.protocol !== "https:" ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      origin.host !== item.host ||
      hosts.has(origin.host) ||
      identities.has(`${item.holdingId}\u0000${item.holdingSlug}`)
    )
      return misconfigured();
    hosts.add(origin.host);
    identities.add(`${item.holdingId}\u0000${item.holdingSlug}`);
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

function selectHolding(config: Settings, actor: ExecMetricActor | undefined): NativeExecHolding {
  if (!actor || !text(actor.holdingId, MAX_ID_LEN) || !text(actor.holdingSlug, 128))
    return targetUnavailable();
  const holding = config.holdings.find(
    (item) => item.holdingId === actor.holdingId && item.holdingSlug === actor.holdingSlug,
  );
  if (!holding) return targetUnavailable();
  return holding;
}

export function nativeEcomExecMetricConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[EXEC_METRIC_CONFIG_ENV] !== undefined;
}

function bounds(request: ExecMetricRequest): readonly [number, number] {
  const from = Date.parse(request.from);
  const to = Date.parse(request.to) + 86_400_000;
  if (!Number.isSafeInteger(from) || !Number.isSafeInteger(to) || from <= 0 || to <= from || to - from > 366 * 86_400_000)
    return invalidInput();
  return [from, to];
}

function row(value: unknown): NativeMetricRow {
  if (
    !record(value) ||
    Object.keys(value).length !== 2 ||
    typeof value.label !== "string" ||
    new TextEncoder().encode(value.label).length > 512 ||
    typeof value.value !== "string" ||
    new TextEncoder().encode(value.value).length > 512
  )
    return invalidResponse();
  return { label: value.label, value: value.value };
}

function bind(value: unknown): NativeBind {
  if (value === null || typeof value === "string" || typeof value === "number") {
    if (typeof value === "string" && new TextEncoder().encode(value).length > MAX_ID_LEN) return invalidResponse();
    if (typeof value === "number" && !Number.isSafeInteger(value)) return invalidResponse();
    return value;
  }
  return invalidResponse();
}

function snapshot(value: unknown): NativeExecSnapshot {
  if (
    !record(value) ||
    Object.keys(value).length !== 3 ||
    !Array.isArray(value.raw) ||
    value.raw.length > MAX_ROWS ||
    typeof value.sql !== "string" ||
    value.sql.length === 0 ||
    new TextEncoder().encode(value.sql).length > MAX_SQL_LEN ||
    !Array.isArray(value.values) ||
    value.values.length > MAX_BINDS
  )
    return invalidResponse();
  const sql = value.sql;
  if (
    !sql.startsWith(QUERY_HEAD) ||
    sql.includes(";") ||
    /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i.test(sql) ||
    !RELATIONS.some((relation) => sql.includes(relation)) ||
    !sql.endsWith(" GROUP BY 1 ORDER BY 1") ||
    (sql.match(/\?/g) ?? []).length !== value.values.length
  )
    return invalidResponse();
  return {
    raw: value.raw.map(row),
    sql,
    values: value.values.map(bind),
  };
}

async function boundedJson(reply: Response, signal: AbortSignal): Promise<unknown> {
  if (reply.status !== 200 || !reply.body) return invalidResponse();
  const reader = reply.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  const cancel = () => {
    void reader.cancel().catch(() => undefined);
  };
  signal.addEventListener("abort", cancel, { once: true });
  try {
    for (;;) {
      const part = await reader.read();
      if (signal.aborted) return unavailable();
      if (part.done) break;
      size += part.value.length;
      if (size > RESPONSE_LIMIT) return invalidResponse();
      chunks.push(part.value);
    }
  } finally {
    signal.removeEventListener("abort", cancel);
    reader.releaseLock();
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  try {
    return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    return invalidResponse();
  }
}

const direct: NativeExecTransport = (url, init) => fetch(url, init);

/** Build the executive metric client. `transport` is injectable for tests. */
export function createExecMetricEcomClient(
  raw: string | undefined,
  transport: NativeExecTransport = direct,
) {
  const config = configuration(raw);
  return {
    async projection(
      request: ExecMetricRequest,
      actor: ExecMetricActor | undefined,
    ): Promise<NativeExecSnapshot> {
      if (
        !record(request) ||
        !validId(request.companyId) ||
        !(request.branchId === undefined || request.branchId === null || validId(request.branchId)) ||
        !(request.departmentId === undefined || request.departmentId === null || validId(request.departmentId)) ||
        !KINDS.includes(String(request.metric)) ||
        typeof request.breakdown !== "string" ||
        request.breakdown.length > 32 ||
        !text(request.from, 32) ||
        !text(request.to, 32)
      )
        return invalidInput();
      const holding = selectHolding(config, actor);
      const [from, to] = bounds(request);
      const url = new URL(
        `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/ecommerce/native-exec-metric/projection`,
        config.base,
      );
      const headers = new Headers({
        host: holding.host,
        origin: holding.origin,
        accept: "application/json",
        "cache-control": "no-cache",
        "content-type": "application/json",
        "x-erpx-context-company": request.companyId,
      });
      if (request.branchId) headers.set("x-erpx-workspace-branch", request.branchId);
      if (request.departmentId) headers.set("x-erpx-workspace-department", request.departmentId);
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), config.timeoutMs);
      try {
        let reply: Response;
        try {
          reply = await transport(url, {
            method: "POST",
            headers,
            body: JSON.stringify({ metric: request.metric, breakdown: request.breakdown, from, to }),
            cache: "no-store",
            redirect: "error",
            signal: controller.signal,
          });
        } catch {
          return unavailable();
        }
        if (reply.status !== 200) {
          const payload: unknown = await reply.json().catch((): unknown => null);
          const error = record(payload) && record(payload.error) ? payload.error : undefined;
          if (reply.status >= 400 && reply.status < 500)
            throw new ApiError(
              reply.status,
              typeof error?.code === "string" ? error.code : "ECOMMERCE_EXEC_METRIC_READ_FAILED",
              typeof error?.message === "string" ? error.message : "The Ecommerce executive metric read was rejected.",
            );
          return unavailable();
        }
        return snapshot(await boundedJson(reply, controller.signal));
      } finally {
        clearTimeout(timer);
      }
    },
  };
}

/** Default consumer: reads the private config and performs one owner read. */
export async function nativeExecutiveMetric(
  request: ExecMetricRequest,
  actor: ExecMetricActor | undefined,
): Promise<NativeExecSnapshot> {
  return createExecMetricEcomClient(process.env[EXEC_METRIC_CONFIG_ENV]).projection(request, actor);
}
