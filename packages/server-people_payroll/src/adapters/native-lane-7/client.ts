import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

/**
 * Strict private transport for the lane-7 native People Payroll clock read.
 * The axum router owns the statement, the Company scope and the projection.
 * This bridge emits no SQL, accepts no SQL from a caller and fails closed with
 * a 503 when the native service is missing or returns an unexpected shape.
 *
 * Every registered Holding is configured explicitly. A target is selected by
 * the caller's WorkContext `(holdingId, holdingSlug)` pair; callers without a
 * WorkContext (the IAM legacy-module consumer) must map through the configured
 * `companyId`. There is deliberately no first-holding default.
 */
const ENV = "ERPX_NATIVE_PEOPLE_PAYROLL_LANE7_CONFIG";
const READ_KIND = "people_payroll_lane_7_legacy_people_transition";
const PAYMENTS_KIND = "people_payroll_lane_7_workspace_people_payments";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

/** SHA-256 of the owner Rust `SQL_LEGACY_PEOPLE_TRANSITION` constant. */
export const SQL_ORACLE_SHA256 = "79ad85c3c72c22e111f812e45e22068608eeb21fb5e000c23beaa6ec162930f2";
/** SHA-256 of the pinned non-null native response payload. */
export const RESPONSE_ORACLE_SHA256 = "b9d5a6988cbe97e37f2c13ff946ff430253e78d54d4ded15907db887ac65b98f";
/** SHA-256 of the owner Rust `SQL_WORKSPACE_PEOPLE_PAYMENTS` constant. */
export const PAYMENTS_SQL_ORACLE_SHA256 = "97047147e5dc9782b5de8e4c0659cb6c693f45a398186acb19ddb1f2b0200c2e";
/** SHA-256 of the pinned People-payments response payload. */
export const PAYMENTS_RESPONSE_ORACLE_SHA256 = "991dbb8da772de20366554766db436b89235fad81b537272fdf54702bb348f2e";
const AUTHORITY_KIND = "people_payroll_lane_7_organization_authority_person";
/** SHA-256 of the owner Rust `SQL_AUTHORITY_ACTOR` constant. */
export const AUTHORITY_ACTOR_SQL_ORACLE_SHA256 = "c5264c5c857254afc0d7f10e7627a513e05a9d3aea5c250e08858d6b759a0840";
/** SHA-256 of the owner Rust `SQL_AUTHORITY_APPOINTEE` constant. */
export const AUTHORITY_APPOINTEE_SQL_ORACLE_SHA256 = "f579db98f3ccd908d63b13ef6c64fbb39096f7a8864cf2598dfd78bff8a5face";
/** SHA-256 of the pinned non-null authority response payload. */
export const AUTHORITY_RESPONSE_ORACLE_SHA256 = "e8888517d3fb5b2ba2f6b6c3de973ed39168d86f1efa8b80e7b594e30ec1c3b9";

/** Owner-projected payment row; keys are the original Finance column names. */
export type WorkspacePaymentRow = {
  id: string;
  source_type: string;
  source_id: string;
  currency: string;
  status: string;
  due_date: string | null;
  amount_minor: number;
};

/** Bounded, discriminator-only query for the two authority person lookups. */
export type AuthorityQuery =
  | { kind: "actor"; userId: string }
  | {
      kind: "appointee";
      personId: string;
      positionId: string;
      appointmentKind: "ceo" | "hr" | "head";
      scopeId: string;
      selfCeo: boolean;
    };

export type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Config = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;
/** The caller's trusted Holding identity. Both ids must match a registered target. */
export type Lane7Actor = Pick<WorkContext, "holdingId" | "holdingSlug"> &
  Partial<Pick<WorkContext, "companyId">>;
export type Transport = (url: URL, headers: Headers, signal: AbortSignal) => Promise<Response>;

const fail = (code: string, status = 503): never => {
  throw new ApiError(status, code, "The native People Payroll clock service is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 256): value is string =>
  typeof value === "string" && value.length > 0 && new TextEncoder().encode(value).length <= max;
const validId = (value: unknown): value is string =>
  text(value, 200) && /^[A-Za-z0-9._:-]+$/.test(value);
const slug = (value: unknown): value is string =>
  text(value, 128) && /^[a-z0-9_-]+$/.test(value as string);

export function nativeLane7Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return typeof environment[ENV] === "string";
}

function configuration(raw: string | undefined): Config {
  if (!raw) return fail("NATIVE_PEOPLE_PAYROLL_LANE7_CONFIG_REQUIRED");
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE7_CONFIG_INVALID");
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
    return fail("NATIVE_PEOPLE_PAYROLL_LANE7_CONFIG_INVALID");
  }
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE7_CONFIG_INVALID");
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
    return fail("NATIVE_PEOPLE_PAYROLL_LANE7_CONFIG_INVALID");
  }
  const holdings: Holding[] = [];
  const ids = new Set<string>();
  const hosts = new Set<string>();
  for (const item of value.holdings) {
    if (
      !object(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !text(item.holdingId, 200) ||
      !slug(item.holdingSlug) ||
      !(item.companyId === undefined || text(item.companyId, 200))
    ) {
      return fail("NATIVE_PEOPLE_PAYROLL_LANE7_CONFIG_INVALID");
    }
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return fail("NATIVE_PEOPLE_PAYROLL_LANE7_CONFIG_INVALID");
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
      return fail("NATIVE_PEOPLE_PAYROLL_LANE7_CONFIG_INVALID");
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
function selectTarget(config: Config, actor: Lane7Actor | undefined, companyId: string): Holding {
  if (actor !== undefined) {
    if (
      !object(actor) ||
      !validId(actor.holdingId) ||
      !slug(actor.holdingSlug) ||
      !(actor.companyId === undefined || validId(actor.companyId)) ||
      (actor.companyId !== undefined && actor.companyId !== companyId)
    ) {
      return fail("NATIVE_PEOPLE_PAYROLL_LANE7_ACTOR_INVALID", 422);
    }
    const target = config.holdings.find(
      (holding) =>
        holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
    );
    if (!target) return fail("NATIVE_PEOPLE_PAYROLL_LANE7_TARGET_UNAVAILABLE");
    return target;
  }
  const matches = config.holdings.filter((holding) => holding.companyId === companyId);
  if (matches.length !== 1) return fail("NATIVE_PEOPLE_PAYROLL_LANE7_TARGET_UNAVAILABLE");
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
 * Read the next owner-local transition deadline, or null when none is due.
 * `actor` selects the Holding when the caller has a WorkContext; otherwise the
 * configured `companyId` mapping is used. Both paths fail closed on an unknown
 * target and never fall back to TypeScript SQL.
 */
export async function readLegacyPeopleTransition(
  companyId: string,
  observedAt: number,
  actor?: Lane7Actor,
  transport: Transport = direct,
): Promise<number | null> {
  const config = configuration(process.env[ENV]);
  if (!validId(companyId)) return fail("PEOPLE_PAYROLL_LANE7_COMPANY_INVALID", 422);
  if (!Number.isSafeInteger(observedAt) || observedAt < 0) {
    return fail("PEOPLE_PAYROLL_LANE7_OBSERVED_AT_INVALID", 422);
  }
  const target = selectTarget(config, actor, companyId);
  const headers = new Headers({
    host: target.host,
    origin: target.origin,
    accept: "application/json",
    "cache-control": "no-cache",
    "x-erpx-context-holding": target.holdingId,
    "x-erpx-context-company": companyId,
    "x-erpx-observed-at": String(observedAt),
  });
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/people-payroll/legacy-people-clock`,
    config.base,
  );
  let response: Response;
  try {
    response = await transport(url, headers, AbortSignal.timeout(config.timeoutMs));
  } catch {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE7_UNAVAILABLE");
  }
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return fail("NATIVE_PEOPLE_PAYROLL_LANE7_REDIRECT");
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    if (response.status >= 400 && response.status < 500) {
      return fail("NATIVE_PEOPLE_PAYROLL_LANE7_REJECTED", response.status);
    }
    return fail("NATIVE_PEOPLE_PAYROLL_LANE7_UNAVAILABLE");
  }
  const payload: unknown = await response.json().catch(() => null);
  if (
    !object(payload) ||
    Object.keys(payload).length !== 2 ||
    payload.kind !== READ_KIND ||
    !object(payload.transition) ||
    Object.keys(payload.transition).length !== 1 ||
    !Object.hasOwn(payload.transition, "deadline")
  ) {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE7_RESPONSE_INVALID", 502);
  }
  const deadline: unknown = payload.transition.deadline;
  if (
    deadline !== null &&
    !(typeof deadline === "number" && Number.isSafeInteger(deadline) && deadline >= 0)
  ) {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE7_RESPONSE_INVALID", 502);
  }
  return deadline;
}

function paymentRow(value: unknown): value is WorkspacePaymentRow {
  return (
    object(value) &&
    Object.keys(value).length === 7 &&
    text(value.id, 200) &&
    text(value.source_type, 40) &&
    text(value.source_id, 200) &&
    text(value.currency, 3) &&
    /^[A-Z]{3}$/.test(value.currency) &&
    text(value.status, 40) &&
    (value.due_date === null || text(value.due_date, 20)) &&
    typeof value.amount_minor === "number" &&
    Number.isSafeInteger(value.amount_minor) &&
    value.amount_minor >= 0
  );
}

/**
 * Native-backed replacement of the Finance workspace people-payments SELECT.
 * Rust owns the statement, the aggregate, the Company scope and the row shape;
 * this bridge only transports and validates it, with no TypeScript fallback.
 */
export async function readFinanceWorkspacePeoplePayments(
  companyId: string,
  actor?: Lane7Actor,
  transport: Transport = direct,
): Promise<WorkspacePaymentRow[]> {
  const config = configuration(process.env[ENV]);
  if (!validId(companyId)) return fail("PEOPLE_PAYROLL_LANE7_COMPANY_INVALID", 422);
  const target = selectTarget(config, actor, companyId);
  const headers = new Headers({
    host: target.host,
    origin: target.origin,
    accept: "application/json",
    "cache-control": "no-cache",
    "x-erpx-context-holding": target.holdingId,
    "x-erpx-context-company": companyId,
  });
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/people-payroll/finance-workspace-payments`,
    config.base,
  );
  let response: Response;
  try {
    response = await transport(url, headers, AbortSignal.timeout(config.timeoutMs));
  } catch {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE7_UNAVAILABLE");
  }
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return fail("NATIVE_PEOPLE_PAYROLL_LANE7_REDIRECT");
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    if (response.status >= 400 && response.status < 500) {
      return fail("NATIVE_PEOPLE_PAYROLL_LANE7_REJECTED", response.status);
    }
    return fail("NATIVE_PEOPLE_PAYROLL_LANE7_UNAVAILABLE");
  }
  const payload: unknown = await response.json().catch(() => null);
  if (
    !object(payload) ||
    Object.keys(payload).length !== 2 ||
    payload.kind !== PAYMENTS_KIND ||
    !Array.isArray(payload.payments) ||
    payload.payments.length > 500 ||
    !payload.payments.every(paymentRow)
  ) {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE7_RESPONSE_INVALID", 502);
  }
  return payload.payments;
}

/** Serialize the bounded authority query; no caller SQL reaches the service. */
function authorityParams(query: AuthorityQuery): URLSearchParams {
  const invalid = () => fail("PEOPLE_PAYROLL_LANE7_AUTHORITY_QUERY_INVALID", 422);
  const params = new URLSearchParams();
  if (query.kind === "actor") {
    if (!validId(query.userId)) return invalid();
    params.set("kind", "actor");
    params.set("user_id", query.userId);
    return params;
  }
  if (
    !validId(query.personId) ||
    !text(query.positionId, 200) ||
    !["ceo", "hr", "head"].includes(query.appointmentKind) ||
    !validId(query.scopeId) ||
    typeof query.selfCeo !== "boolean"
  ) {
    return invalid();
  }
  params.set("kind", "appointee");
  params.set("person_id", query.personId);
  params.set("position_id", query.positionId);
  params.set("appointment_kind", query.appointmentKind);
  params.set("scope_id", query.scopeId);
  params.set("self_ceo", query.selfCeo ? "true" : "false");
  return params;
}

/**
 * Native-backed replacement of the Company-scoped Organization actor/appointee
 * lookups. Rust owns both statements, the Company guard and the id projection;
 * this bridge transports the discriminator query and validates the reply with
 * no TypeScript SQL or fallback.
 */
export async function readOrganizationAuthorityPerson(
  companyId: string,
  query: AuthorityQuery,
  actor?: Lane7Actor,
  transport: Transport = direct,
): Promise<string | null> {
  const config = configuration(process.env[ENV]);
  if (!validId(companyId)) return fail("PEOPLE_PAYROLL_LANE7_COMPANY_INVALID", 422);
  const params = authorityParams(query);
  const target = selectTarget(config, actor, companyId);
  const headers = new Headers({
    host: target.host,
    origin: target.origin,
    accept: "application/json",
    "cache-control": "no-cache",
    "x-erpx-context-holding": target.holdingId,
    "x-erpx-context-company": companyId,
  });
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/people-payroll/organization-authority-person?${params.toString()}`,
    config.base,
  );
  let response: Response;
  try {
    response = await transport(url, headers, AbortSignal.timeout(config.timeoutMs));
  } catch {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE7_UNAVAILABLE");
  }
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return fail("NATIVE_PEOPLE_PAYROLL_LANE7_REDIRECT");
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    if (response.status >= 400 && response.status < 500) {
      return fail("NATIVE_PEOPLE_PAYROLL_LANE7_REJECTED", response.status);
    }
    return fail("NATIVE_PEOPLE_PAYROLL_LANE7_UNAVAILABLE");
  }
  const payload: unknown = await response.json().catch(() => null);
  if (
    !object(payload) ||
    Object.keys(payload).length !== 2 ||
    payload.kind !== AUTHORITY_KIND ||
    !object(payload.person) ||
    Object.keys(payload.person).length !== 1 ||
    !Object.hasOwn(payload.person, "id") ||
    !(
      payload.person.id === null ||
      (typeof payload.person.id === "string" && validId(payload.person.id))
    )
  ) {
    return fail("NATIVE_PEOPLE_PAYROLL_LANE7_RESPONSE_INVALID", 502);
  }
  return payload.person.id;
}
