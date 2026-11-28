import "server-only";
import { BusinessRuleError } from "@erpx/platform-shared-server/index";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {
  LANE39_CONFIG_ENV,
  type NativeLane39Holding,
  type NativeLane39Transport,
} from "./native-lane-39/client";

/**
 * Native lane 40 cutover of the Brand management authority guards.
 *
 * The axum owner builds the exact registered statements, executes them on its
 * Holding pool and returns the projected guard rows. This bridge emits no SQL,
 * requests no plan, accepts no bind array from a caller and never falls back to
 * the retired TypeScript reader. Missing configuration, an unregistered Work
 * Context, a transport failure or a drifted reply fails closed.
 */
export type NativeActiveBranch = Readonly<{ id: string; display_name: string }>;

type Actor = Pick<WorkContext, "holdingId" | "holdingSlug" | "companyId" | "assignmentId" | "scope">;
type Settings = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly NativeLane39Holding[];
}>;

const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const MAX_HOLDINGS = 64;
const MAX_ID_LEN = 200;
const MAX_NAME_BYTES = 2048;
const RESPONSE_LIMIT = 256 * 1024;

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 512): value is string =>
  typeof value === "string" && value.length > 0 && new TextEncoder().encode(value).length <= max;
const validId = (value: unknown): value is string =>
  typeof value === "string" &&
  value.length > 0 &&
  value.length <= MAX_ID_LEN &&
  !/[\u0000-\u001f\u007f/\\]/.test(value);

const unavailable = (code = "BRAND_MANAGEMENT_READ_UNAVAILABLE", status = 503): never => {
  throw new BusinessRuleError(status, code, "The native Brand management read is unavailable.");
};
const misconfigured = (): never =>
  unavailable("BRAND_MANAGEMENT_CONFIG_INVALID");
const invalidInput = (): never =>
  unavailable("BRAND_MANAGEMENT_REQUEST_INVALID", 422);
const invalidResponse = (): never =>
  unavailable("BRAND_MANAGEMENT_RESPONSE_INVALID", 502);
const targetUnavailable = (): never =>
  unavailable("BRAND_MANAGEMENT_TARGET_UNAVAILABLE");

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
  if (raw === undefined) return unavailable("BRAND_MANAGEMENT_CONFIGURATION_REQUIRED");
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return misconfigured();
  }
  if (
    !record(value) ||
    !(
      typeof value.timeoutMs === "number" &&
      Number.isSafeInteger(value.timeoutMs) &&
      value.timeoutMs >= 250 &&
      value.timeoutMs <= 30_000
    ) ||
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

/** Exact WorkContext selection; there is deliberately no first-holding default. */
function selectHolding(config: Settings, actor: Actor): NativeLane39Holding {
  if (!text(actor.holdingId, MAX_ID_LEN) || !text(actor.holdingSlug, 128))
    return targetUnavailable();
  const holding = config.holdings.find(
    (item) => item.holdingId === actor.holdingId && item.holdingSlug === actor.holdingSlug,
  );
  if (!holding) return targetUnavailable();
  return holding;
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

const direct: NativeLane39Transport = (url, init) => fetch(url, init);

/** Build the lane 40 Brand management client. `transport` is injectable. */
export function createNativeBrandManagementClient(
  raw: string | undefined,
  transport: NativeLane39Transport = direct,
) {
  const config = configuration(raw);
  const call = async (route: "branch" | "company", actor: Actor, id: string): Promise<unknown> => {
    if (!validId(id)) return invalidInput();
    if (!validId(actor.companyId)) return invalidInput();
    const holding = selectHolding(config, actor);
    const url = new URL(
      `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/branding/lane40-management-${route}`,
      config.base,
    );
    const headers = new Headers({
      host: holding.host,
      origin: holding.origin,
      accept: "application/json",
      "cache-control": "no-cache",
    });
    if (route === "branch") headers.set("x-erpx-context-company", actor.companyId);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeoutMs);
    try {
      let reply: Response;
      try {
        reply = await transport(url, {
          method: "POST",
          headers,
          body: JSON.stringify({ id }),
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
          throw new BusinessRuleError(
            reply.status,
            typeof error?.code === "string" ? error.code : "BRAND_MANAGEMENT_READ_FAILED",
            typeof error?.message === "string" ? error.message : "The Brand management read was rejected.",
          );
        return unavailable();
      }
      return boundedJson(reply, controller.signal);
    } finally {
      clearTimeout(timer);
    }
  };
  return {
    async branch(actor: Actor, id: string): Promise<NativeActiveBranch | null> {
      const body = await call("branch", actor, id);
      if (!record(body) || !("branch" in body)) return invalidResponse();
      const value = body.branch;
      if (value === null) return null;
      if (
        !record(value) ||
        !validId(value.id) ||
        typeof value.display_name !== "string" ||
        value.display_name.length === 0 ||
        new TextEncoder().encode(value.display_name).length > MAX_NAME_BYTES ||
        Object.keys(value).some((key) => !["id", "display_name"].includes(key))
      )
        return invalidResponse();
      return { id: value.id, display_name: value.display_name };
    },
    async companyExists(actor: Actor, id: string): Promise<boolean> {
      const body = await call("company", actor, id);
      if (
        !record(body) ||
        typeof body.exists !== "boolean" ||
        Object.keys(body).some((key) => key !== "exists")
      )
        return invalidResponse();
      return body.exists;
    },
  };
}

/** Native active-Branch guard; `db` is ignored, no TypeScript SQL remains. */
export async function nativeBrandManagementBranch(
  _db: SqlDatabase,
  actor: Actor,
  id: string,
): Promise<NativeActiveBranch | null> {
  void _db;
  return createNativeBrandManagementClient(process.env[LANE39_CONFIG_ENV]).branch(actor, id);
}

/** Native active-Company guard; `db` is ignored, no TypeScript SQL remains. */
export async function nativeBrandManagementCompanyExists(
  _db: SqlDatabase,
  actor: Actor,
  id: string,
): Promise<boolean> {
  void _db;
  return createNativeBrandManagementClient(process.env[LANE39_CONFIG_ENV]).companyExists(actor, id);
}
