import "server-only";
import { Agent, request } from "node:http";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

/**
 * Strict transport for the native Sales price-list workspace list reads. The
 * axum owner executes the statements, enforces Company scope and builds the
 * `{lists,items}` response. Up to 64 registered Holdings are accepted and the
 * target is resolved from the caller's WorkContext; there is no first-Holding
 * fallback and no TypeScript SQL fallback.
 */
export type PriceListActor = Pick<
  WorkContext,
  "holdingId" | "holdingSlug" | "companyId"
>;
export type PriceListClientRow = Record<string, string | number | null>;
export type PriceListWorkspace = Readonly<{
  lists: PriceListClientRow[];
  items: PriceListClientRow[];
}>;
export type PriceListTransport = (
  url: URL,
  headers: Headers,
  signal: AbortSignal,
) => Promise<Response>;

type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Config = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;

export const NATIVE_SALES_PRICE_LIST_CONFIG_ENV = "ERPX_NATIVE_SALES_PRICE_LIST_CONFIG";
const ROUTE = "/api/v1/holdings/{holding_slug}/sales/price-list-workspace";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const MAX_RESPONSE_BYTES = 2 * 1024 * 1024;

const unavailable = (code = "SALES_NATIVE_PRICE_LIST_UNAVAILABLE"): never => {
  throw new ApiError(503, code, "The native Sales price-list read is unavailable.");
};
const record = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === "object" && !Array.isArray(v);
const text = (v: unknown, max = 4096): v is string =>
  typeof v === "string" && new TextEncoder().encode(v).length <= max;
const identifier = (v: unknown): v is string =>
  text(v, 200) && v.length > 0 && !/[\u0000-\u001f\u007f]/.test(v);
const slug = (v: unknown): v is string =>
  typeof v === "string" && v.length <= 128 && /^[a-z0-9_-]+$/.test(v);
const count = (v: unknown): v is number =>
  typeof v === "number" && Number.isSafeInteger(v) && v >= 1;

function configuration(raw: string): Config {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable("SALES_NATIVE_PRICE_LIST_CONFIG_INVALID");
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl, 512) ||
    !count(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable("SALES_NATIVE_PRICE_LIST_CONFIG_INVALID");
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl as string);
  } catch {
    return unavailable("SALES_NATIVE_PRICE_LIST_CONFIG_INVALID");
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
    return unavailable("SALES_NATIVE_PRICE_LIST_CONFIG_INVALID");
  const holdings: Holding[] = [];
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.origin, 512) ||
      !identifier(item.host) ||
      !identifier(item.holdingId) ||
      !slug(item.holdingSlug) ||
      (item.companyId !== undefined && !identifier(item.companyId))
    )
      return unavailable("SALES_NATIVE_PRICE_LIST_CONFIG_INVALID");
    let origin: URL;
    try {
      origin = new URL(item.origin as string);
    } catch {
      return unavailable("SALES_NATIVE_PRICE_LIST_CONFIG_INVALID");
    }
    if (
      origin.protocol !== "https:" ||
      origin.origin !== item.origin ||
      origin.port ||
      origin.username ||
      origin.password ||
      item.host !== origin.host
    )
      return unavailable("SALES_NATIVE_PRICE_LIST_CONFIG_INVALID");
    holdings.push({
      origin: origin.origin,
      host: item.host as string,
      holdingId: item.holdingId as string,
      holdingSlug: item.holdingSlug as string,
      ...(item.companyId === undefined ? {} : { companyId: item.companyId as string }),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs as number, holdings };
}

const LIST_KEYS = [
  "id", "company_id", "name", "currency", "customer_id", "valid_from", "valid_until",
  "max_auto_discount_bps", "status", "version", "created_by", "approved_by",
  "approval_reason", "activated_at", "created_at", "updated_at", "customer_name",
] as const;
const ITEM_KEYS = [
  "id", "price_list_id", "product_id", "price_minor", "tax_bps", "ordinal", "sku", "product_name",
] as const;
const exact = (v: unknown, keys: readonly string[]): v is Record<string, unknown> =>
  record(v) && Object.keys(v).length === keys.length && keys.every((key) => Object.hasOwn(v, key));
const cell = (v: unknown): v is string | number | null =>
  v === null || typeof v === "string" || (typeof v === "number" && Number.isSafeInteger(v));

function row(value: unknown, keys: readonly string[]): boolean {
  return exact(value, keys) && keys.every((key) => cell(value[key]));
}

function response(value: unknown, actor: PriceListActor): PriceListWorkspace {
  if (!exact(value, ["lists", "items"])) return unavailable("SALES_NATIVE_PRICE_LIST_RESPONSE_INVALID");
  if (
    !Array.isArray(value.lists) ||
    value.lists.length > 500 ||
    !Array.isArray(value.items) ||
    value.items.length > 5000
  )
    return unavailable("SALES_NATIVE_PRICE_LIST_RESPONSE_INVALID");
  if (
    !value.lists.every((item) => row(item, LIST_KEYS) && (item as Record<string, unknown>).company_id === actor.companyId) ||
    !value.items.every((item) => row(item, ITEM_KEYS))
  )
    return unavailable("SALES_NATIVE_PRICE_LIST_RESPONSE_INVALID");
  const ids = new Set<string>();
  for (const item of value.lists as PriceListClientRow[]) {
    if (typeof item.id !== "string" || ids.has(item.id)) return unavailable("SALES_NATIVE_PRICE_LIST_RESPONSE_INVALID");
    ids.add(item.id);
  }
  return value as unknown as PriceListWorkspace;
}

// Node fetch replaces an explicit Host with the loopback listener address; the
// registered Holding Host is the authority the native owner expects.
const directTransport: PriceListTransport = (url, headers, signal) =>
  new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import("node:http").ProxyEnv,
    });
    const outgoing = request(
      url,
      {
        method: "GET",
        agent,
        headers: Object.fromEntries(headers),
        signal,
        maxHeaderSize: 8192,
      },
      incoming => {
        incoming.once("error", reject);
        const code = incoming.statusCode ?? 502;
        const status = code >= 200 && code <= 599 ? code : 502;
        const responseHeaders = new Headers();
        const contentType = incoming.headers["content-type"];
        if (contentType !== undefined) responseHeaders.set("content-type", contentType);
        if (status !== 200) {
          incoming.resume();
          resolve(new Response(null, { status, headers: responseHeaders }));
          return;
        }
        const chunks: Buffer[] = [];
        let bytes = 0;
        incoming.on("data", (chunk: Buffer) => {
          bytes += chunk.length;
          if (bytes > MAX_RESPONSE_BYTES)
            incoming.destroy(new Error("SALES_NATIVE_PRICE_LIST_RESPONSE_TOO_LARGE"));
          else chunks.push(chunk);
        });
        incoming.once("end", () => {
          resolve(new Response(Buffer.concat(chunks), { status, headers: responseHeaders }));
        });
      },
    );
    outgoing.once("error", reject);
    outgoing.once("close", () => agent.destroy());
    outgoing.end();
  });

export function createPriceListWorkspaceClient(
  rawConfig: string,
  transport: PriceListTransport = directTransport,
) {
  const config = configuration(rawConfig);
  return {
    async read(actor: PriceListActor): Promise<PriceListWorkspace> {
      if (!identifier(actor.companyId) || !slug(actor.holdingSlug) || !identifier(actor.holdingId))
        return unavailable("SALES_NATIVE_PRICE_LIST_REQUEST_INVALID");
      const holding = config.holdings.find(
        (item) => item.holdingId === actor.holdingId && item.holdingSlug === actor.holdingSlug,
      );
      if (!holding) return unavailable("SALES_NATIVE_PRICE_LIST_TARGET_INVALID");
      if (holding.companyId !== undefined && holding.companyId !== actor.companyId)
        return unavailable("SALES_NATIVE_PRICE_LIST_TARGET_INVALID");
      const headers = new Headers({
        host: holding.host,
        origin: holding.origin,
        accept: "application/json",
        "cache-control": "no-cache",
        "x-erpx-context-company": actor.companyId,
      });
      const url = new URL(
        ROUTE.replace("{holding_slug}", encodeURIComponent(holding.holdingSlug)),
        config.base,
      );
      let incoming: Response;
      try {
        incoming = await transport(url, headers, AbortSignal.timeout(config.timeoutMs));
      } catch {
        return unavailable();
      }
      if (incoming.redirected || incoming.status !== 200) {
        void incoming.body?.cancel().catch(() => undefined);
        return unavailable();
      }
      if ((incoming.headers.get("content-type") ?? "").split(";")[0]?.trim() !== "application/json") {
        void incoming.body?.cancel().catch(() => undefined);
        return unavailable("SALES_NATIVE_PRICE_LIST_RESPONSE_INVALID");
      }
      let value: unknown;
      try {
        value = await incoming.json();
      } catch {
        return unavailable("SALES_NATIVE_PRICE_LIST_RESPONSE_INVALID");
      }
      return response(value, actor);
    },
  };
}

/** Configuration-backed client used by `adapters/pricing.ts`. */
export async function readPriceListWorkspace(
  actor: PriceListActor,
  transport: PriceListTransport = directTransport,
): Promise<PriceListWorkspace> {
  const raw = process.env[NATIVE_SALES_PRICE_LIST_CONFIG_ENV];
  if (raw === undefined) return unavailable("SALES_NATIVE_PRICE_LIST_CONFIG_REQUIRED");
  return createPriceListWorkspaceClient(raw, transport).read(actor);
}
