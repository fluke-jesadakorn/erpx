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
 * Strict private transport for the mounted native POS customer-search read.
 * Rust owns the SQL, the Company scope, the authority/package gates and the
 * response shape; this bridge sends no SQL and accepts no SQL from a caller.
 * Missing configuration or any rejected/invalid native reply fails closed with
 * no TypeScript fallback.
 */
export type PosCustomerRow = Readonly<{
  id: string;
  name: string;
  phone: string | null;
  notification_email: string | null;
  address_json: string;
  version: number;
}>;

type Holding = Readonly<{ origin: string; host: string; holdingId: string; holdingSlug: string }>;
type Settings = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;

const ENV = "ERPX_NATIVE_POS_CASH_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const ROW_KEYS = ["id", "name", "phone", "notification_email", "address_json", "version"] as const;

const unavailable = (code = "NATIVE_POS_CUSTOMERS_UNAVAILABLE"): never => {
  throw new ApiError(503, code, "The native POS customer read is unavailable.");
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === "string" && new TextEncoder().encode(value).length <= max;
const integer = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 1;
const safeId = (value: unknown, max = 200): value is string =>
  text(value, max) && /^[A-Za-z0-9:_.-]+$/.test(String(value));

function configuration(): Settings {
  const raw = process.env[ENV];
  if (raw === undefined) return unavailable("NATIVE_POS_CUSTOMERS_CONFIGURATION_REQUIRED");
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable("NATIVE_POS_CUSTOMERS_CONFIGURATION_INVALID");
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
    return unavailable("NATIVE_POS_CUSTOMERS_CONFIGURATION_INVALID");
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl as string);
  } catch {
    return unavailable("NATIVE_POS_CUSTOMERS_CONFIGURATION_INVALID");
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
    return unavailable("NATIVE_POS_CUSTOMERS_CONFIGURATION_INVALID");
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
      return unavailable("NATIVE_POS_CUSTOMERS_CONFIGURATION_INVALID");
    let origin: URL;
    try {
      origin = new URL(item.origin as string);
    } catch {
      return unavailable("NATIVE_POS_CUSTOMERS_CONFIGURATION_INVALID");
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
      return unavailable("NATIVE_POS_CUSTOMERS_CONFIGURATION_INVALID");
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
  if (!choice) return unavailable("NATIVE_POS_CUSTOMERS_TARGET_UNAVAILABLE");
  return choice;
}

/** Strictly validate one projected row; unknown keys are rejected so the
 * response shape cannot drift from the Rust owner. */
function row(value: unknown): PosCustomerRow {
  if (!object(value)) return unavailable("NATIVE_POS_CUSTOMERS_RESPONSE_INVALID");
  if (
    Object.keys(value).length !== ROW_KEYS.length ||
    !ROW_KEYS.every((key) => Object.hasOwn(value, key))
  )
    return unavailable("NATIVE_POS_CUSTOMERS_RESPONSE_INVALID");
  if (!text(value.id, 200) || !text(value.name, 200) || !text(value.address_json))
    return unavailable("NATIVE_POS_CUSTOMERS_RESPONSE_INVALID");
  if (!integer(value.version)) return unavailable("NATIVE_POS_CUSTOMERS_RESPONSE_INVALID");
  const nullableText = (key: "phone" | "notification_email") => {
    const item = value[key];
    return item === null || text(item, 256)
      ? (item as string | null)
      : unavailable("NATIVE_POS_CUSTOMERS_RESPONSE_INVALID");
  };
  return {
    id: value.id as string,
    name: value.name as string,
    phone: nullableText("phone"),
    notification_email: nullableText("notification_email"),
    address_json: value.address_json as string,
    version: value.version as number,
  };
}

/** Read the complete customer list from the mounted native owner. The actor
 * and package facts are transported as bounded decisions; the Rust owner owns
 * every gate, scope and field. No SQL fallback exists. */
export async function nativePosCustomerSearch(
  actor: WorkContext,
  query: string,
): Promise<PosCustomerRow[]> {
  if (new TextEncoder().encode(query).length > 400)
    throw new ApiError(422, "NATIVE_POS_CUSTOMERS_REQUEST_INVALID", "Use one bounded search filter.");
  const config = configuration();
  const holding = target(config, actor);
  const mode = await posPackageEnforcementMode();
  const access =
    mode === "disabled"
      ? undefined
      : await readPurchasedAccesses(
          (await holdingResources(actor.holdingId)).write,
          mode,
          actor.companyId,
          ["pos.sales"],
        );
  const decisions = {
    actor: { read: hasPermission(actor, "pos.read"), department: Boolean(actor.departmentId) },
    package: {
      enforced: mode !== "disabled",
      history: !access || Object.values(access).every((value) => value.history),
    },
  };
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/pos/customers/search`,
    config.base,
  );
  if (query) url.searchParams.set("q", query);
  const headers = new Headers({
    host: holding.host,
    origin: holding.origin,
    accept: "application/json",
    "cache-control": "no-cache",
    "x-erpx-context-company": actor.companyId,
    "x-erpx-pos-customer-actor": JSON.stringify(decisions.actor),
    "x-erpx-pos-customer-package": JSON.stringify(decisions.package),
  });
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
      throw new ApiError(
        response.status,
        String(body.error.code ?? "NATIVE_POS_CUSTOMERS_UNAVAILABLE"),
        body.error.message,
      );
    return unavailable();
  }
  if (!object(body) || !Array.isArray(body.data))
    return unavailable("NATIVE_POS_CUSTOMERS_RESPONSE_INVALID");
  return body.data.map(row);
}
