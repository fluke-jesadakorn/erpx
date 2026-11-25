import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { Entry, GuestAccount } from "@erpx/server-pms/domain/pms-model";

/**
 * Strict private transport for the lane 19 native PMS evidence read. The axum
 * owner executes the statement, scopes the Company/Branch and returns the
 * projected response. This bridge emits no SQL, accepts no SQL from a caller,
 * and fails closed with 503 when the owner is unavailable, the registered
 * Holding is unknown or a response is malformed. There is no TypeScript
 * fallback and no default Holding: a target must be selected from the caller.
 */
export type NativeLane19Evidence = Readonly<{
  id: string;
  propertyId: string;
  hotel: string;
  businessDate: string;
  currency: string;
  account: GuestAccount;
  entries: readonly Readonly<{ id: string; journalId: string | null; data: Entry }>[];
}>;

export type NativeLane19Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
export type NativeLane19Config = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly NativeLane19Holding[];
}>;
export type NativeLane19Actor = Pick<
  WorkContext,
  "holdingId" | "holdingSlug" | "companyId" | "branchId"
> & { origin?: string };

type Settings = Readonly<{ base: string; timeoutMs: number; holdings: readonly NativeLane19Holding[] }>;
type Fetch = (input: string | URL, init?: RequestInit) => Promise<Response>;

const ENV = "ERPX_NATIVE_LANE_19_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const MAX_ENTRIES = 5000;

const unavailable = (
  code = "PMS_NATIVE_LANE_19_UNAVAILABLE",
  message = "The native PMS evidence service is unavailable.",
): never => {
  throw new ApiError(503, code, message);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 200): value is string =>
  typeof value === "string" && value.length > 0 && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown, max = 200): value is string =>
  text(value, max) && !/[\u0000-\u001f/?#\\]/.test(value);
const slug = (value: unknown): value is string =>
  text(value, 128) && /^[a-z0-9_-]+$/.test(value);
const hostname = (value: unknown): value is string =>
  text(value, 253) && /^[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?$/.test(value);

export function nativeLane19Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}

export function parseNativeLane19Config(value: unknown): Settings {
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
  )
    return unavailable("PMS_NATIVE_LANE_19_CONFIGURATION_INVALID");
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable("PMS_NATIVE_LANE_19_CONFIGURATION_INVALID");
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
    return unavailable("PMS_NATIVE_LANE_19_CONFIGURATION_INVALID");

  const holdings: NativeLane19Holding[] = [];
  const ids = new Set<string>();
  const origins = new Set<string>();
  const targets = new Set<string>();
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.origin, 512) ||
      !hostname(item.host) ||
      !identifier(item.holdingId) ||
      !slug(item.holdingSlug) ||
      (item.companyId !== undefined && !identifier(item.companyId))
    )
      return unavailable("PMS_NATIVE_LANE_19_CONFIGURATION_INVALID");
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return unavailable("PMS_NATIVE_LANE_19_CONFIGURATION_INVALID");
    }
    if (
      origin.protocol !== "https:" ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      // The Host header must be the exact registered Holding host, never a
      // value that disagrees with the origin it claims to represent.
      origin.host !== item.host ||
      ids.has(item.holdingId) ||
      origins.has(origin.origin) ||
      targets.has(`${item.host}\u0000${item.holdingSlug}`)
    )
      return unavailable("PMS_NATIVE_LANE_19_CONFIGURATION_INVALID");
    ids.add(item.holdingId);
    origins.add(origin.origin);
    targets.add(`${item.host}\u0000${item.holdingSlug}`);
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

/** Pick exactly one registered Holding from the caller. No first-holding
 * default is permitted, and two selectors must resolve to the same target. */
function selectTarget(settings: Settings, actor: NativeLane19Actor): NativeLane19Holding {
  const invalidRequest = () =>
    unavailable("PMS_EVIDENCE_REQUEST_INVALID", "Use one bounded PMS guest-account evidence request.");
  const unknown = () => unavailable("PMS_NATIVE_LANE_19_TARGET_UNAVAILABLE");

  let byOrigin: NativeLane19Holding | undefined;
  if (actor.origin !== undefined) {
    if (!text(actor.origin, 512)) return invalidRequest();
    let origin: URL;
    try {
      origin = new URL(actor.origin);
    } catch {
      return invalidRequest();
    }
    if (origin.origin !== actor.origin || origin.protocol !== "https:") return invalidRequest();
    byOrigin = settings.holdings.find((holding) => holding.origin === origin.origin);
  }

  let byIdentity: NativeLane19Holding | undefined;
  if (actor.holdingId !== undefined || actor.holdingSlug !== undefined) {
    if (!identifier(actor.holdingId) || !slug(actor.holdingSlug)) return invalidRequest();
    byIdentity = settings.holdings.find(
      (holding) => holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
    );
  }

  if (!byOrigin && !byIdentity) return unknown();
  if (byOrigin && byIdentity && byOrigin !== byIdentity) return unknown();
  const target = byOrigin ?? byIdentity!;
  if (!identifier(actor.companyId)) return invalidRequest();
  if (target.companyId !== undefined && target.companyId !== actor.companyId)
    return unavailable("PMS_NATIVE_LANE_19_COMPANY_MISMATCH");
  return target;
}

function validateEvidence(payload: unknown): NativeLane19Evidence {
  if (!record(payload) || !record(payload.evidence)) return unavailable("PMS_NATIVE_LANE_19_RESPONSE_INVALID");
  const evidence = payload.evidence;
  if (
    !identifier(evidence.id) ||
    !identifier(evidence.propertyId) ||
    !text(evidence.hotel, 512) ||
    !text(evidence.businessDate, 32) ||
    !text(evidence.currency, 16) ||
    !record(evidence.account) ||
    !Array.isArray(evidence.entries) ||
    evidence.entries.length > MAX_ENTRIES
  )
    return unavailable("PMS_NATIVE_LANE_19_RESPONSE_INVALID");
  const entries = evidence.entries.map((item: unknown) => {
    if (
      !record(item) ||
      !identifier(item.id) ||
      !(item.journalId === null || identifier(item.journalId)) ||
      !record(item.data)
    )
      return unavailable("PMS_NATIVE_LANE_19_RESPONSE_INVALID");
    return {
      id: item.id,
      journalId: item.journalId,
      data: item.data as unknown as Entry,
    };
  });
  return {
    id: evidence.id,
    propertyId: evidence.propertyId,
    hotel: evidence.hotel,
    businessDate: evidence.businessDate,
    currency: evidence.currency,
    account: evidence.account as unknown as GuestAccount,
    entries,
  };
}

/** Create a client with an injectable transport for contract tests. */
export function createNativeLane19Client(config: unknown, fetchImpl: Fetch = fetch) {
  const settings = parseNativeLane19Config(config);
  return {
    async guestAccountFinanceEvidence(
      actor: NativeLane19Actor,
      id: string,
    ): Promise<NativeLane19Evidence> {
      if (!identifier(id))
        return unavailable("PMS_EVIDENCE_REQUEST_INVALID", "Use one bounded PMS guest-account evidence request.");
      const target = selectTarget(settings, actor);
      const headers = new Headers({
        host: target.host,
        origin: target.origin,
        accept: "application/json",
        "content-type": "application/json",
        "cache-control": "no-cache",
        "x-erpx-context-company": actor.companyId,
        "x-erpx-context-record": id,
      });
      if (actor.branchId) {
        if (!identifier(actor.branchId))
          return unavailable("PMS_EVIDENCE_REQUEST_INVALID", "Use one bounded PMS guest-account evidence request.");
        headers.set("x-erpx-context-branch", actor.branchId);
      }
      const url = new URL(
        `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/pms/guest-account-finance-evidence`,
        settings.base,
      );
      let response: Response;
      try {
        response = await fetchImpl(url, {
          method: "GET",
          headers: Object.fromEntries(headers),
          cache: "no-store",
          redirect: "error",
          signal: AbortSignal.timeout(settings.timeoutMs),
        });
      } catch {
        return unavailable();
      }
      if (response.status !== 200) {
        const payload = await response.json().catch((): unknown => null);
        const error = record(payload) && record(payload.error) ? payload.error : undefined;
        if (response.status >= 400 && response.status < 500)
          throw new ApiError(
            response.status,
            text(error?.code, 100) ? String(error!.code) : "PMS_NATIVE_LANE_19_FAILED",
            text(error?.message, 300) ? String(error!.message) : "The native PMS evidence read was rejected.",
          );
        return unavailable();
      }
      const payload = await response.json().catch((): unknown => null);
      return validateEvidence(payload);
    },
  };
}

/** Env-bound entry point used by the owner adapter. */
export async function nativeLane19GuestAccountEvidence(
  actor: NativeLane19Actor,
  id: string,
): Promise<NativeLane19Evidence> {
  const raw = process.env[ENV];
  if (!raw) return unavailable("PMS_NATIVE_LANE_19_CONFIGURATION_REQUIRED");
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return unavailable("PMS_NATIVE_LANE_19_CONFIGURATION_INVALID");
  }
  return createNativeLane19Client(parsed).guestAccountFinanceEvidence(actor, id);
}
