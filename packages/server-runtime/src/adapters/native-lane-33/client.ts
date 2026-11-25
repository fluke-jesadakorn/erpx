import "server-only";
import { Agent, request as httpRequest } from "node:http";
import { ApiError } from "@erpx/server-runtime/public/http";

/**
 * Strict private transport for the native Runtime holding-binding reads.
 *
 * The axum owner executes the `holding_bindings` SQL and owns the response
 * shape. This bridge emits no SQL, accepts no SQL from a caller and keeps no
 * TypeScript fallback reader. Missing configuration, a transport failure or an
 * incomplete reply fails closed with ApiError(503, ...).
 */
export type BindingKind = "by-slug" | "default" | "active";

export type NativeHoldingBinding = Readonly<{
  holdingId: string;
  slug: string;
  displayName: string;
}>;

export type NativeConfig = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;

const ENV = "ERPX_NATIVE_RUNTIME_HOLDING_READ_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const KINDS: readonly BindingKind[] = ["by-slug", "default", "active"];

export const unavailable = (code = "NATIVE_RUNTIME_HOLDING_BINDING_UNAVAILABLE"): never => {
  throw new ApiError(503, code, "The native Runtime holding-binding service is unavailable.");
};

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);

const text = (value: unknown, max = 2048): value is string =>
  typeof value === "string" && new TextEncoder().encode(value).length <= max;

/** Parse and bound the private native configuration; no network here. */
export function parseNativeConfig(raw: string | undefined): NativeConfig {
  if (raw === undefined || raw === "") return unavailable("NATIVE_RUNTIME_HOLDING_BINDING_CONFIG_REQUIRED");
  let value: {
    privateBaseUrl?: unknown;
    host?: unknown;
    holdingSlug?: unknown;
    origin?: unknown;
    timeoutMs?: unknown;
  };
  try {
    value = JSON.parse(raw) as typeof value;
  } catch {
    return unavailable("NATIVE_RUNTIME_HOLDING_BINDING_CONFIG_INVALID");
  }
  let base: URL;
  let origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    return unavailable("NATIVE_RUNTIME_HOLDING_BINDING_CONFIG_INVALID");
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
    origin.protocol !== "https:" ||
    origin.origin !== String(value.origin) ||
    origin.username ||
    origin.password ||
    !text(value.host, 512) ||
    value.host !== origin.host ||
    !text(value.holdingSlug, 128) ||
    !/^[a-z0-9_-]{1,128}$/.test(String(value.holdingSlug)) ||
    !Number.isSafeInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000
  )
    return unavailable("NATIVE_RUNTIME_HOLDING_BINDING_CONFIG_INVALID");
  return {
    base: base.origin,
    host: String(value.host),
    holdingSlug: String(value.holdingSlug),
    origin: origin.origin,
    timeoutMs: value.timeoutMs as number,
  };
}

/** Strict, closed response projection. Extra fields are never forwarded. */
export function validateBinding(value: unknown): NativeHoldingBinding {
  if (
    !object(value) ||
    !text(value.holdingId, 24) ||
    !/^[a-z0-9]{3,24}$/.test(String(value.holdingId)) ||
    !text(value.slug, 128) ||
    !/^[a-z0-9_-]{1,128}$/.test(String(value.slug)) ||
    !text(value.displayName, 512)
  )
    return unavailable("NATIVE_RUNTIME_HOLDING_BINDING_RESPONSE_INVALID");
  return {
    holdingId: String(value.holdingId),
    slug: String(value.slug),
    displayName: String(value.displayName),
  };
}

/** GET with an explicit Host header; fetch forbids overriding Host. */
function get(
  url: URL,
  headers: Record<string, string>,
  timeoutMs: number,
): Promise<{ status: number; value: unknown }> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import("node:http").ProxyEnv,
    });
    const outgoing = httpRequest(
      url,
      {
        method: "GET",
        agent,
        headers: { ...headers },
        signal: AbortSignal.timeout(timeoutMs),
        maxHeaderSize: 8192,
      },
      incoming => {
        const chunks: Buffer[] = [];
        incoming.on("data", chunk => chunks.push(Buffer.from(chunk)));
        incoming.on("end", () => {
          agent.destroy();
          const raw = Buffer.concat(chunks).toString("utf8");
          let value: unknown = null;
          try {
            value = raw ? JSON.parse(raw) : null;
          } catch {
            value = null;
          }
          resolve({ status: incoming.statusCode ?? 502, value });
        });
        incoming.on("error", error => {
          agent.destroy();
          reject(error);
        });
      },
    );
    outgoing.on("error", error => {
      agent.destroy();
      reject(error);
    });
    outgoing.end();
  });
}

function requestHeaders(cfg: NativeConfig, scope: readonly string[], slug?: string): Record<string, string> {
  if (
    !Array.isArray(scope) ||
    scope.length < 1 ||
    scope.length > 64 ||
    !scope.every(id => typeof id === "string" && /^[a-z0-9]{3,24}$/.test(id))
  )
    return unavailable("RUNTIME_HOLDING_BINDING_SCOPE_INVALID");
  const headers: Record<string, string> = {
    host: cfg.host,
    origin: cfg.origin,
    accept: "application/json",
    "cache-control": "no-cache",
    "x-erpx-allowed-holdings": scope.join(","),
  };
  if (slug !== undefined) {
    if (!/^[a-z0-9_-]{1,128}$/.test(slug)) return unavailable("RUNTIME_HOLDING_BINDING_SCOPE_INVALID");
    headers["x-erpx-holding-slug"] = slug;
  }
  return headers;
}

/** Read one binding by slug. `null` means no in-scope active binding. */
export async function readNativeHoldingBindingBySlug(
  scope: readonly string[],
  slug: string,
  environment: NodeJS.ProcessEnv = process.env,
): Promise<NativeHoldingBinding | null> {
  return readOne("by-slug", scope, slug, environment);
}

/** Read the first in-scope active binding. */
export async function readNativeDefaultHoldingBinding(
  scope: readonly string[],
  environment: NodeJS.ProcessEnv = process.env,
): Promise<NativeHoldingBinding | null> {
  return readOne("default", scope, undefined, environment);
}

/** Read all in-scope active bindings in stable holding-id order. */
export async function readNativeActiveHoldingBindings(
  scope: readonly string[],
  environment: NodeJS.ProcessEnv = process.env,
): Promise<readonly NativeHoldingBinding[]> {
  const cfg = parseNativeConfig(environment[ENV]);
  const response = await call(cfg, "active", scope, undefined);
  if (!Array.isArray(response) || response.length > 64)
    return unavailable("NATIVE_RUNTIME_HOLDING_BINDING_RESPONSE_INVALID");
  const rows = response.map(validateBinding);
  const ids = rows.map(row => row.holdingId);
  if (ids.some((id, index) => index > 0 && id <= ids[index - 1]!))
    return unavailable("NATIVE_RUNTIME_HOLDING_BINDING_RESPONSE_INVALID");
  return rows;
}

async function readOne(
  kind: BindingKind,
  scope: readonly string[],
  slug: string | undefined,
  environment: NodeJS.ProcessEnv,
): Promise<NativeHoldingBinding | null> {
  const cfg = parseNativeConfig(environment[ENV]);
  const response = await call(cfg, kind, scope, slug);
  if (response === null) return null;
  return validateBinding(response);
}

async function call(
  cfg: NativeConfig,
  kind: BindingKind,
  scope: readonly string[],
  slug: string | undefined,
): Promise<unknown> {
  if (!KINDS.includes(kind)) return unavailable("RUNTIME_HOLDING_BINDING_SCOPE_INVALID");
  let response: { status: number; value: unknown };
  try {
    response = await get(
      new URL(
        `/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/runtime/holding-bindings/${kind}`,
        cfg.base,
      ),
      requestHeaders(cfg, scope, slug),
      cfg.timeoutMs,
    );
  } catch {
    return unavailable();
  }
  if (response.status !== 200) return unavailable("NATIVE_RUNTIME_HOLDING_BINDING_FAILED");
  const value: unknown = response.value;
  if (!object(value)) return unavailable("NATIVE_RUNTIME_HOLDING_BINDING_RESPONSE_INVALID");
  if (kind === "active") {
    if (!("bindings" in value)) return unavailable("NATIVE_RUNTIME_HOLDING_BINDING_RESPONSE_INVALID");
    return value.bindings;
  }
  if (!("binding" in value)) return unavailable("NATIVE_RUNTIME_HOLDING_BINDING_RESPONSE_INVALID");
  return value.binding;
}

/** True when the native owner transport is configured. */
export function nativeHoldingBindingReadConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
