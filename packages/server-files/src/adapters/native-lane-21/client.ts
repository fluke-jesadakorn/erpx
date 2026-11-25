import "server-only";
import { Agent, request as httpRequest } from "node:http";
import { ApiError } from "@erpx/server-runtime/public/http";

/**
 * Strict private transport for the native Files commercial-usage read family.
 *
 * The axum owner executes the statements with sqlx and returns the complete
 * row projection. This bridge emits no SQL, accepts no SQL from a caller and
 * fails closed with 503 when the owner is unavailable, the registered Holding
 * is unknown or a response is malformed. There is no TypeScript fallback and
 * no default Holding: every request must name a registered target from the
 * caller's WorkContext.
 */
export const FILES_CU_STORED_ORIGINALS = "stored_originals";
export const FILES_CU_UNRECONCILED_ORIGINALS = "unreconciled_originals";
export const FILES_CU_RECONCILIATION = "reconciliation";
export const FILES_CU_EARLIEST_ORIGINAL = "earliest_original";

export type FilesCommercialReadKind =
  | typeof FILES_CU_STORED_ORIGINALS
  | typeof FILES_CU_UNRECONCILED_ORIGINALS
  | typeof FILES_CU_RECONCILIATION
  | typeof FILES_CU_EARLIEST_ORIGINAL;

export type FilesCommercialHolding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
export type FilesCommercialConfig = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly FilesCommercialHolding[];
}>;
/** Fields of WorkContext used to select exactly one registered Holding.
 * These reads are holding-wide, so companyId is optional; when present it is
 * still validated and must agree with the holding's configured company. */
export type FilesCommercialActor = {
  holdingId?: string;
  holdingSlug?: string;
  origin?: string;
  companyId?: string;
};
export type CommercialReadParams = Readonly<{
  kind: FilesCommercialReadKind;
  start?: number | null;
  end?: number | null;
  branchId?: string | null;
}>;
export type CommercialReadTransport = (
  url: URL,
  headers: Record<string, string>,
  body: string,
  timeoutMs: number,
) => Promise<{ status: number; value: unknown }>;

export type CommercialStorageRow = {
  id: string;
  company_id: string;
  branch_id: string | null;
  bytes: number;
  created_at: number;
  destroyed_at: number | null;
};
export type FilesCommercialReadResult =
  | { kind: typeof FILES_CU_STORED_ORIGINALS; rows: CommercialStorageRow[] }
  | { kind: typeof FILES_CU_UNRECONCILED_ORIGINALS; count: number }
  | { kind: typeof FILES_CU_RECONCILIATION; ready: boolean }
  | { kind: typeof FILES_CU_EARLIEST_ORIGINAL; earliest: number | null };

type Settings = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly FilesCommercialHolding[];
}>;
type Environment = Readonly<Record<string, string | undefined>>;

const ENV = "ERPX_NATIVE_LANE_21_READ_CONFIG";
const RESERVED = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const MAX_SAFE_MS = 9_007_199_254_740_991;
const KINDS = new Set<string>([
  FILES_CU_STORED_ORIGINALS,
  FILES_CU_UNRECONCILED_ORIGINALS,
  FILES_CU_RECONCILIATION,
  FILES_CU_EARLIEST_ORIGINAL,
]);

const unavailable = (
  code = "FILES_COMMERCIAL_USAGE_READ_UNAVAILABLE",
  message = "The native Files commercial-usage service is unavailable.",
): never => {
  throw new ApiError(503, code, message);
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 512): value is string =>
  typeof value === "string" && value.length > 0 && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown, max = 200): value is string =>
  text(value, max) && !/[\u0000-\u001f/?#\\]/.test(value);
const slug = (value: unknown): value is string =>
  text(value, 128) && /^[a-z0-9_-]+$/.test(value);
const hostname = (value: unknown): value is string =>
  text(value, 253) && /^[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?$/.test(value);
const timestamp = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0 && value <= MAX_SAFE_MS;

export function nativeFilesCommercialReadConfigured(
  environment: Environment = process.env,
): boolean {
  return environment[ENV] !== undefined;
}

/** Parse and validate 1..64 distinct registered Holdings. */
export function parseCommercialReadConfig(value: unknown): Settings {
  if (
    !object(value) ||
    !text(value.privateBaseUrl, 512) ||
    typeof value.timeoutMs !== "number" ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable("FILES_COMMERCIAL_USAGE_CONFIG_INVALID");
  let base: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
  } catch {
    return unavailable("FILES_COMMERCIAL_USAGE_CONFIG_INVALID");
  }
  const port = Number(base.port);
  if (
    base.protocol !== "http:" ||
    !["127.0.0.1", "[::1]"].includes(base.hostname) ||
    !base.port ||
    RESERVED.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl
  )
    return unavailable("FILES_COMMERCIAL_USAGE_CONFIG_INVALID");

  const holdings: FilesCommercialHolding[] = [];
  const ids = new Set<string>();
  const origins = new Set<string>();
  const targets = new Set<string>();
  for (const item of value.holdings) {
    if (
      !object(item) ||
      !text(item.origin, 512) ||
      !hostname(item.host) ||
      !identifier(item.holdingId) ||
      !slug(item.holdingSlug) ||
      (item.companyId !== undefined && !identifier(item.companyId))
    )
      return unavailable("FILES_COMMERCIAL_USAGE_CONFIG_INVALID");
    let origin: URL;
    try {
      origin = new URL(String(item.origin));
    } catch {
      return unavailable("FILES_COMMERCIAL_USAGE_CONFIG_INVALID");
    }
    if (
      origin.protocol !== "https:" ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      // The Host header must be the exact registered Holding host.
      origin.host !== item.host ||
      ids.has(String(item.holdingId)) ||
      origins.has(origin.origin) ||
      targets.has(`${String(item.host)}\u0000${String(item.holdingSlug)}`)
    )
      return unavailable("FILES_COMMERCIAL_USAGE_CONFIG_INVALID");
    ids.add(String(item.holdingId));
    origins.add(origin.origin);
    targets.add(`${String(item.host)}\u0000${String(item.holdingSlug)}`);
    holdings.push({
      origin: origin.origin,
      host: String(item.host),
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
      ...(item.companyId === undefined ? {} : { companyId: String(item.companyId) }),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Pick exactly one registered Holding from the caller's WorkContext. Two
 * selectors must agree; there is no first-Holding default. */
function selectTarget(settings: Settings, actor: FilesCommercialActor): FilesCommercialHolding {
  const invalidRequest = () =>
    unavailable("FILES_COMMERCIAL_USAGE_REQUEST_INVALID", "Use one bounded Files commercial-usage request.");
  const unknown = () => unavailable("FILES_COMMERCIAL_USAGE_TARGET_UNAVAILABLE");

  if (!object(actor)) return invalidRequest();
  let byOrigin: FilesCommercialHolding | undefined;
  if (actor.origin !== undefined) {
    if (!text(actor.origin, 512)) return invalidRequest();
    let origin: URL;
    try {
      origin = new URL(actor.origin);
    } catch {
      return invalidRequest();
    }
    if (origin.origin !== actor.origin || origin.protocol !== "https:") return invalidRequest();
    byOrigin = settings.holdings.find(holding => holding.origin === origin.origin);
  }
  let byIdentity: FilesCommercialHolding | undefined;
  if (actor.holdingId !== undefined || actor.holdingSlug !== undefined) {
    if (!identifier(actor.holdingId) || !slug(actor.holdingSlug)) return invalidRequest();
    byIdentity = settings.holdings.find(
      holding =>
        holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
    );
  }
  if (!byOrigin && !byIdentity) return unknown();
  if (byOrigin && byIdentity && byOrigin !== byIdentity) return unknown();
  const target = byOrigin ?? byIdentity!;
  if (actor.companyId !== undefined) {
    if (!identifier(actor.companyId)) return invalidRequest();
    if (target.companyId !== undefined && target.companyId !== actor.companyId)
      return unavailable("FILES_COMMERCIAL_USAGE_COMPANY_MISMATCH");
  }
  return target;
}

/** POST with an explicit Host header; global fetch forbids overriding Host. */
const post: CommercialReadTransport = (url, headers, body, timeoutMs) =>
  new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import("node:http").ProxyEnv,
    });
    const outgoing = httpRequest(
      url,
      {
        method: "POST",
        agent,
        headers: {
          ...headers,
          "content-type": "application/json",
          "content-length": Buffer.byteLength(body),
        },
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
    outgoing.end(body);
  });

function row(value: unknown): value is CommercialStorageRow {
  return (
    object(value) &&
    typeof value.id === "string" &&
    value.id.length > 0 &&
    typeof value.company_id === "string" &&
    value.company_id.length > 0 &&
    (value.branch_id === null || typeof value.branch_id === "string") &&
    timestamp(value.bytes) &&
    timestamp(value.created_at) &&
    (value.destroyed_at === null || timestamp(value.destroyed_at))
  );
}

function payload(data: unknown, kind: FilesCommercialReadKind): FilesCommercialReadResult {
  const invalid = () => unavailable("FILES_COMMERCIAL_USAGE_RESPONSE_INVALID");
  if (!object(data) || data.kind !== kind) return invalid();
  switch (kind) {
    case FILES_CU_STORED_ORIGINALS:
      if (!Array.isArray(data.rows) || !data.rows.every(row)) return invalid();
      return { kind, rows: data.rows as CommercialStorageRow[] };
    case FILES_CU_UNRECONCILED_ORIGINALS:
      if (!Number.isSafeInteger(data.count) || (data.count as number) < 0) return invalid();
      return { kind, count: data.count as number };
    case FILES_CU_RECONCILIATION:
      if (typeof data.ready !== "boolean") return invalid();
      return { kind, ready: data.ready };
    case FILES_CU_EARLIEST_ORIGINAL:
      if (data.earliest !== null && !timestamp(data.earliest)) return invalid();
      return { kind, earliest: data.earliest as number | null };
    default:
      return invalid();
  }
}

/** Create a client with an injectable transport for contract tests. */
export function createCommercialReadClient(
  config: unknown,
  transport: CommercialReadTransport = post,
) {
  const settings = parseCommercialReadConfig(config);
  return {
    async read(
      actor: FilesCommercialActor | undefined,
      params: CommercialReadParams,
    ): Promise<FilesCommercialReadResult> {
      const invalidRequest = () =>
        unavailable("FILES_COMMERCIAL_USAGE_REQUEST_INVALID", "Use one bounded Files commercial-usage request.");
      if (!KINDS.has(params.kind)) return invalidRequest();
      const target = selectTarget(settings, actor as FilesCommercialActor);
      if (params.branchId !== undefined && params.branchId !== null && !identifier(params.branchId))
        return invalidRequest();
      if (params.start !== undefined && params.start !== null && !timestamp(params.start))
        return invalidRequest();
      if (params.end !== undefined && params.end !== null && !timestamp(params.end))
        return invalidRequest();

      const body = JSON.stringify({
        kind: params.kind,
        start: params.start ?? null,
        end: params.end ?? null,
        branchId: params.branchId ?? null,
      });
      if (new TextEncoder().encode(body).length > 16 * 1024) return invalidRequest();
      const url = new URL(
        `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/files/commercial-usage`,
        settings.base,
      );
      // Holding-wide reads omit the company header entirely; a scoped caller
      // still sends a validated company id.
      const companyId = actor?.companyId;
      const headers: Record<string, string> = {
        host: target.host,
        origin: target.origin,
        accept: "application/json",
        "cache-control": "no-cache",
      };
      if (companyId !== undefined) headers["x-erpx-context-company"] = companyId;
      let response: { status: number; value: unknown };
      try {
        response = await transport(url, headers, body, settings.timeoutMs);
      } catch {
        return unavailable();
      }
      const value: unknown = response.value;
      if (response.status !== 200) {
        const error = object(value) && object(value.error) ? value.error : {};
        if (response.status >= 400 && response.status < 500)
          throw new ApiError(
            response.status,
            text(error.code, 100) ? String(error.code) : "FILES_COMMERCIAL_USAGE_READ_FAILED",
            text(error.message, 300)
              ? String(error.message)
              : "The native Files commercial-usage read was rejected.",
          );
        return unavailable();
      }
      if (
        !object(value) ||
        !text(value.witness, 64) ||
        !/^[a-f0-9]{64}$/.test(String(value.witness))
      )
        return unavailable("FILES_COMMERCIAL_USAGE_RESPONSE_INVALID");
      return payload(value.data, params.kind);
    },
  };
}

/** Env-bound entry point. The caller's WorkContext is required; no Holding is
 * inferred from configuration order. */
export function readFilesCommercialUsage(
  params: CommercialReadParams & { kind: typeof FILES_CU_STORED_ORIGINALS },
  actor: FilesCommercialActor | undefined,
  environment?: Environment,
  transport?: CommercialReadTransport,
): Promise<Extract<FilesCommercialReadResult, { kind: typeof FILES_CU_STORED_ORIGINALS }>>;
export function readFilesCommercialUsage(
  params: CommercialReadParams & { kind: typeof FILES_CU_UNRECONCILED_ORIGINALS },
  actor: FilesCommercialActor | undefined,
  environment?: Environment,
  transport?: CommercialReadTransport,
): Promise<Extract<FilesCommercialReadResult, { kind: typeof FILES_CU_UNRECONCILED_ORIGINALS }>>;
export function readFilesCommercialUsage(
  params: CommercialReadParams & { kind: typeof FILES_CU_RECONCILIATION },
  actor: FilesCommercialActor | undefined,
  environment?: Environment,
  transport?: CommercialReadTransport,
): Promise<Extract<FilesCommercialReadResult, { kind: typeof FILES_CU_RECONCILIATION }>>;
export function readFilesCommercialUsage(
  params: CommercialReadParams & { kind: typeof FILES_CU_EARLIEST_ORIGINAL },
  actor: FilesCommercialActor | undefined,
  environment?: Environment,
  transport?: CommercialReadTransport,
): Promise<Extract<FilesCommercialReadResult, { kind: typeof FILES_CU_EARLIEST_ORIGINAL }>>;
export async function readFilesCommercialUsage(
  params: CommercialReadParams,
  actor: FilesCommercialActor | undefined,
  environment: Environment = process.env,
  transport?: CommercialReadTransport,
): Promise<FilesCommercialReadResult> {
  const raw = environment[ENV];
  if (!raw) return unavailable("FILES_COMMERCIAL_USAGE_CONFIG_REQUIRED");
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable("FILES_COMMERCIAL_USAGE_CONFIG_INVALID");
  }
  return createCommercialReadClient(value, transport).read(actor, params);
}
