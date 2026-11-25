import "server-only";
import { Agent, request as httpRequest } from "node:http";
import { ApiError } from "@erpx/server-runtime/public/http";

/**
 * Strict private transport for the native Platform release child-image read.
 *
 * The axum owner executes the acceptance-row and child-aggregate SQL and owns
 * the response string. This bridge emits no SQL, accepts no SQL from a caller
 * and keeps no fallback reader. Missing configuration, an unregistered Holding,
 * a transport failure or an incomplete reply fails closed with ApiError(503).
 */
export type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
}>;
export type NativeConfig = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;
export type ImageActor = Readonly<{ holdingId: string; holdingSlug: string; companyId: string }>;

const ENV = "ERPX_NATIVE_PLATFORM_RELEASE_IMAGE_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

export const unavailable = (code = "NATIVE_PLATFORM_RELEASE_IMAGE_UNAVAILABLE"): never => {
  throw new ApiError(503, code, "The native Platform release child-image service is unavailable.");
};

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);

const text = (value: unknown, max = 8192): value is string =>
  typeof value === "string" && new TextEncoder().encode(value).length <= max;

export function parseNativeConfig(raw: string | undefined): NativeConfig {
  if (raw === undefined || raw === "") return unavailable("NATIVE_PLATFORM_RELEASE_IMAGE_CONFIG_REQUIRED");
  let value: { privateBaseUrl?: unknown; timeoutMs?: unknown; holdings?: unknown };
  try {
    value = JSON.parse(raw) as typeof value;
  } catch {
    return unavailable("NATIVE_PLATFORM_RELEASE_IMAGE_CONFIG_INVALID");
  }
  let base: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
  } catch {
    return unavailable("NATIVE_PLATFORM_RELEASE_IMAGE_CONFIG_INVALID");
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
    !Number.isSafeInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable("NATIVE_PLATFORM_RELEASE_IMAGE_CONFIG_INVALID");

  const holdings: Holding[] = [];
  const hosts = new Set<string>();
  for (const item of value.holdings) {
    if (
      !object(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !text(item.holdingId, 200) ||
      !text(item.holdingSlug, 128) ||
      !/^[a-z0-9_-]{1,128}$/.test(String(item.holdingSlug)) ||
      !/^[a-z0-9]{3,24}$/.test(String(item.holdingId))
    )
      return unavailable("NATIVE_PLATFORM_RELEASE_IMAGE_CONFIG_INVALID");
    let origin: URL;
    try {
      origin = new URL(String(item.origin));
    } catch {
      return unavailable("NATIVE_PLATFORM_RELEASE_IMAGE_CONFIG_INVALID");
    }
    if (
      origin.protocol !== "https:" ||
      origin.origin !== String(item.origin) ||
      origin.port ||
      origin.username ||
      origin.password ||
      origin.host !== String(item.host) ||
      hosts.has(origin.host)
    )
      return unavailable("NATIVE_PLATFORM_RELEASE_IMAGE_CONFIG_INVALID");
    hosts.add(origin.host);
    holdings.push({
      origin: origin.origin,
      host: origin.host,
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs as number, holdings };
}

/** Select the registered target from the caller's Holding identity only. */
export function selectHolding(cfg: NativeConfig, actor: ImageActor): Holding {
  if (
    !object(actor) ||
    !text(actor.holdingId, 200) ||
    !text(actor.holdingSlug, 128) ||
    !text(actor.companyId, 200)
  )
    return unavailable("PLATFORM_RELEASE_IMAGE_TARGET_INVALID");
  const holding = cfg.holdings.find(
    item => item.holdingId === actor.holdingId && item.holdingSlug === actor.holdingSlug,
  );
  if (!holding) return unavailable("PLATFORM_RELEASE_IMAGE_TARGET_INVALID");
  return holding;
}

/** The owner-built image is an opaque, bounded JSON object string. */
export function validateImage(value: unknown): string | null {
  if (value === null || value === undefined) return null;
  if (!text(value, 1_048_576) || !String(value).startsWith("{"))
    return unavailable("NATIVE_PLATFORM_RELEASE_IMAGE_RESPONSE_INVALID");
  return String(value);
}

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

export async function readNativeReleaseChildImage(
  actor: ImageActor,
  releaseId: string,
  environment: NodeJS.ProcessEnv = process.env,
): Promise<string | null> {
  if (!text(releaseId, 200) || !/^[A-Za-z0-9:_.-]+$/.test(String(releaseId)))
    return unavailable("PLATFORM_RELEASE_IMAGE_INPUT_INVALID");
  const cfg = parseNativeConfig(environment[ENV]);
  const holding = selectHolding(cfg, actor);
  let response: { status: number; value: unknown };
  try {
    response = await get(
      new URL(
        `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/platform-administration/release-child-image`,
        cfg.base,
      ),
      {
        host: holding.host,
        origin: holding.origin,
        accept: "application/json",
        "cache-control": "no-cache",
        "x-erpx-context-company": actor.companyId,
        "x-erpx-context-release": releaseId,
      },
      cfg.timeoutMs,
    );
  } catch {
    return unavailable();
  }
  if (response.status !== 200) return unavailable("NATIVE_PLATFORM_RELEASE_IMAGE_FAILED");
  const value: unknown = response.value;
  if (!object(value) || !("image" in value))
    return unavailable("NATIVE_PLATFORM_RELEASE_IMAGE_RESPONSE_INVALID");
  return validateImage(value.image);
}

export function nativeReleaseChildImageConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
