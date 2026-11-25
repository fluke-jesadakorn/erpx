import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError } from "@erpx/server-runtime/public/http";

/**
 * Strict private transport for the native lane 11 person-identity reads.
 *
 * The owner axum service at `native_lane_11` owns the two `iam_members`
 * statements, the alias/successor projection, the Company bind and the wire
 * shape. This bridge emits no SQL, accepts no SQL from a caller and never
 * falls back to a database read. A Holding is selected only by the caller's
 * WorkContext `holdingId` + `holdingSlug`; there is no first-holding default.
 * Missing or unknown targets, malformed replies and failed requests fail
 * closed with an explicit 503.
 */
export type NativeLane11Actor = Pick<WorkContext, "holdingId" | "holdingSlug" | "companyId">;

type LaneHolding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;

type NativeConfig = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly LaneHolding[];
}>;

export type NativePersonIdentityReader = Readonly<{
  identityIds(
    actor: NativeLane11Actor | undefined,
    ids: readonly string[],
    companyId: string | undefined,
  ): Promise<string[]>;
  principals(
    actor: NativeLane11Actor | undefined,
    ids: readonly string[],
    snapshot: boolean,
    companyId: string | undefined,
  ): Promise<Map<string, string>>;
}>;

const ENV = "ERPX_NATIVE_IDENTITY_ACCESS_LANE_11_CONFIG";
const MAX_IDS = 200;
const MAX_OUTPUT = 1000;
const CODE = "IDENTITY_ACCESS_NATIVE_LANE_11_UNAVAILABLE";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const unavailable = (message = "The native person identity service is unavailable."): never => {
  throw new ApiError(503, CODE, message);
};

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === "string" && new TextEncoder().encode(value).length <= max;

function configuration(): NativeConfig {
  const raw = process.env[ENV];
  if (raw === undefined) return unavailable("The native person identity transport is not configured.");
  let value: unknown;
  try {
    value = JSON.parse(raw) as unknown;
  } catch {
    return unavailable("The native person identity transport is misconfigured.");
  }
  if (!object(value) || !text(value.privateBaseUrl, 512)) return unavailable("The native person identity transport is misconfigured.");
  if (
    !Number.isSafeInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable("The native person identity transport is misconfigured.");
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable("The native person identity transport is misconfigured.");
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
    return unavailable("The native person identity transport is misconfigured.");

  const holdings: LaneHolding[] = [];
  const pairs = new Set<string>();
  const routes = new Set<string>();
  for (const item of value.holdings) {
    if (
      !object(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !text(item.holdingId, 200) ||
      !text(item.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(item.holdingSlug) ||
      (item.companyId !== undefined && !text(item.companyId, 200))
    )
      return unavailable("The native person identity transport is misconfigured.");
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return unavailable("The native person identity transport is misconfigured.");
    }
    if (
      origin.protocol !== "https:" ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      origin.host !== item.host
    )
      return unavailable("The native person identity transport is misconfigured.");
    const targetPair = `${item.holdingId}\u0000${item.holdingSlug}`;
    const routePair = `${item.host}\u0000${item.holdingSlug}`;
    if (pairs.has(targetPair) || routes.has(routePair))
      return unavailable("The native person identity transport is misconfigured.");
    pairs.add(targetPair);
    routes.add(routePair);
    holdings.push({
      origin: origin.origin,
      host: item.host,
      holdingId: item.holdingId,
      holdingSlug: item.holdingSlug,
      ...(item.companyId !== undefined ? { companyId: item.companyId } : {}),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs as number, holdings };
}

type ResolvedTarget = Readonly<{ holding: LaneHolding; companyId: string }>;

function validCompanyId(value: unknown): value is string {
  return text(value, 200) && /^[A-Za-z0-9_.:-]+$/.test(value);
}

/**
 * Select the native target.
 *
 * With a WorkContext the Holding must match `holdingId` + `holdingSlug` and
 * any configured company pin must agree (409 otherwise). Without one, the
 * legacy caller supplies only a Company id: exactly one registered Holding may
 * declare that `companyId`. Zero or multiple matches fail closed; there is no
 * first-Holding default and no database fallback.
 */
function selectTarget(
  config: NativeConfig,
  actor: NativeLane11Actor | undefined,
  legacyCompanyId: string | undefined,
): ResolvedTarget {
  if (actor !== undefined) {
    if (
      !text(actor.holdingId, 200) ||
      !text(actor.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(actor.holdingSlug)
    )
      return unavailable("The native person identity target is unknown.");
    const holding = config.holdings.find(
      (item) => item.holdingId === actor.holdingId && item.holdingSlug === actor.holdingSlug,
    );
    if (!holding) return unavailable("The native person identity target is unknown.");
    if (holding.companyId !== undefined && holding.companyId !== actor.companyId)
      throw new ApiError(409, "WORK_CONTEXT_CHANGED", "The selected Company changed. Reload and retry.");
    if (!validCompanyId(actor.companyId))
      throw new ApiError(422, "IDENTITY_ACCESS_NATIVE_LANE_11_REQUEST_INVALID", "Use a bounded Company.");
    return { holding, companyId: actor.companyId };
  }
  if (!validCompanyId(legacyCompanyId))
    throw new ApiError(422, "IDENTITY_ACCESS_NATIVE_LANE_11_REQUEST_INVALID", "Use a bounded Company.");
  const matches = config.holdings.filter((item) => item.companyId === legacyCompanyId);
  if (matches.length !== 1) return unavailable("The native person identity target is unknown.");
  return { holding: matches[0], companyId: legacyCompanyId };
}

function bounded(value: unknown, max = 200): value is string {
  return (
    typeof value === "string" &&
    value.length > 0 &&
    value.length <= max &&
    !/[\u0000-\u001f\u007f]/.test(value)
  );
}

function validateIds(ids: readonly string[]): void {
  if (!Array.isArray(ids) || ids.length < 1 || ids.length > MAX_IDS || ids.some((id) => !bounded(id)))
    throw new ApiError(422, "IDENTITY_ACCESS_NATIVE_LANE_11_REQUEST_INVALID", "Use a bounded identity list.");
}

async function post(
  config: NativeConfig,
  holding: LaneHolding,
  companyId: string,
  path: string,
  body: unknown,
): Promise<unknown> {
  const headers = new Headers({
    host: holding.host,
    accept: "application/json",
    "content-type": "application/json",
    "cache-control": "no-cache",
    "x-erpx-context-company": companyId,
  });
  let response: Response;
  try {
    response = await fetch(
      `${config.base}/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/identity-access/person/${path}`,
      {
        method: "POST",
        headers,
        body: JSON.stringify(body),
        cache: "no-store",
        redirect: "error",
        signal: AbortSignal.timeout(config.timeoutMs),
      },
    );
  } catch {
    return unavailable();
  }
  const bodyText = await response.text().catch(() => "");
  if (new TextEncoder().encode(bodyText).length > 1024 * 1024)
    return unavailable("The native person identity reply was invalid.");
  let payload: unknown;
  try {
    payload = JSON.parse(bodyText) as unknown;
  } catch {
    return unavailable("The native person identity reply was invalid.");
  }
  if (!response.ok) {
    const error = object(payload) && object(payload.error) ? payload.error : undefined;
    if (response.status >= 400 && response.status < 500)
      throw new ApiError(
        response.status,
        text(error?.code, 100) && /^[A-Z0-9_]{3,80}$/.test(String(error!.code))
          ? String(error!.code)
          : "IDENTITY_ACCESS_NATIVE_LANE_11_FAILED",
        text(error?.message, 1000) ? String(error!.message) : "The native person identity read was rejected.",
      );
    return unavailable();
  }
  return payload;
}

function parseIdentityIds(value: unknown): string[] {
  if (!object(value)) return unavailable("The native person identity reply was invalid.");
  const ids = value.identityIds;
  if (!Array.isArray(ids) || ids.length < 1 || ids.length > MAX_OUTPUT || ids.some((id) => !bounded(id)))
    return unavailable("The native person identity reply was invalid.");
  return ids as string[];
}

function parsePrincipals(value: unknown): Map<string, string> {
  if (!object(value)) return unavailable("The native person identity reply was invalid.");
  const pairs = value.principals;
  if (!Array.isArray(pairs) || pairs.length < 1 || pairs.length > MAX_IDS)
    return unavailable("The native person identity reply was invalid.");
  const map = new Map<string, string>();
  for (const pair of pairs) {
    if (!object(pair) || !bounded(pair.original) || !bounded(pair.current) || map.has(pair.original))
      return unavailable("The native person identity reply was invalid.");
    map.set(pair.original, pair.current);
  }
  return map;
}

/** Test seam for the owner contract oracle. Production always uses native. */
let testReader: NativePersonIdentityReader | undefined;
export function setNativeLane11ForTest(reader?: NativePersonIdentityReader): void {
  testReader = reader;
}

export async function nativeIdentityIds(
  actor: NativeLane11Actor | undefined,
  ids: readonly string[],
  legacyCompanyId?: string,
): Promise<string[]> {
  validateIds(ids);
  if (testReader) return testReader.identityIds(actor, ids, legacyCompanyId);
  const config = configuration();
  const target = selectTarget(config, actor, legacyCompanyId);
  return parseIdentityIds(await post(config, target.holding, target.companyId, "identity-ids", { ids }));
}

export async function nativePrincipals(
  actor: NativeLane11Actor | undefined,
  ids: readonly string[],
  snapshot: boolean,
  legacyCompanyId?: string,
): Promise<Map<string, string>> {
  validateIds(ids);
  if (typeof snapshot !== "boolean")
    throw new ApiError(422, "IDENTITY_ACCESS_NATIVE_LANE_11_REQUEST_INVALID", "Use a bounded snapshot flag.");
  if (testReader) return testReader.principals(actor, ids, snapshot, legacyCompanyId);
  const config = configuration();
  const target = selectTarget(config, actor, legacyCompanyId);
  return parsePrincipals(
    await post(config, target.holding, target.companyId, "principals", { ids, snapshot }),
  );
}

/** True only when the native lane 11 consumer switch is configured. */
export function nativeLane11Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
