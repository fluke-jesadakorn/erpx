import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError } from "@erpx/server-runtime/public/http";

/**
 * Strict private transport for lane 10's native account invitation reads.
 * The axum owner owns all three statements, the Company scope and the
 * projection. This bridge carries no SQL, accepts no SQL from a caller and
 * fails closed when the native runtime is missing, malformed or unavailable.
 *
 * Every registered Holding is configured explicitly. A read selects its
 * target by the caller's exact (holdingId, holdingSlug) pair; there is no
 * first-holding default and an unknown target is a hard 503.
 */
export type NativeInvitation = {
  id: string;
  holding_id: string;
  company_id: string;
  person_id: string;
  email: string;
  name: string;
  role: string;
  scope_kind: 'holding' | 'branch' | 'department';
  branch_id: string | null;
  department_id: string | null;
  created_by: string;
  assignment_id: string;
  expires_at: number;
  status: string;
};
export type LaneHoldingScope = Pick<WorkContext, "holdingId" | "holdingSlug" | "companyId">;
type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Config = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;
type Reader = (
  kind: string,
  values: Record<string, unknown>,
  companyId: string,
  target: Holding,
) => Promise<unknown>;

const ENV = "ERPX_NATIVE_LANE_10_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
/** Control-plane reads without a WorkContext route to the holding explicitly
 * marked with this Company; never to the first configured holding. */
const CONTROL_COMPANY = "platform";

const unavailable = (code = "ACCOUNT_ACCESS_READ_UNAVAILABLE", status = 503): never => {
  throw new ApiError(status, code, "The native account access read service is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === "string" && new TextEncoder().encode(value).length <= max;
const id = (value: unknown, max = 200): value is string =>
  text(value, max) && /^[A-Za-z0-9_.:-]+$/.test(value);
const slug = (value: unknown): value is string =>
  id(value, 128) && /^[a-z0-9_-]+$/.test(value);

function configuration(): Config {
  const raw = process.env[ENV];
  if (!raw) return unavailable("ACCOUNT_ACCESS_READ_CONFIGURATION_REQUIRED");
  let value: Record<string, unknown>;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!object(parsed)) return unavailable("ACCOUNT_ACCESS_READ_CONFIGURATION_INVALID");
    value = parsed;
  } catch {
    return unavailable("ACCOUNT_ACCESS_READ_CONFIGURATION_INVALID");
  }
  let base: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
  } catch {
    return unavailable("ACCOUNT_ACCESS_READ_CONFIGURATION_INVALID");
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
    return unavailable("ACCOUNT_ACCESS_READ_CONFIGURATION_INVALID");
  const holdings: Holding[] = [];
  const keys = new Set<string>();
  for (const item of value.holdings) {
    if (!object(item) || !text(item.origin, 512) || !id(item.holdingId) || !slug(item.holdingSlug))
      return unavailable("ACCOUNT_ACCESS_READ_CONFIGURATION_INVALID");
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return unavailable("ACCOUNT_ACCESS_READ_CONFIGURATION_INVALID");
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
      return unavailable("ACCOUNT_ACCESS_READ_CONFIGURATION_INVALID");
    const key = `${item.host}\u0000${item.holdingSlug}`;
    if (keys.has(key)) return unavailable("ACCOUNT_ACCESS_READ_CONFIGURATION_INVALID");
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
function targetFor(scope: LaneHoldingScope): Holding {
  if (!id(scope.holdingId) || !slug(scope.holdingSlug))
    return unavailable("ACCOUNT_ACCESS_READ_TARGET_UNAVAILABLE");
  const config = configuration();
  const found = config.holdings.filter(
    (item) => item.holdingId === scope.holdingId && item.holdingSlug === scope.holdingSlug,
  );
  if (found.length !== 1) return unavailable("ACCOUNT_ACCESS_READ_TARGET_UNAVAILABLE");
  return found[0]!;
}

/** Control-wide reads (no WorkContext) use the explicitly marked Control holding. */
function controlScope(): LaneHoldingScope {
  const config = configuration();
  const marked = config.holdings.filter((item) => item.companyId === CONTROL_COMPANY);
  if (marked.length !== 1) return unavailable("ACCOUNT_ACCESS_READ_TARGET_UNAVAILABLE");
  const target = marked[0]!;
  return { holdingId: target.holdingId, holdingSlug: target.holdingSlug, companyId: CONTROL_COMPANY };
}

/** Test seam for the native read oracle. Production always calls the router. */
let testReader: Reader | undefined;
export function setAccountAccessReadForTest(reader?: Reader) {
  testReader = reader;
}

async function nativeRead(
  kind: string,
  values: Record<string, unknown>,
  scope: LaneHoldingScope,
  companyOverride?: string,
): Promise<unknown> {
  const body = JSON.stringify({ kind, ...values });
  if (new TextEncoder().encode(body).length > 8 * 1024)
    return unavailable("ACCOUNT_ACCESS_READ_REQUEST_INVALID", 422);
  const target = targetFor(scope);
  const companyId = companyOverride ?? scope.companyId ?? target.companyId ?? target.holdingId;
  if (!id(companyId)) return unavailable("ACCOUNT_ACCESS_READ_REQUEST_INVALID", 422);
  if (testReader) return testReader(kind, values, companyId, target);
  const config = configuration();
  const headers = new Headers({
    host: target.host,
    origin: target.origin,
    accept: "application/json",
    "content-type": "application/json",
    "cache-control": "no-cache",
    "x-erpx-context-company": companyId,
  });
  let response: Response;
  try {
    response = await fetch(
      `${config.base}/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/identity-access/account-access-read`,
      { method: "POST", headers, body, cache: "no-store", redirect: "error", signal: AbortSignal.timeout(config.timeoutMs) },
    );
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
        text(error?.code, 100) ? String(error!.code) : "ACCOUNT_ACCESS_READ_FAILED",
        text(error?.message, 300) ? String(error!.message) : "The native account access read was rejected.",
      );
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  if (!object(payload) || payload.kind !== kind || !("value" in payload))
    return unavailable("ACCOUNT_ACCESS_READ_RESPONSE_INVALID");
  return payload.value;
}

function nullableText(value: unknown): string | null {
  if (value === null) return null;
  if (!text(value, 128)) return unavailable("ACCOUNT_ACCESS_READ_RESPONSE_INVALID");
  return value;
}

function parseInvitation(value: unknown): NativeInvitation | null {
  if (value === null) return null;
  if (
    !object(value) ||
    !id(value.id) ||
    !id(value.holding_id) ||
    !id(value.company_id) ||
    !id(value.person_id) ||
    !text(value.email, 320) ||
    !text(value.name, 320) ||
    !text(value.role, 200) ||
    !["holding", "branch", "department"].includes(String(value.scope_kind)) ||
    !id(value.created_by) ||
    !id(value.assignment_id) ||
    !Number.isSafeInteger(value.expires_at) ||
    !text(value.status, 64)
  )
    return unavailable("ACCOUNT_ACCESS_READ_RESPONSE_INVALID");
  return {
    id: value.id,
    holding_id: value.holding_id,
    company_id: value.company_id,
    person_id: value.person_id,
    email: value.email,
    name: value.name,
    role: value.role,
    scope_kind: String(value.scope_kind) as 'holding' | 'branch' | 'department',
    branch_id: nullableText(value.branch_id),
    department_id: nullableText(value.department_id),
    created_by: value.created_by,
    assignment_id: value.assignment_id,
    expires_at: value.expires_at as number,
    status: value.status,
  };
}

/** Control-wide; routed to the explicitly marked Control Holding when no
 * WorkContext is available (for example the unauthenticated activation page). */
export async function nativeEmailExists(email: string, scope?: LaneHoldingScope): Promise<boolean> {
  if (!text(email, 320) || !email.includes("@"))
    return unavailable("ACCOUNT_ACCESS_READ_REQUEST_INVALID", 422);
  const value = await nativeRead("email_exists", { email }, scope ?? controlScope());
  if (typeof value !== "boolean") return unavailable("ACCOUNT_ACCESS_READ_RESPONSE_INVALID");
  return value;
}

export async function nativeInvitationStatus(
  actor: LaneHoldingScope,
  personId: string,
  invitationId: string,
): Promise<string | null> {
  if (!id(actor.holdingId) || !id(personId) || !id(invitationId))
    return unavailable("ACCOUNT_ACCESS_READ_REQUEST_INVALID", 422);
  const value = await nativeRead(
    "invitation_status",
    { holding_id: actor.holdingId, person_id: personId, invitation_id: invitationId },
    actor,
    actor.companyId,
  );
  return nullableText(value);
}

export async function nativeInvitationByToken(
  tokenHash: string,
  now: number,
  scope?: LaneHoldingScope,
): Promise<NativeInvitation | null> {
  if (!text(tokenHash, 128) || !/^[a-f0-9_-]{32,128}$/i.test(tokenHash) || !Number.isSafeInteger(now) || now < 1_000_000_000_000)
    return unavailable("ACCOUNT_ACCESS_READ_REQUEST_INVALID", 422);
  return parseInvitation(
    await nativeRead("invitation_by_token", { token_hash: tokenHash, now }, scope ?? controlScope()),
  );
}

/** True when the native lane 10 consumer switch is configured. */
export function nativeAccountAccessReadConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
