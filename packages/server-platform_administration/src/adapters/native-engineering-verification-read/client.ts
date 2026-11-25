import "server-only";
import { Agent, request as httpRequest } from "node:http";
import { Readable } from "node:stream";
import { ApiError } from "@erpx/server-runtime/public/http";

/**
 * Strict private transport for the owner-local native engineering verification
 * read port. The axum owner executes the `prior` statement against its own
 * transaction and owns the response. This bridge emits no SQL, accepts no SQL
 * from callers and keeps no fallback: missing configuration, an unregistered
 * target, a transport failure or an incomplete reply fails closed.
 *
 * It shares the enrollment command's registered target configuration so the
 * lead mount only has to add the read router, not a second env contract.
 */
export type NativeEngineeringReadConfig = Readonly<{
  privateBaseUrl: string;
  holdingSlug: string;
  holdingId: string;
  origin: string;
  timeoutMs: number;
}>;

export type EngineeringPrior = Readonly<{
  source_fingerprint: string;
  software_version: string;
  release_manifest_sha256: string;
}>;

const ENV = "ERPX_NATIVE_PLATFORM_ENGINEERING_CONFIG";
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

export const unavailable = (code = "NATIVE_PLATFORM_ENGINEERING_READ_UNAVAILABLE"): never => {
  throw new ApiError(503, code, "The native engineering verification read is unavailable.");
};

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);

export function parseNativeEngineeringReadConfig(raw: string | undefined): NativeEngineeringReadConfig {
  if (raw === undefined || raw === "") return unavailable("NATIVE_PLATFORM_ENGINEERING_CONFIG_REQUIRED");
  let value: Record<string, unknown>;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!record(parsed)) return unavailable("NATIVE_PLATFORM_ENGINEERING_CONFIG_INVALID");
    value = parsed;
  } catch {
    return unavailable("NATIVE_PLATFORM_ENGINEERING_CONFIG_INVALID");
  }
  if (typeof value.privateBaseUrl !== "string") return unavailable("NATIVE_PLATFORM_ENGINEERING_CONFIG_INVALID");
  if (typeof value.timeoutMs !== "number" || !Number.isSafeInteger(value.timeoutMs) || value.timeoutMs < 250 || value.timeoutMs > 30000)
    return unavailable("NATIVE_PLATFORM_ENGINEERING_CONFIG_INVALID");
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable("NATIVE_PLATFORM_ENGINEERING_CONFIG_INVALID");
  }
  const port = Number(base.port);
  if (
    base.protocol !== "http:" ||
    !["127.0.0.1", "[::1]"].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username || base.password || base.origin !== value.privateBaseUrl
  )
    return unavailable("NATIVE_PLATFORM_ENGINEERING_CONFIG_INVALID");
  if (
    typeof value.holdingSlug !== "string" || !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug) ||
    typeof value.holdingId !== "string" || !value.holdingId || value.holdingId.length > 200 ||
    typeof value.origin !== "string" ||
    !(value.origin === "http://127.0.0.1:3104" || value.origin === "http://localhost:3104" ||
      (value.origin.startsWith("https://") && value.origin === new URL(value.origin).origin))
  )
    return unavailable("NATIVE_PLATFORM_ENGINEERING_CONFIG_INVALID");
  return {
    privateBaseUrl: base.origin,
    holdingSlug: value.holdingSlug,
    holdingId: value.holdingId,
    origin: value.origin,
    timeoutMs: value.timeoutMs,
  };
}

function post(
  url: URL,
  headers: Record<string, string>,
  body: string,
  signal: AbortSignal,
): Promise<{ status: number; text: string }> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import("node:http").ProxyEnv });
    const outgoing = httpRequest(
      url,
      { method: "POST", agent, headers: { ...headers, "content-length": String(new TextEncoder().encode(body).length) }, signal, maxHeaderSize: 8192 },
      incoming => {
        const chunks: Buffer[] = [];
        incoming.on("data", chunk => chunks.push(Buffer.from(chunk)));
        incoming.on("end", () => {
          agent.destroy();
          resolve({ status: incoming.statusCode ?? 502, text: Buffer.concat(chunks).toString("utf8") });
        });
        incoming.on("error", error => { agent.destroy(); reject(error); });
      },
    );
    outgoing.once("error", error => { agent.destroy(); reject(error); });
    outgoing.write(body);
    outgoing.end();
  });
}

const hex64 = (value: unknown): value is string =>
  typeof value === "string" && /^[a-f0-9]{64}$/.test(value);

function parsePrior(value: unknown): EngineeringPrior | null {
  if (value === null || value === undefined) return null;
  if (
    !record(value) ||
    !hex64(value.source_fingerprint) ||
    !hex64(value.release_manifest_sha256) ||
    value.software_version !== `release-${value.source_fingerprint}`
  )
    return unavailable("NATIVE_PLATFORM_ENGINEERING_READ_RESPONSE_INVALID");
  return {
    source_fingerprint: value.source_fingerprint,
    software_version: value.software_version,
    release_manifest_sha256: value.release_manifest_sha256,
  };
}

export async function nativeEngineeringPriorRead(
  companyId: string,
  run: string,
  environment: NodeJS.ProcessEnv = process.env,
): Promise<EngineeringPrior | null> {
  if (
    typeof companyId !== "string" || !companyId || companyId.length > 200 ||
    typeof run !== "string" || !/^[a-f0-9]{12}$/.test(run)
  )
    return unavailable("NATIVE_PLATFORM_ENGINEERING_READ_INPUT_INVALID");
  const config = parseNativeEngineeringReadConfig(environment[ENV]);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(config.holdingSlug)}/platform/engineering-verification/read/prior`,
    config.privateBaseUrl,
  );
  const body = JSON.stringify({ companyId, run });
  if (new TextEncoder().encode(body).length > 16 * 1024) return unavailable("NATIVE_PLATFORM_ENGINEERING_READ_INPUT_INVALID");
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  try {
    const response = await post(
      url,
      {
        host: new URL(config.origin).host,
        origin: config.origin,
        accept: "application/json",
        "content-type": "application/json",
        "cache-control": "no-cache",
        "x-erpx-context-company": config.holdingId,
      },
      body,
      controller.signal,
    );
    if (response.status !== 200 || response.text.length > 64 * 1024)
      return unavailable("NATIVE_PLATFORM_ENGINEERING_READ_FAILED");
    const payload: unknown = JSON.parse(response.text);
    if (!record(payload) || !("data" in payload))
      return unavailable("NATIVE_PLATFORM_ENGINEERING_READ_RESPONSE_INVALID");
    return parsePrior(payload.data);
  } catch (error) {
    if (error instanceof ApiError) throw error;
    return unavailable();
  } finally {
    clearTimeout(timer);
  }
}

const hex = (value: unknown, length: number): value is string =>
  typeof value === "string" && new RegExp(`^[a-f0-9]{${length}}$`).test(value);

function boundedCompany(companyId: unknown): companyId is string {
  return typeof companyId === "string" && !!companyId && companyId.length <= 200;
}

async function read(
  path: string,
  body: string,
  config: NativeEngineeringReadConfig,
): Promise<unknown> {
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(config.holdingSlug)}/platform/engineering-verification/read/${path}`,
    config.privateBaseUrl,
  );
  if (new TextEncoder().encode(body).length > 16 * 1024)
    return unavailable("NATIVE_PLATFORM_ENGINEERING_READ_INPUT_INVALID");
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  try {
    const response = await post(
      url,
      {
        host: new URL(config.origin).host,
        origin: config.origin,
        accept: "application/json",
        "content-type": "application/json",
        "cache-control": "no-cache",
        "x-erpx-context-company": config.holdingId,
      },
      body,
      controller.signal,
    );
    if (response.status !== 200 || response.text.length > 64 * 1024)
      return unavailable("NATIVE_PLATFORM_ENGINEERING_READ_FAILED");
    const payload: unknown = JSON.parse(response.text);
    if (!record(payload) || !("data" in payload))
      return unavailable("NATIVE_PLATFORM_ENGINEERING_READ_RESPONSE_INVALID");
    return payload.data;
  } catch (error) {
    if (error instanceof ApiError) throw error;
    return unavailable();
  } finally {
    clearTimeout(timer);
  }
}

/** Persisted half of the engineering verification predicate. The separate
 *  database-identity predicate stays in the caller's own SQL fence. */
export async function nativeEngineeringActiveRead(
  companyId: string,
  run: string,
  sourceFingerprint: string,
  softwareVersion: string,
  fileManifestSha256: string,
  environment: NodeJS.ProcessEnv = process.env,
): Promise<boolean> {
  if (
    !boundedCompany(companyId) || !hex(run, 12) || !hex(sourceFingerprint, 64) ||
    !hex(fileManifestSha256, 64) || softwareVersion !== `release-${sourceFingerprint}`
  )
    return unavailable("NATIVE_PLATFORM_ENGINEERING_READ_INPUT_INVALID");
  const config = parseNativeEngineeringReadConfig(environment[ENV]);
  const data = await read("active", JSON.stringify({
    companyId, run, sourceFingerprint, softwareVersion, fileManifestSha256,
  }), config);
  if (typeof data !== "boolean")
    return unavailable("NATIVE_PLATFORM_ENGINEERING_READ_RESPONSE_INVALID");
  return data;
}

export async function nativeEngineeringReplyGuard(
  companyId: string,
  environment: NodeJS.ProcessEnv = process.env,
): Promise<string[]> {
  if (!boundedCompany(companyId))
    return unavailable("NATIVE_PLATFORM_ENGINEERING_READ_INPUT_INVALID");
  const config = parseNativeEngineeringReadConfig(environment[ENV]);
  const data = await read("reply-guard", JSON.stringify({ companyId }), config);
  if (
    !Array.isArray(data) ||
    !data.every(value => typeof value === "string" && value.length <= 200)
  )
    return unavailable("NATIVE_PLATFORM_ENGINEERING_READ_RESPONSE_INVALID");
  return data as string[];
}

export function nativeEngineeringReadAvailable(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
