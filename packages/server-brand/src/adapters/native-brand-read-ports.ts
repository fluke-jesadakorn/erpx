import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

/**
 * Strict private transport for the owner-local Brand read ports. The axum owner
 * plans the exact statement for a registered read kind; this bridge emits no
 * SQL, accepts no SQL from a caller and executes the returned statement on the
 * caller's already-open unit. Missing configuration or any configured failure
 * fails closed with no TypeScript fallback.
 */
type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
}>;
type Config = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;
type Flags = Readonly<{ range?: boolean; query?: boolean; cursor?: boolean }>;
type Plan = Readonly<{ sql: string; values: unknown[] }>;

const ENV = "ERPX_NATIVE_BRAND_READ_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

/** Registered read kinds. These mirror the Rust owner allow-list in
 * `native_read_sql.rs`; the wire accepts no other discriminator. */
export const BRAND_RD_METRIC_PROFILES_LABEL = "brand_rd_metric_profiles_label";
export const BRAND_RD_METRIC_PROFILES_BRANCH = "brand_rd_metric_profiles_branch";
export const BRAND_RD_METRIC_UPDATES_MONTH = "brand_rd_metric_updates_month";
export const BRAND_RD_METRIC_UPDATES_BRANCH = "brand_rd_metric_updates_branch";
export const BRAND_RD_EVIDENCE_SEARCH = "brand_rd_evidence_search";
export const BRAND_RD_MANAGEMENT_BRANCH_EXISTS = "brand_rd_management_branch_exists";
export const BRAND_RD_MANAGEMENT_COMPANY_EXISTS = "brand_rd_management_company_exists";
export const BRAND_RD_WORKSPACE_CHOICE = "brand_rd_workspace_choice";

const unavailable = (code = "NATIVE_BRAND_READ_UNAVAILABLE", status = 503): never => {
  throw new ApiError(status, code, "The native Brand read service is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === "string" && new TextEncoder().encode(value).length <= max;

function configuration(): Config {
  const raw = process.env[ENV];
  if (!raw) return unavailable("NATIVE_BRAND_READ_CONFIGURATION_REQUIRED");
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable("NATIVE_BRAND_READ_CONFIGURATION_INVALID");
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
    return unavailable("NATIVE_BRAND_READ_CONFIGURATION_INVALID");
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable("NATIVE_BRAND_READ_CONFIGURATION_INVALID");
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
    return unavailable("NATIVE_BRAND_READ_CONFIGURATION_INVALID");
  const holdings: Holding[] = [];
  for (const item of value.holdings) {
    if (
      !object(item) ||
      !text(item.origin, 512) ||
      !text(item.holdingId, 200) ||
      !text(item.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(String(item.holdingSlug))
    )
      return unavailable("NATIVE_BRAND_READ_CONFIGURATION_INVALID");
    let origin: URL;
    try {
      origin = new URL(String(item.origin));
    } catch {
      return unavailable("NATIVE_BRAND_READ_CONFIGURATION_INVALID");
    }
    if (origin.protocol !== "https:" || origin.origin !== item.origin || origin.username || origin.password)
      return unavailable("NATIVE_BRAND_READ_CONFIGURATION_INVALID");
    holdings.push({
      origin: origin.origin,
      host: origin.host,
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Plan one registered Brand read on the native owner and validate the exact
 * statement envelope before the caller runs it on its own unit. */
export async function planBrandRead(
  kind: string,
  values: readonly unknown[],
  flags: Flags = {},
  actor?: Pick<WorkContext, "holdingId" | "holdingSlug" | "companyId" | "assignmentId" | "scope">,
): Promise<Plan> {
  const config = configuration();
  let target: Holding | undefined;
  if (actor) {
    target = config.holdings.find(
      (holding) =>
        holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
    );
    if (!target) return unavailable("WORK_CONTEXT_CHANGED", 409);
  } else {
    target = config.holdings[0];
  }
  if (!target) return unavailable("NATIVE_BRAND_READ_TARGET_UNAVAILABLE");

  const requestHeaders = new Headers({
    host: target.host,
    origin: target.origin,
    accept: "application/json",
    "content-type": "application/json",
    "cache-control": "no-cache",
  });
  if (actor) {
    requestHeaders.set("x-erpx-context-company", actor.companyId);
    requestHeaders.set("x-erpx-context-assignment", actor.assignmentId);
    if (actor.scope.kind !== "holding" && actor.scope.branchSlug)
      requestHeaders.set("x-erpx-workspace-branch", actor.scope.branchSlug);
    if (actor.scope.kind === "department" && actor.scope.departmentSlug)
      requestHeaders.set("x-erpx-workspace-department", actor.scope.departmentSlug);
  }
  const body = JSON.stringify({
    kind,
    values,
    range: flags.range === true,
    query: flags.query === true,
    cursor: flags.cursor === true,
  });
  if (new TextEncoder().encode(body).length > 16 * 1024) return unavailable("BRAND_READ_INPUT_INVALID", 422);

  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/branding/read-port`,
    config.base,
  );
  let response: Response;
  try {
    response = await fetch(url, {
      method: "POST",
      headers: Object.fromEntries(requestHeaders),
      body,
      cache: "no-store",
      redirect: "error",
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    return unavailable();
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    const payload = await response.json().catch((): unknown => null);
    const error = object(payload) && object(payload.error) ? payload.error : undefined;
    if (response.status >= 400 && response.status < 500)
      throw new ApiError(
        response.status,
        text(error?.code, 100) ? String(error!.code) : "NATIVE_BRAND_READ_FAILED",
        text(error?.message, 300) ? String(error!.message) : "The native Brand read was rejected.",
      );
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  const data = object(payload) ? payload.data : undefined;
  if (
    !object(data) ||
    !text(data.sql, 1_048_576) ||
    !/^SELECT\s/i.test(data.sql) ||
    !Array.isArray(data.values)
  )
    return unavailable("NATIVE_BRAND_READ_RESPONSE_INVALID", 502);
  return { sql: data.sql, values: data.values as unknown[] };
}

/** True when the native Brand read consumer switch is configured. */
export function nativeBrandReadConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
