import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { nativeRequest } from "@erpx/platform-runtime-server/public/native-http";

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

export type DirectoryKind = "connections" | "groups" | "previews" | "identities";
const DIRECTORY_KINDS: readonly DirectoryKind[] = ["connections", "groups", "previews", "identities"];
const MAX_DIRECTORY_ROWS = 500;

export type NativeRoleRevision = {
  id: string;
  name: string;
  companyId: string;
  revision: number;
  builtIn: boolean;
  permissions: string[];
  grantablePermissions: string[];
  scopes: string[];
  status: string;
};

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
  directory(
    actor: NativeLane11Actor | undefined,
    kind: DirectoryKind,
    canManage: boolean,
    companyId: string | undefined,
  ): Promise<Record<string, unknown>[]>;
  role(
    actor: NativeLane11Actor | undefined,
    id: string,
    companyId: string | undefined,
  ): Promise<NativeRoleRevision | null>;
  roles(
    actor: NativeLane11Actor | undefined,
    ids: readonly string[],
    companyId: string | undefined,
  ): Promise<NativeRoleRevision[]>;
  roleIds(
    actor: NativeLane11Actor | undefined,
    companyId: string | undefined,
  ): Promise<string[]>;
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
  let response: Response;
  try {
    response = await nativeRequest(
      new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/identity-access/${path}`, config.base),
      {
        method: "POST",
        headers: {
          host: holding.host,
          origin: holding.origin,
          accept: "application/json",
          "content-type": "application/json",
          "cache-control": "no-cache",
          "x-erpx-context-company": companyId,
        },
        body: JSON.stringify(body),
      },
      config.timeoutMs,
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
  return parseIdentityIds(await post(config, target.holding, target.companyId, "person/identity-ids", { ids }));
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
    await post(config, target.holding, target.companyId, "person/principals", { ids, snapshot }),
  );
}

function parseDirectoryRows(value: unknown): Record<string, unknown>[] {
  if (!object(value)) return unavailable("The native directory reply was invalid.");
  const rows = value.rows;
  if (!Array.isArray(rows) || rows.length > MAX_DIRECTORY_ROWS || rows.some((row) => !object(row) || !bounded(row.id)))
    return unavailable("The native directory reply was invalid.");
  return rows as Record<string, unknown>[];
}

function strings(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => text(item, 1000));
}

function parseRole(value: unknown): NativeRoleRevision {
  if (
    !object(value) ||
    !bounded(value.id) ||
    !text(value.name, 200) ||
    !text(value.companyId, 200) ||
    !Number.isSafeInteger(value.revision) ||
    typeof value.builtIn !== "boolean" ||
    !strings(value.permissions) ||
    !strings(value.grantablePermissions) ||
    !strings(value.scopes) ||
    !text(value.status, 100)
  )
    return unavailable("The native role reply was invalid.");
  return {
    id: value.id,
    name: value.name,
    companyId: value.companyId,
    revision: value.revision as number,
    builtIn: value.builtIn,
    permissions: value.permissions,
    grantablePermissions: value.grantablePermissions,
    scopes: value.scopes,
    status: value.status,
  };
}

/**
 * Read the four owner-local directory configuration projections. A
 * WorkContext selects the Holding; callers without one (the IAM overview
 * actor has no holding slug) use the legacy Company-to-Holding resolution.
 */
export async function nativeDirectoryConfig(
  actor: NativeLane11Actor | undefined,
  kind: DirectoryKind,
  canManage: boolean,
  legacyCompanyId?: string,
): Promise<Record<string, unknown>[]> {
  if (!DIRECTORY_KINDS.includes(kind) || typeof canManage !== "boolean")
    throw new ApiError(422, "IDENTITY_ACCESS_NATIVE_LANE_11_REQUEST_INVALID", "Use a bounded directory read.");
  if (testReader) return testReader.directory(actor, kind, canManage, legacyCompanyId);
  const config = configuration();
  const target = selectTarget(config, actor, legacyCompanyId);
  const payload = await post(config, target.holding, target.companyId, `directory/${kind}`, {
    canManage,
  });
  return parseDirectoryRows(payload);
}

/** Read one published role definition, or null when no active revision exists. */
export async function nativeRole(
  actor: NativeLane11Actor | undefined,
  id: string,
  legacyCompanyId?: string,
): Promise<NativeRoleRevision | null> {
  if (!bounded(id))
    throw new ApiError(422, "IDENTITY_ACCESS_NATIVE_LANE_11_REQUEST_INVALID", "Use a bounded role id.");
  if (testReader) return testReader.role(actor, id, legacyCompanyId);
  const config = configuration();
  const target = selectTarget(config, actor, legacyCompanyId);
  const payload = await post(config, target.holding, target.companyId, `role/${encodeURIComponent(id)}`, {});
  if (!object(payload) || !("role" in payload))
    return unavailable("The native role reply was invalid.");
  return payload.role === null ? null : parseRole(payload.role);
}

/** Read published definitions for a bounded role id list, first row per id. */
export async function nativeRoles(
  actor: NativeLane11Actor | undefined,
  ids: readonly string[],
  legacyCompanyId?: string,
): Promise<NativeRoleRevision[]> {
  validateIds(ids);
  if (testReader) return testReader.roles(actor, ids, legacyCompanyId);
  const config = configuration();
  const target = selectTarget(config, actor, legacyCompanyId);
  const payload = await post(config, target.holding, target.companyId, "roles", { ids });
  if (!object(payload) || !Array.isArray(payload.roles) || payload.roles.length > MAX_IDS)
    return unavailable("The native role reply was invalid.");
  const seen = new Set<string>();
  const roles: NativeRoleRevision[] = [];
  for (const row of payload.roles) {
    const role = parseRole(row);
    if (!seen.has(role.id)) {
      seen.add(role.id);
      roles.push(role);
    }
  }
  return roles;
}

/** Read the published active role ids in their owner-defined order. */
export async function nativeRoleIds(
  actor: NativeLane11Actor | undefined,
  legacyCompanyId?: string,
): Promise<string[]> {
  if (testReader) return testReader.roleIds(actor, legacyCompanyId);
  const config = configuration();
  const target = selectTarget(config, actor, legacyCompanyId);
  const payload = await post(config, target.holding, target.companyId, "role-ids", {});
  if (!object(payload) || !Array.isArray(payload.ids) || payload.ids.length > MAX_IDS || payload.ids.some((id) => !bounded(id)))
    return unavailable("The native role reply was invalid.");
  return payload.ids as string[];
}

/** True only when the native lane 11 consumer switch is configured. */
export function nativeLane11Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
