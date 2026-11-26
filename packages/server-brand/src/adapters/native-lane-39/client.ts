import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";
import { Agent, request as httpRequest } from "node:http";
import { Readable } from "node:stream";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

/**
 * Strict private transport for the lane 39 Brand workspace-choice publication
 * read. The axum owner executes the SQL, applies Company scope and returns the
 * exact projection; this bridge emits no SQL, accepts no SQL from a caller and
 * never falls back to the retired TypeScript reader. Missing configuration, a
 * transport failure, an unmatched work context or a drifted response fails
 * closed.
 */
export type NativeWorkspaceBrandChoice = Readonly<{
  scope_kind: "holding" | "branch";
  scope_id: string;
  primary_publication_id: string | null;
}>;

export type NativeLane39Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;

/** Caller's current work context; selection is an exact identity match. */
export type Lane39HoldingContext = Pick<WorkContext, "holdingId" | "holdingSlug">;

export type NativeLane39Transport = (url: URL, init: RequestInit) => Promise<Response>;

type Settings = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly NativeLane39Holding[];
}>;

export const LANE39_CONFIG_ENV = "ERPX_NATIVE_BRAND_LANE39_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const MAX_COMPANY_IDS = 1000;
const MAX_ID_LEN = 200;
const MAX_HOLDINGS = 64;
const RESPONSE_LIMIT = 1024 * 1024;

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
  throw new ApiError(503, "BRAND_WORKSPACE_CHOICE_UNAVAILABLE", "The native Brand workspace choice service is unavailable.");
};
const misconfigured = (): never => {
  throw new ApiError(503, "BRAND_WORKSPACE_CHOICE_CONFIG_INVALID", "The native Brand workspace choice transport is misconfigured.");
};
const invalidInput = (): never => {
  throw new ApiError(422, "BRAND_WORKSPACE_CHOICE_REQUEST_INVALID", "Use one bounded set of Company identities.");
};
const invalidResponse = (): never => {
  throw new ApiError(503, "BRAND_WORKSPACE_CHOICE_RESPONSE_INVALID", "The native Brand workspace choice reply was invalid.");
};
const targetUnavailable = (): never => {
  throw new ApiError(503, "BRAND_WORKSPACE_CHOICE_TARGET_UNAVAILABLE", "The current Work Context has no registered Brand workspace choice target.");
};

function loopback(value: unknown): URL {
  if (!text(value, 512)) return misconfigured();
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    return misconfigured();
  }
  const port = Number(url.port);
  if (
    url.protocol !== "http:" ||
    !["127.0.0.1", "[::1]"].includes(url.hostname) ||
    !url.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    url.username ||
    url.password ||
    url.origin !== value
  )
    return misconfigured();
  return url;
}

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
  const holdings: NativeLane39Holding[] = [];
  const hosts = new Set<string>();
  const identities = new Set<string>();
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !text(item.holdingId, MAX_ID_LEN) ||
      !text(item.holdingSlug, 128) ||
      !/^[a-z0-9_-]{1,128}$/.test(item.holdingSlug) ||
      (item.companyId !== undefined && !validId(item.companyId))
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
      ...(item.companyId === undefined ? {} : { companyId: item.companyId }),
    });
  }
  return { base: loopback(value.privateBaseUrl).origin, timeoutMs: value.timeoutMs, holdings };
}

/** Exact WorkContext selection. There is deliberately no first-holding default. */
function selectHolding(
  config: Settings,
  actor: Lane39HoldingContext | undefined,
): NativeLane39Holding {
  if (!actor || !text(actor.holdingId, MAX_ID_LEN) || !text(actor.holdingSlug, 128))
    return targetUnavailable();
  const holding = config.holdings.find(
    (item) => item.holdingId === actor.holdingId && item.holdingSlug === actor.holdingSlug,
  );
  if (!holding) return targetUnavailable();
  return holding;
}

/** True when the native lane 39 Brand consumer switch is configured. */
export function nativeLane39BrandConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[LANE39_CONFIG_ENV] !== undefined;
}

function toChoice(value: unknown): NativeWorkspaceBrandChoice {
  if (
    !record(value) ||
    (value.scope_kind !== "holding" && value.scope_kind !== "branch") ||
    !validId(value.scope_id) ||
    !(value.primary_publication_id === null || validId(value.primary_publication_id)) ||
    Object.keys(value).some(
      (key) => !["scope_kind", "scope_id", "primary_publication_id"].includes(key),
    )
  )
    return invalidResponse();
  return {
    scope_kind: value.scope_kind,
    scope_id: value.scope_id,
    primary_publication_id: value.primary_publication_id as string | null,
  };
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

const direct: NativeLane39Transport = (url, init) =>
  new Promise((resolve, reject) => {
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import("node:http").ProxyEnv });
    const headers = Object.fromEntries(new Headers(init?.headers).entries());
    const outgoing = httpRequest(url, {
      method: init?.method ?? "GET",
      agent,
      headers,
      signal: init?.signal ?? undefined,
      maxHeaderSize: 8192,
    }, incoming => {
      incoming.once("close", () => agent.destroy());
      try {
        const values = new Headers();
        for (let i = 0; i < incoming.rawHeaders.length; i += 2) values.append(incoming.rawHeaders[i]!, incoming.rawHeaders[i + 1]!);
        resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, { status: incoming.statusCode ?? 502, headers: values }));
      } catch (error) { incoming.destroy(); agent.destroy(); reject(error); }
    });
    outgoing.once("error", error => { agent.destroy(); reject(error); });
    if (init?.body) outgoing.write(init.body);
    outgoing.end();
  });

/** Build the lane 39 Brand client. `transport` is injectable for contract tests. */
export function createLane39BrandClient(
  raw: string | undefined,
  transport: NativeLane39Transport = direct,
) {
  const config = configuration(raw);
  return {
    async workspaceChoices(
      companyIds: readonly string[],
      actor: Lane39HoldingContext | undefined,
    ): Promise<NativeWorkspaceBrandChoice[]> {
      if (
        !Array.isArray(companyIds) ||
        companyIds.length < 1 ||
        companyIds.length > MAX_COMPANY_IDS ||
        !companyIds.every(validId)
      )
        return invalidInput();
      const holding = selectHolding(config, actor);
      const url = new URL(
        `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/branding/lane39-workspace-choice`,
        config.base,
      );
      const headers = new Headers({
        host: holding.host,
        origin: holding.origin,
        "content-type": "application/json",
        accept: "application/json",
        "cache-control": "no-cache",
      });
      if (holding.companyId) headers.set("x-erpx-context-company", holding.companyId);
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), config.timeoutMs);
      try {
        let reply: Response;
        try {
          reply = await transport(url, {
            method: "POST",
            headers,
            body: JSON.stringify({ companyIds }),
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
              typeof error?.code === "string" ? error.code : "BRAND_WORKSPACE_CHOICE_READ_FAILED",
              typeof error?.message === "string" ? error.message : "The Brand workspace choice read was rejected.",
            );
          return unavailable();
        }
        const body = await boundedJson(reply, controller.signal);
        if (
          !record(body) ||
          !Array.isArray(body.brands) ||
          body.brands.length > MAX_COMPANY_IDS * 2
        )
          return invalidResponse();
        return body.brands.map(toChoice);
      } finally {
        clearTimeout(timer);
      }
    },
  };
}

/** Default consumer: reads the private configuration and performs the read. */
export async function nativeWorkspaceChoiceBrands(
  companyIds: readonly string[],
  actor: Lane39HoldingContext | undefined,
): Promise<NativeWorkspaceBrandChoice[]> {
  return createLane39BrandClient(process.env[LANE39_CONFIG_ENV]).workspaceChoices(companyIds, actor);
}
