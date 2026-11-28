import "server-only";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {
  configuration,
  selectTarget,
  type Lane9Actor,
} from "./client";

/**
 * Strict private transport for the lane-9 work-review person-route read. The
 * axum owner executes the Company-scoped statement with sqlx and owns the
 * projection and response shape; this bridge emits no SQL and accepts no SQL
 * from a caller. Every registered Holding is selected explicitly (WorkContext
 * `(holdingId, holdingSlug)`, otherwise an exact configured `companyId`); an
 * unknown target fails closed with no TS fallback.
 */
const ENV = "ERPX_NATIVE_PEOPLE_PAYROLL_LANE9_CONFIG";
const MAX_USERS = 200;
const MAX_ROWS = 1_000;

/** SHA-256 of the owner Rust `SQL_ROUTE_PEOPLE` constant. */
export const SQL_ROUTE_ORACLE_SHA256 =
  "0acbcc7c016d3d0aa05fc555575bc458ad4af3759fb51fc7fb677e841df7a589";
/** SHA-256 of the pinned non-empty native response payload. */
export const RESPONSE_ORACLE_SHA256 =
  "07600369a17612953f9810e031254acd20df6597dc6246ccdfe405767fed1dc2";

export type RouteTransport = (
  url: URL,
  headers: Headers,
  init: { body: string; signal: AbortSignal },
) => Promise<Response>;

const fail = (code: string, status = 503): never => {
  throw new ApiError(status, code, "The native People Payroll person-route service is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const validId = (value: unknown): value is string =>
  typeof value === "string" && value.length > 0 && value.length <= 200 &&
  /^[A-Za-z0-9._:-]+$/.test(value);
const bounded = (value: unknown): value is string => validId(value);

function exactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  return Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
}

export async function directRoute(
  url: URL,
  headers: Headers,
  init: { body: string; signal: AbortSignal },
): Promise<Response> {
  return fetch(url, {
    method: "POST",
    headers,
    body: init.body,
    cache: "no-store",
    redirect: "error",
    signal: init.signal,
  });
}

export type RoutePerson = Readonly<{ user_id: string; legal_name: string }>;
export type RouteResult = Readonly<{ results: readonly RoutePerson[] }>;

/**
 * Native replacement of `routePeople` in `adapters/work-review-facts.ts`.
 * `database` is retained only for call-site compatibility; no statement is
 * issued on it. Empty id lists return no rows without issuing a request; the
 * original consumer already short-circuits them.
 */
export async function routePeople(
  _database: SqlDatabase,
  companyId: string,
  userIds: readonly string[],
  actor?: Lane9Actor,
  transport: RouteTransport = directRoute,
): Promise<RouteResult> {
  if (!validId(companyId)) return fail("PEOPLE_PAYROLL_LANE9_ROUTE_COMPANY_INVALID", 422);
  if (userIds.length === 0) return { results: [] };
  if (userIds.length > MAX_USERS || !userIds.every(validId)) {
    return fail("PEOPLE_PAYROLL_LANE9_ROUTE_USERS_INVALID", 422);
  }
  const config = configuration(process.env[ENV]);
  const target = selectTarget(config, actor, companyId);
  const headers = new Headers({
    host: target.host,
    origin: target.origin,
    accept: "application/json",
    "cache-control": "no-cache",
    "content-type": "application/json",
    "x-erpx-context-holding": target.holdingId,
    "x-erpx-context-company": companyId,
  });
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/people-payroll/route-people`,
    config.base,
  );
  const body = JSON.stringify({ users: [...userIds] });
  let response: Response;
  try {
    response = await transport(url, headers, {
      body,
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_ROUTE_UNAVAILABLE");
  }
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_ROUTE_REDIRECT");
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    if (response.status >= 400 && response.status < 500) {
      return fail("NATIVE_PEOPLE_PAYROLL_LANE9_ROUTE_REJECTED", response.status);
    }
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_ROUTE_UNAVAILABLE");
  }
  const payload: unknown = await response.json().catch(() => null);
  if (!object(payload) || Object.keys(payload).length !== 1 || !Array.isArray(payload.results)) {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_ROUTE_RESPONSE_INVALID", 502);
  }
  if (payload.results.length > MAX_ROWS) {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_ROUTE_RESPONSE_INVALID", 502);
  }
  const results = payload.results.map((item: unknown) => {
    if (!object(item) || !exactKeys(item, ["user_id", "legal_name"])) {
      return fail("NATIVE_PEOPLE_PAYROLL_LANE9_ROUTE_RESPONSE_INVALID", 502);
    }
    if (!bounded(item.user_id) || typeof item.legal_name !== "string" ||
        item.legal_name.length === 0 || item.legal_name.length > 1_048_576) {
      return fail("NATIVE_PEOPLE_PAYROLL_LANE9_ROUTE_RESPONSE_INVALID", 502);
    }
    return { user_id: item.user_id, legal_name: item.legal_name };
  });
  return { results };
}
