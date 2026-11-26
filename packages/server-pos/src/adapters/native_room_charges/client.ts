import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";
import { holdingResources } from "@erpx/server-runtime/public/runtime";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import {
  posPackageEnforcementMode,
  readPurchasedAccesses,
} from "@erpx/server-commercial/public/purchase-command-boundary";

/**
 * Strict private transport for the mounted native POS room-charge reversal
 * queue read. Rust owns the SQL, the projection, the scope and the response
 * shape; this bridge sends no SQL and accepts no SQL from a caller. Missing
 * configuration or any rejected/invalid native reply fails closed with no
 * TypeScript fallback.
 */
export type RoomChargeReviewRow = Readonly<{
  id: string;
  company_id: string;
  branch_id: string;
  sale_id: string;
  property_id: string;
  sale_version: number;
  property_version: number;
  restock_line_ids_json: string;
  number: string;
  state: "requested" | "pos_approved";
  version: number;
  reason: string;
  amount_minor: number;
  requested_by: string;
  pos_approved_by: string | null;
  created_by: string;
  charged_by: string | null;
  updated_at: number;
  source_current: boolean;
  stage: "pos" | "pms";
  canAct: boolean;
  canApprove: boolean;
  reasonDisabled: string | null;
}>;

type Holding = Readonly<{ origin: string; host: string; holdingId: string; holdingSlug: string }>;
type Settings = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;

const ENV = "ERPX_NATIVE_POS_CASH_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const ROW_KEYS = [
  "id", "company_id", "branch_id", "sale_id", "property_id", "sale_version",
  "property_version", "restock_line_ids_json", "number", "state", "version",
  "reason", "amount_minor", "requested_by", "pos_approved_by", "created_by",
  "charged_by", "updated_at", "source_current", "stage", "canAct", "canApprove",
  "reasonDisabled",
] as const;

const unavailable = (code = "NATIVE_POS_ROOM_CHARGES_UNAVAILABLE"): never => {
  throw new ApiError(503, code, "The native POS room-charge read is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 4096): value is string =>
  typeof value === "string" && new TextEncoder().encode(value).length <= max;
const integer = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value);
const safeId = (value: unknown, max = 200): value is string =>
  text(value, max) && /^[A-Za-z0-9:_.-]+$/.test(String(value));

function configuration(): Settings {
  const raw = process.env[ENV];
  if (raw === undefined) return unavailable("NATIVE_POS_ROOM_CHARGES_CONFIGURATION_REQUIRED");
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable("NATIVE_POS_ROOM_CHARGES_CONFIGURATION_INVALID");
  }
  if (
    !object(value) ||
    !text(value.privateBaseUrl, 512) ||
    !integer(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable("NATIVE_POS_ROOM_CHARGES_CONFIGURATION_INVALID");
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl as string);
  } catch {
    return unavailable("NATIVE_POS_ROOM_CHARGES_CONFIGURATION_INVALID");
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
    return unavailable("NATIVE_POS_ROOM_CHARGES_CONFIGURATION_INVALID");
  const holdings: Holding[] = [];
  const hosts = new Set<string>();
  const slugs = new Set<string>();
  for (const item of value.holdings) {
    if (
      !object(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 253) ||
      !safeId(item.holdingId) ||
      !safeId(item.holdingSlug, 128)
    )
      return unavailable("NATIVE_POS_ROOM_CHARGES_CONFIGURATION_INVALID");
    let origin: URL;
    try {
      origin = new URL(item.origin as string);
    } catch {
      return unavailable("NATIVE_POS_ROOM_CHARGES_CONFIGURATION_INVALID");
    }
    if (
      origin.protocol !== "https:" ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      origin.host !== item.host ||
      hosts.has(item.host as string) ||
      slugs.has(item.holdingSlug as string)
    )
      return unavailable("NATIVE_POS_ROOM_CHARGES_CONFIGURATION_INVALID");
    hosts.add(item.host as string);
    slugs.add(item.holdingSlug as string);
    holdings.push({
      origin: origin.origin,
      host: item.host as string,
      holdingId: item.holdingId as string,
      holdingSlug: item.holdingSlug as string,
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs as number, holdings };
}

/** Select the actor's exact registered Holding. Never falls back to holdings[0]. */
function target(config: Settings, actor: WorkContext): Holding {
  const choice = config.holdings.find(
    (holding) => holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
  );
  if (!choice) return unavailable("NATIVE_POS_ROOM_CHARGES_TARGET_UNAVAILABLE");
  return choice;
}

/** Strictly validate one projected row; unknown keys are rejected so the
 * response shape cannot drift from the Rust owner. */
function row(value: unknown, actor: WorkContext, propertyId?: string): RoomChargeReviewRow {
  if (!object(value)) return unavailable("NATIVE_POS_ROOM_CHARGES_RESPONSE_INVALID");
  if (Object.keys(value).length !== ROW_KEYS.length || !ROW_KEYS.every((key) => Object.hasOwn(value, key)))
    return unavailable("NATIVE_POS_ROOM_CHARGES_RESPONSE_INVALID");
  const idKeys = [
    "id", "company_id", "branch_id", "sale_id", "property_id", "restock_line_ids_json", "number", "reason", "requested_by", "created_by",
  ] as const;
  const intKeys = [
    "sale_version", "property_version", "version", "amount_minor", "updated_at",
  ] as const;
  if (!idKeys.every((key) => text(value[key], 8192))) return unavailable("NATIVE_POS_ROOM_CHARGES_RESPONSE_INVALID");
  if (!intKeys.every((key) => integer(value[key]))) return unavailable("NATIVE_POS_ROOM_CHARGES_RESPONSE_INVALID");
  if (value.company_id !== actor.companyId) return unavailable("NATIVE_POS_ROOM_CHARGES_RESPONSE_INVALID");
  if (propertyId !== undefined && value.property_id !== propertyId)
    return unavailable("NATIVE_POS_ROOM_CHARGES_RESPONSE_INVALID");
  if (value.state !== "requested" && value.state !== "pos_approved")
    return unavailable("NATIVE_POS_ROOM_CHARGES_RESPONSE_INVALID");
  if (value.stage !== "pos" && value.stage !== "pms")
    return unavailable("NATIVE_POS_ROOM_CHARGES_RESPONSE_INVALID");
  if (typeof value.source_current !== "boolean" || typeof value.canAct !== "boolean" || typeof value.canApprove !== "boolean")
    return unavailable("NATIVE_POS_ROOM_CHARGES_RESPONSE_INVALID");
  const nullableText = (key: "pos_approved_by" | "charged_by" | "reasonDisabled") => {
    const item = value[key];
    return item === null || text(item, 8192) ? (item as string | null) : unavailable("NATIVE_POS_ROOM_CHARGES_RESPONSE_INVALID");
  };
  return {
    id: value.id as string,
    company_id: value.company_id as string,
    branch_id: value.branch_id as string,
    sale_id: value.sale_id as string,
    property_id: value.property_id as string,
    sale_version: value.sale_version as number,
    property_version: value.property_version as number,
    restock_line_ids_json: value.restock_line_ids_json as string,
    number: value.number as string,
    state: value.state as "requested" | "pos_approved",
    version: value.version as number,
    reason: value.reason as string,
    amount_minor: value.amount_minor as number,
    requested_by: value.requested_by as string,
    pos_approved_by: nullableText("pos_approved_by"),
    created_by: value.created_by as string,
    charged_by: nullableText("charged_by"),
    updated_at: value.updated_at as number,
    source_current: value.source_current as boolean,
    stage: value.stage as "pos" | "pms",
    canAct: value.canAct as boolean,
    canApprove: value.canApprove as boolean,
    reasonDisabled: nullableText("reasonDisabled"),
  };
}

/** Read the complete room-charge review queue from the mounted native owner.
 * The actor and package facts are transported as bounded decisions; the Rust
 * owner owns every gate, scope and derivation. No SQL fallback exists. */
export async function nativeRoomChargeQueue(
  actor: WorkContext,
  propertyId?: string,
): Promise<RoomChargeReviewRow[]> {
  if (propertyId !== undefined && !safeId(propertyId) && propertyId !== "")
    throw new ApiError(422, "NATIVE_POS_ROOM_CHARGES_REQUEST_INVALID", "Use one bounded property filter.");
  const config = configuration();
  const holding = target(config, actor);
  const mode = await posPackageEnforcementMode();
  const access = mode === "disabled"
    ? undefined
    : await readPurchasedAccesses(
        (await holdingResources(actor.holdingId)).write,
        mode,
        actor.companyId,
        ["pos.hotel-charge", "pms.guest-accounts", "finance.hotel-posting"],
      );
  const decisions = {
    actor: {
      posApprove: hasPermission(actor, "pos.approve"),
      pmsApprove: hasPermission(actor, "pms.approve"),
      department: Boolean(actor.departmentId),
      selfIds: [actor.userId, ...(actor.iam?.personUserIds ?? [])],
    },
    package: {
      enforced: mode !== "disabled",
      history: !access || Object.values(access).every((value) => value.history),
      settleExisting: !access || Object.values(access).every((value) => value.settleExisting),
    },
  };
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/pos/room-charge-reversals`,
    config.base,
  );
  if (propertyId !== undefined) url.searchParams.set("property", propertyId);
  const headers = new Headers({
    host: holding.host,
    origin: holding.origin,
    accept: "application/json",
    "cache-control": "no-cache",
    "x-erpx-context-company": actor.companyId,
    "x-erpx-pos-room-actor": JSON.stringify(decisions.actor),
    "x-erpx-pos-room-package": JSON.stringify(decisions.package),
  });
  if (actor.branchId) headers.set("x-erpx-workspace-branch", actor.branchId);
  let response: Response;
  try {
    response = await fetch(url, {
      method: "GET",
      headers: Object.fromEntries(headers),
      cache: "no-store",
      redirect: "error",
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    return unavailable();
  }
  const body: unknown = await response.json().catch(() => undefined);
  if (!response.ok) {
    if (object(body) && object(body.error) && typeof body.error.message === "string")
      throw new ApiError(response.status, String(body.error.code ?? "NATIVE_POS_ROOM_CHARGES_UNAVAILABLE"), body.error.message);
    return unavailable();
  }
  if (!object(body) || !Array.isArray(body.data)) return unavailable("NATIVE_POS_ROOM_CHARGES_RESPONSE_INVALID");
  return body.data.map((item) => row(item, actor, propertyId));
}
