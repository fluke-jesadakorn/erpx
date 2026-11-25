import "server-only";
import { headers } from "next/headers";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

/**
 * Strict private transport for the native People Payroll app-entrypoint read
 * family (lane 8). The axum owner executes the SQL with sqlx, owns the Company
 * scope and projects the response; this bridge carries only bounded identities.
 *
 * Every registered Holding is supported. The target is selected from the
 * caller's WorkContext (holdingId + holdingSlug), or, for legacy fact callers
 * without one, from the exact incoming Host. There is no first-Holding default
 * and an unknown target fails closed. Missing configuration, transport failure
 * or a malformed reply fail closed with no TypeScript SQL fallback.
 */
export type Lane8Actor = Pick<WorkContext, "holdingId" | "holdingSlug" | "companyId">;

type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Config = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;

export type PayGroup = Readonly<{ id: string; name: string; currency: string; frequency: string }>;
export type PersonOption = Readonly<{
  id: string;
  legal_name: string;
  email: string | null;
  version: number;
}>;
export type ApproverName = Readonly<{ legal_name: string }>;
export type RosterShift = Readonly<{
  id: string;
  local_date: string;
  start_time: string;
  end_time: string | null;
  end_local: string;
  timezone: string;
}>;
export type SqlLikeResult<T> = Readonly<{
  results: T[];
  success: true;
  meta: Readonly<{ changes: 0; duration: 0 }>;
}>;

export type IssuableRun = Readonly<{
  id: string;
  input_digest: string;
  revision: number;
  version: number;
  mode: string;
  period_start: string;
  period_end: string;
  pay_date: string;
  currency: string;
}>;
export type PayslipItemRow = Readonly<{
  id: string;
  engagement_id: string;
  person_id: string;
  recipient_user_id: string | null;
  input_snapshot_json: string;
  result_json: string;
  gross_minor: number;
  deductions_minor: number;
  employer_cost_minor: number;
  net_minor: number;
  currency: string;
}>;
export type OwnSlipRow = Readonly<{
  id: string;
  run_id: string;
  run_revision: number;
  pay_date: string;
  currency: string;
  gross_minor: number;
  deductions_minor: number;
  net_minor: number;
  source_digest: string;
  export_sha256: string;
  issued_at: number;
  snapshot_json: string;
}>;
export type SlipRow = Readonly<{
  id: string;
  run_id: string;
  person_id: string;
  recipient_user_id: string | null;
  snapshot_json: string;
  export_html: string;
  export_sha256: string;
  issued_at: number;
}>;
export type PaymentRow = Readonly<{ status: string; settled_at: string | null; batch_status: string }>;
export type ExportBytesRow = Readonly<{ export_html: string; export_sha256: string }>;

const ENV = "ERPX_NATIVE_PEOPLE_PAYROLL_LANE8_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const MAX_ROWS = 5000;
const MAX_ID = 200;

export const LANE8_RD_PAY_GROUPS = "hire_handoff_pay_groups";
export const LANE8_RD_PEOPLE = "hire_handoff_people";
export const LANE8_RD_APPROVER_NAME = "request_approver_legal_name";
export const LANE8_RD_ROSTER_SHIFT = "overtime_roster_shift";
export const LANE8_RD_ISSUABLE_RUN = "payslip_issuable_run";
export const LANE8_RD_ISSUABLE_ITEMS = "payslip_issuable_items";
export const LANE8_RD_OWN_SLIPS = "payslip_own_slips";
export const LANE8_RD_SLIP = "payslip_slip";
export const LANE8_RD_PAYMENT = "payslip_payment";
export const LANE8_RD_EXPORT_BYTES = "payslip_export_bytes";

const unavailable = (): never => {
  throw new ApiError(503, "PEOPLE_PAYROLL_LANE8_UNAVAILABLE", "The native People Payroll read service is unavailable.");
};
const invalid = (): never => {
  throw new ApiError(503, "PEOPLE_PAYROLL_LANE8_CONFIG_INVALID", "The native People Payroll read transport is misconfigured.");
};
const targetChanged = (): never => {
  throw new ApiError(409, "WORK_CONTEXT_CHANGED", "The selected Holding is not registered.");
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === "string" && new TextEncoder().encode(value).length <= max;
const id = (value: unknown): value is string =>
  text(value, MAX_ID) && /^[A-Za-z0-9_.:-]+$/.test(value);

/** Selector presence only; an absent configuration never probes the service. */
export function nativeLane8Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}

/** Strict configuration: 1..64 exact, distinct registered Holdings. */
export function lane8Configuration(raw: string | undefined = process.env[ENV]): Config {
  if (raw === undefined) return unavailable();
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return invalid(); }
  if (
    !record(value) ||
    !text(value.privateBaseUrl, 512) ||
    typeof value.timeoutMs !== "number" ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  ) return invalid();
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return invalid(); }
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
  ) return invalid();

  const holdings: Holding[] = [];
  const keys = new Set<string>();
  const hosts = new Set<string>();
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !id(item.holdingId) ||
      !id(item.holdingSlug) ||
      !/^[a-z0-9_-]{1,128}$/.test(String(item.holdingSlug)) ||
      (item.companyId !== undefined && !id(item.companyId))
    ) return invalid();
    let origin: URL;
    try { origin = new URL(String(item.origin)); } catch { return invalid(); }
    if (
      origin.protocol !== "https:" || origin.origin !== item.origin ||
      origin.username || origin.password || origin.host !== item.host
    ) return invalid();
    const holding: Holding = {
      origin: origin.origin,
      host: String(item.host),
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
      ...(item.companyId === undefined ? {} : { companyId: String(item.companyId) }),
    };
    const key = `${holding.holdingId}\u0000${holding.holdingSlug}`;
    if (keys.has(key) || hosts.has(holding.host)) return invalid();
    keys.add(key);
    hosts.add(holding.host);
    holdings.push(holding);
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Resolve exactly one registered Holding. No first-Holding default: an actor
 * must name a registered (holdingId, holdingSlug) pair, and a legacy fact caller
 * must present an exact registered Host. A configured companyId pins the actor. */
async function targetFor(config: Config, actor?: Lane8Actor): Promise<Holding> {
  const incoming = await headers().catch(() => null);
  const host = incoming?.get("host") ?? null;
  if (actor) {
    if (!id(actor.holdingId) || !id(actor.holdingSlug) || !id(actor.companyId)) return targetChanged();
    const target = config.holdings.find(
      (holding) =>
        holding.holdingId === actor.holdingId &&
        holding.holdingSlug === actor.holdingSlug &&
        (holding.companyId === undefined || holding.companyId === actor.companyId),
    );
    if (!target || (host !== null && host !== target.host)) return targetChanged();
    return target;
  }
  const target = host === null ? undefined : config.holdings.find(holding => holding.host === host);
  if (!target) return targetChanged();
  return target;
}

function payGroup(value: unknown): PayGroup {
  if (
    !keysExact(value, ["id", "name", "currency", "frequency"]) ||
    !id(value.id) || !text(value.name) || !text(value.currency, 32) || !text(value.frequency, 32)
  ) return unavailable();
  return {
    id: value.id,
    name: value.name,
    currency: value.currency,
    frequency: value.frequency,
  };
}

function personOption(value: unknown): PersonOption {
  if (
    !keysExact(value, ["id", "legal_name", "email", "version"]) ||
    !id(value.id) || !text(value.legal_name) ||
    !(value.email === null || text(value.email)) ||
    typeof value.version !== "number" || !Number.isSafeInteger(value.version)
  ) return unavailable();
  return {
    id: value.id,
    legal_name: value.legal_name,
    email: value.email as string | null,
    version: value.version,
  };
}

function approverName(value: unknown): ApproverName | null {
  if (value === null) return null;
  if (keysExact(value, ["legal_name"]) && text(value.legal_name)) {
    return { legal_name: value.legal_name };
  }
  return unavailable();
}

function rosterShift(value: unknown): RosterShift {
  if (
    !keysExact(value, ["id", "local_date", "start_time", "end_time", "end_local", "timezone"]) ||
    !id(value.id) || !text(value.local_date, 32) || !text(value.start_time, 32) ||
    !(value.end_time === null || text(value.end_time, 32)) ||
    !text(value.end_local, 32) || !text(value.timezone, 64)
  ) return unavailable();
  return {
    id: value.id,
    local_date: value.local_date,
    start_time: value.start_time,
    end_time: value.end_time as string | null,
    end_local: value.end_local,
    timezone: value.timezone,
  };
}

function keysExact(value: unknown, expected: readonly string[]): value is Record<string, unknown> {
  return record(value) &&
    Object.keys(value).length === expected.length &&
    expected.every((key) => Object.prototype.hasOwnProperty.call(value, key));
}

/** Execute exactly one owner read on the resolved Holding and validate the
 * wire envelope strictly. */
async function read(
  kind: string,
  values: readonly string[],
  companyId: string,
  actor?: Lane8Actor,
): Promise<unknown> {
  if (!id(companyId)) throw new ApiError(422, "PEOPLE_PAYROLL_LANE8_COMPANY_INVALID", "Use a bounded Company id.");
  const config = lane8Configuration();
  const target = await targetFor(config, actor);
  if (target.companyId !== undefined && target.companyId !== companyId) return targetChanged();
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/people-payroll/app-entrypoint-read`,
    config.base,
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
        "x-erpx-context-company": companyId,
      },
      body: JSON.stringify({ kind, values }),
      cache: "no-store",
      redirect: "error",
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch { return unavailable(); }
  if (response.status !== 200) {
    const payload = await response.json().catch((): unknown => null);
    const error = record(payload) && record(payload.error) ? payload.error : undefined;
    if (response.status >= 400 && response.status < 500)
      throw new ApiError(
        response.status,
        text(error?.code, 100) ? String(error!.code) : "PEOPLE_PAYROLL_LANE8_READ_FAILED",
        text(error?.message, 300) ? String(error!.message) : "The native People Payroll read was rejected.",
      );
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  if (!record(payload) || !("data" in payload) || Object.keys(payload).length !== 1) return unavailable();
  return payload.data;
}

function rows<T>(value: unknown, validate: (row: unknown) => T): SqlLikeResult<T> {
  if (!Array.isArray(value) || value.length > MAX_ROWS) return unavailable();
  return {
    results: value.map(validate),
    success: true,
    meta: { changes: 0, duration: 0 },
  };
}

/** The four leaf reads. `db` is retained for call-site identity only; execution
 * is owned by the axum/sqlx reader, never by TypeScript. `actor` is optional for
 * legacy fact callers, which resolve the exact registered Host instead. */
export async function readHireHandoffPayGroups(
  db: SqlDatabase,
  companyId: string,
  actor?: Lane8Actor,
): Promise<SqlLikeResult<PayGroup>> {
  void db;
  return rows(await read(LANE8_RD_PAY_GROUPS, [], companyId, actor), payGroup);
}

export async function readHireHandoffPeople(
  db: SqlDatabase,
  companyId: string,
  actor?: Lane8Actor,
): Promise<SqlLikeResult<PersonOption>> {
  void db;
  return rows(await read(LANE8_RD_PEOPLE, [], companyId, actor), personOption);
}

export async function readApproverName(
  db: SqlDatabase,
  companyId: string,
  userId: string,
  actor?: Lane8Actor,
): Promise<ApproverName | null> {
  void db;
  return approverName(await read(LANE8_RD_APPROVER_NAME, [userId], companyId, actor));
}

export async function readRosterShift(
  db: SqlDatabase,
  shiftId: string,
  companyId: string,
  engagementId: string,
  actor?: Lane8Actor,
): Promise<RosterShift | null> {
  void db;
  return rosterShift(await read(LANE8_RD_ROSTER_SHIFT, [shiftId, engagementId], companyId, actor));
}

const whole = (value: unknown): value is number => typeof value === "number" && Number.isSafeInteger(value);

function issuableRun(value: unknown): IssuableRun | null {
  if (value === null) return null;
  if (
    !keysExact(value, ["id", "input_digest", "revision", "version", "mode", "period_start", "period_end", "pay_date", "currency"]) ||
    !id(value.id) || !text(value.input_digest, 200) || !whole(value.revision) || !whole(value.version) ||
    !text(value.mode, 64) || !text(value.period_start, 32) || !text(value.period_end, 32) ||
    !text(value.pay_date, 32) || !text(value.currency, 32)
  ) return unavailable();
  return {
    id: value.id, input_digest: value.input_digest, revision: value.revision, version: value.version,
    mode: value.mode, period_start: value.period_start, period_end: value.period_end,
    pay_date: value.pay_date, currency: value.currency,
  };
}

function issuableItems(value: unknown): PayslipItemRow {
  if (
    !keysExact(value, ["id", "engagement_id", "person_id", "recipient_user_id", "input_snapshot_json", "result_json", "gross_minor", "deductions_minor", "employer_cost_minor", "net_minor", "currency"]) ||
    !id(value.id) || !id(value.engagement_id) || !id(value.person_id) ||
    !(value.recipient_user_id === null || id(value.recipient_user_id)) ||
    !text(value.input_snapshot_json, 1_048_576) || !text(value.result_json, 1_048_576) ||
    !whole(value.gross_minor) || !whole(value.deductions_minor) || !whole(value.employer_cost_minor) ||
    !whole(value.net_minor) || !text(value.currency, 32)
  ) return unavailable();
  return {
    id: value.id, engagement_id: value.engagement_id, person_id: value.person_id,
    recipient_user_id: value.recipient_user_id as string | null,
    input_snapshot_json: value.input_snapshot_json, result_json: value.result_json,
    gross_minor: value.gross_minor, deductions_minor: value.deductions_minor,
    employer_cost_minor: value.employer_cost_minor, net_minor: value.net_minor, currency: value.currency,
  };
}

function ownSlip(value: unknown): OwnSlipRow {
  if (
    !keysExact(value, ["id", "run_id", "run_revision", "pay_date", "currency", "gross_minor", "deductions_minor", "net_minor", "source_digest", "export_sha256", "issued_at", "snapshot_json"]) ||
    !id(value.id) || !id(value.run_id) || !whole(value.run_revision) || !text(value.pay_date, 32) ||
    !text(value.currency, 32) || !whole(value.gross_minor) || !whole(value.deductions_minor) ||
    !whole(value.net_minor) || !text(value.source_digest, 200) || !text(value.export_sha256, 200) ||
    !whole(value.issued_at) || !text(value.snapshot_json, 1_048_576)
  ) return unavailable();
  return {
    id: value.id, run_id: value.run_id, run_revision: value.run_revision, pay_date: value.pay_date,
    currency: value.currency, gross_minor: value.gross_minor, deductions_minor: value.deductions_minor,
    net_minor: value.net_minor, source_digest: value.source_digest, export_sha256: value.export_sha256,
    issued_at: value.issued_at, snapshot_json: value.snapshot_json,
  };
}

function slip(value: unknown): SlipRow | null {
  if (value === null) return null;
  if (
    !keysExact(value, ["id", "run_id", "person_id", "recipient_user_id", "snapshot_json", "export_html", "export_sha256", "issued_at"]) ||
    !id(value.id) || !id(value.run_id) || !id(value.person_id) ||
    !(value.recipient_user_id === null || id(value.recipient_user_id)) ||
    !text(value.snapshot_json, 1_048_576) || !text(value.export_html, 4_194_304) ||
    !text(value.export_sha256, 200) || !whole(value.issued_at)
  ) return unavailable();
  return {
    id: value.id, run_id: value.run_id, person_id: value.person_id,
    recipient_user_id: value.recipient_user_id as string | null,
    snapshot_json: value.snapshot_json, export_html: value.export_html,
    export_sha256: value.export_sha256, issued_at: value.issued_at,
  };
}

function payment(value: unknown): PaymentRow | null {
  if (value === null) return null;
  if (
    !keysExact(value, ["status", "settled_at", "batch_status"]) ||
    !text(value.status, 64) || !(value.settled_at === null || text(value.settled_at, 64)) ||
    !text(value.batch_status, 64)
  ) return unavailable();
  return {
    status: value.status, settled_at: value.settled_at as string | null,
    batch_status: value.batch_status,
  };
}

function exportBytes(value: unknown): ExportBytesRow | null {
  if (value === null) return null;
  if (
    !keysExact(value, ["export_html", "export_sha256"]) ||
    !text(value.export_html, 4_194_304) || !text(value.export_sha256, 200)
  ) return unavailable();
  return { export_html: value.export_html, export_sha256: value.export_sha256 };
}

/** Payslip family: same owner route, six closed read kinds. Empty-string
 * sentinels mean an absent optional filter; user filters are always paired. */
export async function readIssuableRun(
  db: SqlDatabase, runId: string, companyId: string, actor?: Lane8Actor,
): Promise<IssuableRun | null> {
  void db;
  if (!id(runId)) return unavailable();
  return issuableRun(await read(LANE8_RD_ISSUABLE_RUN, [runId], companyId, actor));
}

export async function readIssuableItems(
  db: SqlDatabase, companyId: string, runId: string, actor?: Lane8Actor,
): Promise<SqlLikeResult<PayslipItemRow>> {
  void db;
  if (!id(runId)) return unavailable();
  return rows(await read(LANE8_RD_ISSUABLE_ITEMS, [runId], companyId, actor), issuableItems);
}

export async function readOwnSlips(
  db: SqlDatabase, companyId: string, runId: string | undefined, userId: string | undefined,
  actor?: Lane8Actor,
): Promise<SqlLikeResult<OwnSlipRow>> {
  void db;
  const run = runId ?? "";
  const user = userId ?? "";
  return rows(await read(LANE8_RD_OWN_SLIPS, [run, user, user], companyId, actor), ownSlip);
}

export async function readSlip(
  db: SqlDatabase, slipId: string, companyId: string, userId: string | undefined,
  actor?: Lane8Actor,
): Promise<SlipRow | null> {
  void db;
  const user = userId ?? "";
  return slip(await read(LANE8_RD_SLIP, [slipId, user, user], companyId, actor));
}

export async function readPayment(
  db: SqlDatabase, companyId: string, runId: string, personId: string, actor?: Lane8Actor,
): Promise<PaymentRow | null> {
  void db;
  return payment(await read(LANE8_RD_PAYMENT, [runId, personId], companyId, actor));
}

export async function readExportBytes(
  db: SqlDatabase, slipId: string, companyId: string, actor?: Lane8Actor,
): Promise<ExportBytesRow | null> {
  void db;
  return exportBytes(await read(LANE8_RD_EXPORT_BYTES, [slipId], companyId, actor));
}
