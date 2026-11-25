import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

/**
 * Strict private transport for the lane-9 native People Payroll person-image
 * read. The axum owner executes the retired `projectWorkerPersonImage` statement
 * with sqlx and owns its Company scope, projection and response shape. This
 * bridge emits no SQL, accepts no SQL from a caller and fails closed with a 503
 * when the native service is absent or the payload is unexpected.
 *
 * Every registered Holding is configured explicitly. A target is selected by
 * the caller's WorkContext `(holdingId, holdingSlug)` pair; a caller without a
 * WorkContext (the durable placement job) must map through a configured
 * `companyId`. There is deliberately no first-holding default.
 */
const ENV = "ERPX_NATIVE_PEOPLE_PAYROLL_LANE9_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const IMAGE_KEYS = ["person", "engagements", "employees", "cases", "tasks", "compensation", "leave"];

/** SHA-256 of the owner Rust `SQL_PERSON_IMAGE` constant. */
export const SQL_ORACLE_SHA256 = "944d4be0677435e95c0f3fe456038c9f217c5ec2c4990673318d7bc9901b64e0";
/** SHA-256 of the pinned non-null native response payload. */
export const RESPONSE_ORACLE_SHA256 = "5ef5ac117da385b5f86d249033449b7fff1483f9317ef0378804a993b595b81c";

export type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
export type Config = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;
/** The caller's trusted Holding identity. Both ids must match a registered target. */
export type Lane9Actor = Pick<WorkContext, "holdingId" | "holdingSlug"> &
  Partial<Pick<WorkContext, "companyId">>;
export type Transport = (url: URL, headers: Headers, signal: AbortSignal) => Promise<Response>;

const fail = (code: string, status = 503): never => {
  throw new ApiError(status, code, "The native People Payroll person-image service is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 256): value is string =>
  typeof value === "string" && value.length > 0 && new TextEncoder().encode(value).length <= max;
const validId = (value: unknown): value is string =>
  text(value, 200) && /^[A-Za-z0-9._:-]+$/.test(value);
const slug = (value: unknown): value is string =>
  text(value, 128) && /^[a-z0-9_-]+$/.test(value);

export function nativeLane9Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return typeof environment[ENV] === "string";
}

export function configuration(raw: string | undefined): Config {
  if (!raw) return fail("NATIVE_PEOPLE_PAYROLL_LANE9_CONFIG_REQUIRED");
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_CONFIG_INVALID");
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
  ) {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_CONFIG_INVALID");
  }
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_CONFIG_INVALID");
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
  ) {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_CONFIG_INVALID");
  }
  const holdings: Holding[] = [];
  const ids = new Set<string>();
  const hosts = new Set<string>();
  for (const item of value.holdings) {
    if (
      !object(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !validId(item.holdingId) ||
      !slug(item.holdingSlug) ||
      !(item.companyId === undefined || validId(item.companyId))
    ) {
      return fail("NATIVE_PEOPLE_PAYROLL_LANE9_CONFIG_INVALID");
    }
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return fail("NATIVE_PEOPLE_PAYROLL_LANE9_CONFIG_INVALID");
    }
    if (
      origin.protocol !== "https:" ||
      origin.origin !== item.origin ||
      origin.host !== item.host ||
      origin.username ||
      origin.password ||
      ids.has(item.holdingId) ||
      hosts.has(`${item.host}|${item.holdingSlug}`)
    ) {
      return fail("NATIVE_PEOPLE_PAYROLL_LANE9_CONFIG_INVALID");
    }
    ids.add(item.holdingId);
    hosts.add(`${item.host}|${item.holdingSlug}`);
    holdings.push({
      origin: origin.origin,
      host: origin.host,
      holdingId: item.holdingId,
      holdingSlug: item.holdingSlug,
      ...(item.companyId === undefined ? {} : { companyId: item.companyId }),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Resolve one registered target. Never infer a Holding from the first entry. */
export function selectTarget(config: Config, actor: Lane9Actor | undefined, companyId: string): Holding {
  if (actor !== undefined) {
    if (
      !object(actor) ||
      !validId(actor.holdingId) ||
      !slug(actor.holdingSlug) ||
      !(actor.companyId === undefined || validId(actor.companyId)) ||
      (actor.companyId !== undefined && actor.companyId !== companyId)
    ) {
      return fail("NATIVE_PEOPLE_PAYROLL_LANE9_ACTOR_INVALID", 422);
    }
    const target = config.holdings.find(
      (holding) =>
        holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
    );
    if (!target) return fail("NATIVE_PEOPLE_PAYROLL_LANE9_TARGET_UNAVAILABLE");
    return target;
  }
  const matches = config.holdings.filter((holding) => holding.companyId === companyId);
  if (matches.length !== 1) return fail("NATIVE_PEOPLE_PAYROLL_LANE9_TARGET_UNAVAILABLE");
  return matches[0]!;
}

export async function direct(url: URL, headers: Headers, signal: AbortSignal): Promise<Response> {
  return fetch(url, {
    method: "GET",
    headers: Object.fromEntries(headers),
    cache: "no-store",
    redirect: "error",
    signal,
  });
}

/**
 * Native-backed replacement of the deleted TS SQL. `database` is retained only
 * for call-site compatibility; no statement is issued on it. `actor` selects
 * the Holding by WorkContext; without it the configured `companyId` mapping is
 * used. Both paths fail closed on an unknown target and never fall back to TS SQL.
 */
export async function personImage(
  _database: SqlDatabase,
  companyId: string,
  personId: string,
  actor?: Lane9Actor,
  transport: Transport = direct,
): Promise<string> {
  const config = configuration(process.env[ENV]);
  if (!validId(companyId)) return fail("PEOPLE_PAYROLL_LANE9_COMPANY_INVALID", 422);
  if (!validId(personId)) return fail("PEOPLE_PAYROLL_LANE9_PERSON_INVALID", 422);
  const target = selectTarget(config, actor, companyId);
  const headers = new Headers({
    host: target.host,
    origin: target.origin,
    accept: "application/json",
    "cache-control": "no-cache",
    "x-erpx-context-holding": target.holdingId,
    "x-erpx-context-company": companyId,
    "x-erpx-context-person": personId,
  });
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/people-payroll/person-image`,
    config.base,
  );
  let response: Response;
  try {
    response = await transport(url, headers, AbortSignal.timeout(config.timeoutMs));
  } catch {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_UNAVAILABLE");
  }
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_REDIRECT");
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    if (response.status >= 400 && response.status < 500) {
      return fail("NATIVE_PEOPLE_PAYROLL_LANE9_REJECTED", response.status);
    }
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_UNAVAILABLE");
  }
  const payload: unknown = await response.json().catch(() => null);
  if (!object(payload) || Object.keys(payload).length !== 1 || !Object.hasOwn(payload, "personImage")) {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_RESPONSE_INVALID", 502);
  }
  const image: unknown = payload.personImage;
  if (typeof image !== "string" || image.length === 0 || image.length > 1_048_576) {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_RESPONSE_INVALID", 502);
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(image);
  } catch {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_RESPONSE_INVALID", 502);
  }
  if (
    !object(parsed) ||
    Object.keys(parsed).length !== IMAGE_KEYS.length ||
    IMAGE_KEYS.some(key => !(key in parsed))
  ) {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE9_RESPONSE_INVALID", 502);
  }
  return image;
}
