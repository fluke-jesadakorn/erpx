import "server-only";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";

/**
 * Strict private transport for the Ecommerce checkout detail-link family.
 * The axum owner executes both SELECTs against the Ecommerce-owned link tables,
 * joins the closed Commerce return projection and projects every retired
 * column. This bridge emits no SQL and has no fallback reader: missing config,
 * an unmatched Work Context, a transport failure or a drifted reply all fail
 * closed.
 */
export type NativeLinkRow = Readonly<Record<string, string | number | null>>;
export type NativeDetailLinks = Readonly<{
  fulfillmentLinks: readonly NativeLinkRow[];
  returnLinks: readonly NativeLinkRow[];
}>;

export type DetailLinkHolding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
}>;

export type DetailLinkHoldingContext = Pick<WorkContext, "holdingId" | "holdingSlug">;
export type DetailLinkTransport = (url: URL, init: RequestInit) => Promise<Response>;

type Settings = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly DetailLinkHolding[];
}>;

export const DETAIL_LINKS_CONFIG_ENV = "ERPX_NATIVE_ECOM_DETAIL_LINKS_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const MAX_ID_LEN = 200;
const MAX_ROWS = 5000;
const MAX_HOLDINGS = 64;
const RESPONSE_LIMIT = 4 * 1024 * 1024;

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 512): value is string =>
  typeof value === "string" && value.length > 0 && new TextEncoder().encode(value).length <= max;
const validId = (value: unknown): value is string =>
  typeof value === "string" &&
  value.length > 0 &&
  value.length <= MAX_ID_LEN &&
  !/[\u0000-\u001f\u007f/\\'"\s]/.test(value);

const unavailable = (): never => {
  throw new ApiError(503, "ECOMMERCE_DETAIL_LINKS_UNAVAILABLE", "The native Ecommerce detail link service is unavailable.");
};
const misconfigured = (): never => {
  throw new ApiError(503, "ECOMMERCE_DETAIL_LINKS_CONFIG_INVALID", "The native Ecommerce detail link transport is misconfigured.");
};
const invalidInput = (): never => {
  throw new ApiError(422, "ECOMMERCE_DETAIL_LINKS_REQUEST_INVALID", "Use one bounded Ecommerce checkout identity.");
};
const invalidResponse = (): never => {
  throw new ApiError(503, "ECOMMERCE_DETAIL_LINKS_RESPONSE_INVALID", "The native Ecommerce detail link reply was invalid.");
};
const targetUnavailable = (): never => {
  throw new ApiError(503, "ECOMMERCE_DETAIL_LINKS_TARGET_UNAVAILABLE", "The current Work Context has no registered Ecommerce detail-link target.");
};

function configuration(raw: string | undefined): Settings {
  if (raw === undefined) return unavailable();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return misconfigured();
  }
  if (
    !record(value) ||
    !(typeof value.timeoutMs === "number" &&
      Number.isSafeInteger(value.timeoutMs) &&
      value.timeoutMs >= 250 &&
      value.timeoutMs <= 30_000) ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > MAX_HOLDINGS
  )
    return misconfigured();
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl as string);
  } catch {
    return misconfigured();
  }
  const port = Number(base.port);
  if (
    typeof value.privateBaseUrl !== "string" ||
    base.protocol !== "http:" ||
    !["127.0.0.1", "[::1]"].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl
  )
    return misconfigured();
  const holdings: DetailLinkHolding[] = [];
  const hosts = new Set<string>();
  const identities = new Set<string>();
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !text(item.holdingId, MAX_ID_LEN) ||
      !text(item.holdingSlug, 128) ||
      !/^[a-z0-9_-]{1,128}$/.test(item.holdingSlug)
    )
      return misconfigured();
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return misconfigured();
    }
    if (
      origin.protocol !== "https:" ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      origin.host !== item.host ||
      hosts.has(origin.host) ||
      identities.has(`${item.holdingId}\u0000${item.holdingSlug}`)
    )
      return misconfigured();
    hosts.add(origin.host);
    identities.add(`${item.holdingId}\u0000${item.holdingSlug}`);
    holdings.push({
      origin: origin.origin,
      host: item.host,
      holdingId: item.holdingId,
      holdingSlug: item.holdingSlug,
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

function selectHolding(config: Settings, actor: DetailLinkHoldingContext | undefined): DetailLinkHolding {
  if (!actor || !text(actor.holdingId, MAX_ID_LEN) || !text(actor.holdingSlug, 128))
    return targetUnavailable();
  const holding = config.holdings.find(
    (item) => item.holdingId === actor.holdingId && item.holdingSlug === actor.holdingSlug,
  );
  if (!holding) return targetUnavailable();
  return holding;
}

export function nativeDetailLinksEcomConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[DETAIL_LINKS_CONFIG_ENV] !== undefined;
}

function toNativeRow(value: unknown): NativeLinkRow {
  if (
    !record(value) ||
    Object.keys(value).length > 64 ||
    !validId(value.id) ||
    !validId(value.checkout_id) ||
    Object.values(value).some((item) => !(item === null || typeof item === "string" || (typeof item === "number" && Number.isFinite(item)))) ||
    JSON.stringify(value).length > 64 * 1024
  )
    return invalidResponse();
  return value as NativeLinkRow;
}

async function boundedJson(reply: Response, signal: AbortSignal): Promise<unknown> {
  if (reply.status !== 200 || !reply.body) return invalidResponse();
  const reader = reply.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  const cancel = () => {
    void reader.cancel().catch(() => undefined);
  };
  signal.addEventListener("abort", cancel, { once: true });
  try {
    for (;;) {
      const part = await reader.read();
      if (signal.aborted) return unavailable();
      if (part.done) break;
      size += part.value.length;
      if (size > RESPONSE_LIMIT) return invalidResponse();
      chunks.push(part.value);
    }
  } finally {
    signal.removeEventListener("abort", cancel);
    reader.releaseLock();
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  try {
    return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    return invalidResponse();
  }
}

const direct: DetailLinkTransport = (url, init) => fetch(url, init);

/** Build the detail-link client. `transport` is injectable for tests. */
export function createDetailLinkEcomClient(
  raw: string | undefined,
  transport: DetailLinkTransport = direct,
) {
  const config = configuration(raw);
  return {
    async links(
      checkoutId: string,
      actor: DetailLinkHoldingContext | undefined,
    ): Promise<NativeDetailLinks> {
      if (!validId(checkoutId)) return invalidInput();
      const holding = selectHolding(config, actor);
      const url = new URL(
        `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/ecommerce/native-detail-links`,
        config.base,
      );
      const headers = new Headers({
        host: holding.host,
        origin: holding.origin,
        accept: "application/json",
        "cache-control": "no-cache",
        "content-type": "application/json",
      });
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), config.timeoutMs);
      try {
        let reply: Response;
        try {
          reply = await transport(url, {
            method: "POST",
            headers,
            body: JSON.stringify({ checkoutId }),
            cache: "no-store",
            redirect: "error",
            signal: controller.signal,
          });
        } catch {
          return unavailable();
        }
        if (reply.status !== 200) {
          const payload: unknown = await reply.json().catch((): unknown => null);
          const error = record(payload) && record(payload.error) ? payload.error : undefined;
          if (reply.status >= 400 && reply.status < 500)
            throw new ApiError(
              reply.status,
              typeof error?.code === "string" ? error.code : "ECOMMERCE_DETAIL_LINKS_READ_FAILED",
              typeof error?.message === "string" ? error.message : "The Ecommerce detail link read was rejected.",
            );
          return unavailable();
        }
        const body = await boundedJson(reply, controller.signal);
        if (
          !record(body) ||
          Object.keys(body).length !== 2 ||
          !Array.isArray(body.fulfillmentLinks) ||
          !Array.isArray(body.returnLinks) ||
          body.fulfillmentLinks.length > MAX_ROWS ||
          body.returnLinks.length > MAX_ROWS
        )
          return invalidResponse();
        return {
          fulfillmentLinks: body.fulfillmentLinks.map(toNativeRow),
          returnLinks: body.returnLinks.map(toNativeRow),
        };
      } finally {
        clearTimeout(timer);
      }
    },
  };
}

/** Default consumer: reads the private config and performs the owner read. */
export async function nativeDetailLinks(
  checkoutId: string,
  actor: DetailLinkHoldingContext | undefined,
): Promise<NativeDetailLinks> {
  return createDetailLinkEcomClient(process.env[DETAIL_LINKS_CONFIG_ENV]).links(checkoutId, actor);
}
