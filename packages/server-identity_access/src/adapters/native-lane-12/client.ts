import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError } from "@erpx/server-runtime/public/http";

/**
 * Strict private transport for lane 12's owner-local People-template attribute
 * reads. The axum owner executes the three statements, applies the Company
 * scope and owns the projection; this bridge carries no SQL, accepts no SQL
 * from a caller and never retries locally.
 *
 * Every registered Holding is configured explicitly. A read selects its target
 * by the caller's exact (holdingId, holdingSlug) pair; there is no
 * first-holding default. Work-context-free reads route only to the Holding
 * explicitly marked with the Control Company. Unknown targets fail closed.
 */
export type LaneHoldingScope = Pick<WorkContext, "holdingId" | "holdingSlug" | "companyId">;
type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Settings = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;

export type NativeLane12Page<T extends Record<string, unknown>> = Readonly<{
  rows: T[];
  rowCount: number;
}>;

export type NativeLane12Attribute = Readonly<{
  id: string;
  version: number;
  definition: Record<string, unknown>;
}>;

const ENV = "ERPX_NATIVE_LANE_12_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
/** Reads without a WorkContext route to the Holding explicitly marked with
 * this Company; never to the first configured Holding. */
const CONTROL_COMPANY = "platform";

const unavailable = (code = "NATIVE_LANE_12_READ_UNAVAILABLE", status = 503): never => {
  throw new ApiError(status, code, "The native People-template attribute read is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === "string" && new TextEncoder().encode(value).length <= max;
const id = (value: unknown, max = 200): value is string =>
  text(value, max) && value.length > 0 && /^[A-Za-z0-9_.:-]+$/.test(value);
const slug = (value: unknown): value is string =>
  id(value, 128) && /^[a-z0-9_-]+$/.test(value);

export function nativeLane12Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}

function configuration(): Settings {
  const raw = process.env[ENV];
  if (!raw) return unavailable("NATIVE_LANE_12_READ_CONFIGURATION_REQUIRED");
  let value: Record<string, unknown>;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!object(parsed)) return unavailable("NATIVE_LANE_12_READ_CONFIGURATION_INVALID");
    value = parsed;
  } catch {
    return unavailable("NATIVE_LANE_12_READ_CONFIGURATION_INVALID");
  }
  let base: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
  } catch {
    return unavailable("NATIVE_LANE_12_READ_CONFIGURATION_INVALID");
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
    base.origin !== String(value.privateBaseUrl) ||
    typeof value.timeoutMs !== "number" ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable("NATIVE_LANE_12_READ_CONFIGURATION_INVALID");
  const holdings: Holding[] = [];
  const keys = new Set<string>();
  for (const item of value.holdings) {
    if (!object(item) || !text(item.origin, 512) || !id(item.holdingId) || !slug(item.holdingSlug))
      return unavailable("NATIVE_LANE_12_READ_CONFIGURATION_INVALID");
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return unavailable("NATIVE_LANE_12_READ_CONFIGURATION_INVALID");
    }
    if (
      origin.protocol !== "https:" ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      !text(item.host, 512) ||
      item.host !== origin.host ||
      (item.companyId !== undefined && !id(item.companyId))
    )
      return unavailable("NATIVE_LANE_12_READ_CONFIGURATION_INVALID");
    const key = `${item.host}\u0000${item.holdingSlug}`;
    if (keys.has(key)) return unavailable("NATIVE_LANE_12_READ_CONFIGURATION_INVALID");
    keys.add(key);
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

/** Exact registered target; no first-holding fallback. */
function targetFor(config: Settings, scope: LaneHoldingScope): Holding {
  if (!id(scope.holdingId) || !slug(scope.holdingSlug))
    return unavailable("NATIVE_LANE_12_READ_TARGET_UNAVAILABLE");
  const found = config.holdings.filter(
    (item) => item.holdingId === scope.holdingId && item.holdingSlug === scope.holdingSlug,
  );
  if (found.length !== 1) return unavailable("NATIVE_LANE_12_READ_TARGET_UNAVAILABLE");
  return found[0]!;
}

/** Work-context-free reads use the explicitly marked Control Holding. */
function controlTarget(config: Settings): Holding {
  const marked = config.holdings.filter((item) => item.companyId === CONTROL_COMPANY);
  if (marked.length !== 1) return unavailable("NATIVE_LANE_12_READ_TARGET_UNAVAILABLE");
  return marked[0]!;
}

type Params = Readonly<Record<string, string>>;

function validateRows<T extends Record<string, unknown>>(
  kind: string,
  rows: unknown[],
  rowCount: number,
): asserts rows is T[] {
  if (rowCount !== rows.length) return unavailable("NATIVE_LANE_12_READ_RESPONSE_INVALID");
  for (const row of rows) {
    if (!object(row)) return unavailable("NATIVE_LANE_12_READ_RESPONSE_INVALID");
    if (kind === "attributes") {
      if (
        !text(row.id, 200) ||
        typeof row.version !== "number" ||
        !Number.isSafeInteger(row.version) ||
        !object(row.definition)
      )
        return unavailable("NATIVE_LANE_12_READ_RESPONSE_INVALID");
    } else if (kind === "expenseDecisionEvidence") {
      if (
        !text(row.id, 200) ||
        !text(row.role, 200) ||
        !text(row.scope_kind, 40) ||
        typeof row.valid_from !== "number" ||
        !Number.isSafeInteger(row.valid_from) ||
        typeof row.valid_until !== "number" ||
        !Number.isSafeInteger(row.valid_until)
      )
        return unavailable("NATIVE_LANE_12_READ_RESPONSE_INVALID");
    } else if (row.one !== 1) {
      return unavailable("NATIVE_LANE_12_READ_RESPONSE_INVALID");
    }
  }
}

async function readPage<T extends Record<string, unknown>>(
  segment: "people-template-attributes" | "approval-facts",
  kind: string,
  companyId: string,
  params: Params,
  scope?: LaneHoldingScope,
): Promise<NativeLane12Page<T>> {
  if (!id(companyId)) return unavailable("NATIVE_LANE_12_READ_REQUEST_INVALID", 422);
  const config = configuration();
  const target = scope ? targetFor(config, scope) : controlTarget(config);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/identity-access/${segment}/${kind}`,
    config.base,
  );
  for (const [key, value] of Object.entries(params)) url.searchParams.set(key, value);

  let response: Response;
  try {
    response = await fetch(url, {
      method: "GET",
      headers: {
        host: target.host,
        origin: target.origin,
        accept: "application/json",
        "content-type": "application/json",
        "cache-control": "no-cache",
        "x-erpx-context-company": companyId,
      },
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
    if (response.status >= 400 && response.status < 500) {
      throw new ApiError(
        response.status,
        text(error?.code, 100) ? String(error!.code) : "NATIVE_LANE_12_READ_FAILED",
        text(error?.message, 300) ? String(error!.message) : "The native People-template read was rejected.",
      );
    }
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  if (
    !object(payload) ||
    !Array.isArray(payload.rows) ||
    typeof payload.rowCount !== "number" ||
    !Number.isSafeInteger(payload.rowCount) ||
    payload.rowCount < 0
  )
    return unavailable("NATIVE_LANE_12_READ_RESPONSE_INVALID");
  validateRows(kind, payload.rows, payload.rowCount);
  return { rows: payload.rows as T[], rowCount: payload.rowCount };
}

export function nativePeopleTemplateAttributes<
  T extends Record<string, unknown> = NativeLane12Attribute,
>(companyId: string, scope?: LaneHoldingScope): Promise<NativeLane12Page<T>> {
  return readPage<T>("people-template-attributes", "attributes", companyId, {}, scope);
}

export function nativePeopleTemplateActiveRole(
  companyId: string,
  role: string,
  scope?: LaneHoldingScope,
): Promise<NativeLane12Page<Record<string, unknown>>> {
  return readPage<Record<string, unknown>>("people-template-attributes", "activeRole", companyId, { role }, scope);
}

export function nativePeopleTemplateGroupReferences(
  companyId: string,
  value: string,
  scope?: LaneHoldingScope,
): Promise<NativeLane12Page<Record<string, unknown>>> {
  return readPage<Record<string, unknown>>("people-template-attributes", "groupReferences", companyId, { value }, scope);
}

/* Owner-local workflow approval assignment reads. The retained repository
 * ports keep their pg-compatible shapes: `.first()` reads map rows[0] or null,
 * `.all()` reads map `{results}`. Command guards stay in TypeScript. */

async function firstApproval(
  kind: "canOverrideStaffStepRole" | "approvalViewActiveScope",
  companyId: string,
  params: Params,
  scope?: LaneHoldingScope,
): Promise<Record<string, unknown> | null> {
  const page = await readPage<Record<string, unknown>>("approval-facts", kind, companyId, params, scope);
  return page.rows[0] ?? null;
}

export function nativeApprovalCanOverride(
  companyId: string,
  assignmentId: string,
  userId: string,
  branchId: string | null,
  departmentId: string | null,
  scope?: LaneHoldingScope,
): Promise<Record<string, unknown> | null> {
  return firstApproval(
    "canOverrideStaffStepRole",
    companyId,
    {
      company: companyId,
      assignment: assignmentId,
      user: userId,
      ...(branchId === null ? {} : { branch: branchId }),
      ...(departmentId === null ? {} : { department: departmentId }),
    },
    scope,
  );
}

export function nativeApprovalActiveScope(
  companyId: string,
  assignmentId: string,
  userId: string,
  role: string,
  now: number,
  now2: number,
  policyStep: boolean,
  branchId: string | null,
  departmentId: string | null,
  scope?: LaneHoldingScope,
): Promise<Record<string, unknown> | null> {
  return firstApproval(
    "approvalViewActiveScope",
    companyId,
    {
      company: companyId,
      assignment: assignmentId,
      user: userId,
      role,
      now: String(now),
      now2: String(now2),
      policyStep: String(policyStep),
      ...(branchId === null ? {} : { branch: branchId }),
      ...(departmentId === null ? {} : { department: departmentId }),
    },
    scope,
  );
}

export async function nativeApprovalExpenseEvidence(
  companyId: string,
  assignmentId: string,
  now: number,
  now2: number,
  scope?: LaneHoldingScope,
): Promise<{ results: Record<string, unknown>[] }> {
  const page = await readPage<Record<string, unknown>>(
    "approval-facts",
    "expenseDecisionEvidence",
    companyId,
    { company: companyId, assignment: assignmentId, now: String(now), now2: String(now2) },
    scope,
  );
  return { results: page.rows };
}
