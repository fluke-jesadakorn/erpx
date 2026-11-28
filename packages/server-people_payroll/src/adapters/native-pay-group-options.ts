import "server-only";
import { nativeRequest } from "@erpx/server-runtime/public/native-http";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {
  configuration,
  selectTarget,
  type Lane9Actor,
  type Transport,
} from "./native-lane-9/client";

/**
 * Strict private transport for the native active pay-group options read. The
 * axum owner executes the Company-scoped statement with sqlx and owns the
 * projection, field order and response shape; this bridge emits no SQL and
 * accepts no SQL from a caller. Every registered Holding is selected
 * explicitly from the caller's WorkContext or a configured companyId; an
 * unknown target or an unexpected payload fails closed with no TS fallback.
 */
const MAX_GROUPS = 10_000;

/** SHA-256 of the owner Rust `SQL_PAY_GROUP_OPTIONS` constant. */
export const SQL_ORACLE_SHA256 =
  "d53d068a7d9d7a7a5f1ccaf34026302bf253cfb15128adc8c7cd6d9aa5aeda43";
/** SHA-256 of the pinned native response payload. */
export const RESPONSE_ORACLE_SHA256 =
  "dce62e6f1ebac21b311cd31edd02c69b8e6f2c8e62d0d7104718a378e692a29d";

const fail = (code: string, status = 503): never => {
  throw new ApiError(status, code, "The native People Payroll pay-group options service is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 256): value is string =>
  typeof value === "string" && value.length > 0 && new TextEncoder().encode(value).length <= max;
const validId = (value: unknown): value is string =>
  text(value, 200) && /^[A-Za-z0-9._:-]+$/.test(value);

type PayGroupOptionRow = Readonly<{
  id: string;
  name: string;
  currency: string;
  frequency: string;
}>;

const ROW_KEYS = ["id", "name", "currency", "frequency"];

function exactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  return Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
}

/**
 * Native replacement for `readPayrollOptionsEffect` (the
 * `PEOPLE_PAYROLL_RD_PAYROLL_OPTIONS_GROUPS` plan). `database` is retained only
 * for call-site compatibility; no statement is issued on it.
 */
export async function nativePayGroupOptions(
  _database: SqlDatabase,
  companyId: string,
  actor?: Lane9Actor,
  transport?: Transport,
): Promise<PayGroupOptionRow[]> {
  if (!validId(companyId)) return fail("NATIVE_PEOPLE_PAYROLL_PGO_COMPANY_INVALID", 422);
  const config = configuration(process.env["ERPX_NATIVE_PEOPLE_PAYROLL_LANE9_CONFIG"]);
  const target = selectTarget(config, actor, companyId);
  const headers = new Headers({
    host: target.host,
    origin: target.origin,
    accept: "application/json",
    "cache-control": "no-cache",
    "x-erpx-context-company": companyId,
  });
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/people-payroll/pay-group-options`,
    config.base,
  );
  let response: Response;
  try {
    const signal = AbortSignal.timeout(config.timeoutMs);
    response = transport
      ? await transport(url, headers, signal)
      : await nativeRequest(url, { headers: Object.fromEntries(headers), signal }, config.timeoutMs);
  } catch {
    return fail("NATIVE_PEOPLE_PAYROLL_PGO_UNAVAILABLE");
  }
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return fail("NATIVE_PEOPLE_PAYROLL_PGO_REDIRECT");
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    if (response.status >= 400 && response.status < 500) {
      return fail("NATIVE_PEOPLE_PAYROLL_PGO_REJECTED", response.status);
    }
    return fail("NATIVE_PEOPLE_PAYROLL_PGO_UNAVAILABLE");
  }
  const payload: unknown = await response.json().catch(() => null);
  if (!object(payload) || Object.keys(payload).length !== 1 || !Array.isArray(payload.groups)) {
    return fail("NATIVE_PEOPLE_PAYROLL_PGO_RESPONSE_INVALID", 502);
  }
  const groups: unknown[] = payload.groups;
  if (groups.length > MAX_GROUPS) {
    return fail("NATIVE_PEOPLE_PAYROLL_PGO_RESPONSE_INVALID", 502);
  }
  return groups.map((item: unknown) => {
    if (!object(item) || !exactKeys(item, ROW_KEYS)) {
      return fail("NATIVE_PEOPLE_PAYROLL_PGO_RESPONSE_INVALID", 502);
    }
    if (
      !validId(item.id) ||
      !text(item.name, 256) ||
      !text(item.currency, 16) ||
      !text(item.frequency, 32)
    ) {
      return fail("NATIVE_PEOPLE_PAYROLL_PGO_RESPONSE_INVALID", 502);
    }
    return {
      id: item.id,
      name: item.name,
      currency: item.currency,
      frequency: item.frequency,
    };
  });
}

export type { Lane9Actor, Transport } from "./native-lane-9/client";
