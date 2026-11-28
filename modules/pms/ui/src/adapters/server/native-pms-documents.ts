import "server-only";
import { Agent, request as httpRequest } from "node:http";
import { ApiError } from "@erpx/platform-runtime-server/public/http";

/**
 * Strict transport for the mounted owner document routes:
 * `GET /api/v1/holdings/{slug}/pms/native-documents/document?id=…` and
 * `GET /api/v1/holdings/{slug}/pms/native-documents/postings?property=…&id=…`.
 *
 * The axum owner executes the SQL and Company scope. This bridge issues no
 * SQL, accepts no SQL from a caller and has no fallback reader: an absent or
 * malformed configuration, an unregistered Holding, a rejected request or an
 * incomplete payload fails closed. The returned rows are the exact owner
 * projection, with no field renamed or derived.
 */
export type PmsDocumentActor = Readonly<{
  holdingSlug: string;
  holdingId: string;
  companyId: string;
}>;

export type PmsNativeDocument = Readonly<{
  id: string;
  property_id: string;
  guest_account_id: string;
  kind: string;
  finance_id: string | null;
  snapshot_json: string;
  created_at: number;
}>;

export type PmsNativePosting = Readonly<{ journal_id: string }>;

export type PmsDocumentsQueries = Readonly<{
  document: (id: string) => Promise<PmsNativeDocument | null>;
  relatedPostings: (property: string, id: string) => Promise<PmsNativePosting[]>;
}>;

export type PmsDocumentsConfig = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly Readonly<{
    origin: string;
    host: string;
    holdingId: string;
    holdingSlug: string;
  }>[];
}>;

export type PmsDocumentsTransport = (
  url: URL,
  headers: Readonly<Record<string, string>>,
  signal: AbortSignal,
) => Promise<Response>;

/** Shared PMS owner runtime configuration; no separate document env var. */
export const PMS_DOCUMENTS_CONFIG_KEY = "ERPX_NATIVE_PMS_CONFIG";

const RESERVED = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const unavailable = (message = "The native PMS document read is unavailable."): never => {
  throw new ApiError(503, "PMS_NATIVE_DOCUMENTS_UNAVAILABLE", message);
};
const record = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === "object" && !Array.isArray(v);
const exact = (v: unknown, keys: readonly string[]): v is Record<string, unknown> =>
  record(v) && Object.keys(v).length === keys.length && keys.every((key) => Object.hasOwn(v, key));
const idOk = (v: unknown): v is string =>
  typeof v === "string" && /^[A-Za-z0-9._:-]{1,200}$/.test(v);
const slugOk = (v: unknown): v is string =>
  typeof v === "string" && /^[a-z0-9_-]{1,128}$/.test(v);
const text = (v: unknown, max: number): v is string =>
  typeof v === "string" && new TextEncoder().encode(v).length <= max;
const integer = (v: unknown): v is number => typeof v === "number" && Number.isSafeInteger(v);

/** Parse the shared PMS owner config. Missing config always fails closed. */
export function pmsDocumentsConfig(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): PmsDocumentsConfig {
  const raw = environment[PMS_DOCUMENTS_CONFIG_KEY];
  if (raw === undefined) return unavailable("The native PMS document service is not configured.");
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable();
  }
  if (!record(value)) return unavailable();
  let base: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
  } catch {
    return unavailable();
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
    base.origin !== String(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable();

  const origins = new Set<string>();
  const hosts = new Set<string>();
  const ids = new Set<string>();
  const slugs = new Set<string>();
  const holdings: PmsDocumentsConfig["holdings"][number][] = [];
  for (const item of value.holdings as unknown[]) {
    if (!exact(item, ["origin", "holdingSlug", "holdingId"])) return unavailable();
    let origin: URL;
    try {
      origin = new URL(String(item.origin));
    } catch {
      return unavailable();
    }
    if (
      origin.protocol !== "https:" ||
      origin.origin !== String(item.origin) ||
      origin.username ||
      origin.password ||
      !slugOk(item.holdingSlug) ||
      !idOk(item.holdingId) ||
      origins.has(origin.origin) ||
      hosts.has(origin.host) ||
      ids.has(item.holdingId) ||
      slugs.has(item.holdingSlug)
    )
      return unavailable();
    origins.add(origin.origin);
    hosts.add(origin.host);
    ids.add(item.holdingId);
    slugs.add(item.holdingSlug);
    holdings.push({
      origin: origin.origin,
      host: origin.host,
      holdingId: item.holdingId,
      holdingSlug: item.holdingSlug,
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs as number, holdings };
}

/** True when the shared PMS owner config is present (callers still fail closed on bad config). */
export function pmsDocumentsConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[PMS_DOCUMENTS_CONFIG_KEY] !== undefined;
}

function select(cfg: PmsDocumentsConfig, actor: PmsDocumentActor): PmsDocumentsConfig["holdings"][number] {
  if (
    !record(actor) ||
    !idOk(actor.holdingId) ||
    !slugOk(actor.holdingSlug) ||
    !idOk(actor.companyId)
  )
    return unavailable();
  const matches = cfg.holdings.filter(
    (holding) => holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
  );
  if (matches.length !== 1) return unavailable("The native PMS document target is not registered.");
  return matches[0]!;
}

function get(url: URL, headers: Readonly<Record<string, string>>, signal: AbortSignal): Promise<Response> {
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
        signal,
        maxHeaderSize: 8192,
      },
      (incoming) => {
        const chunks: Buffer[] = [];
        let total = 0;
        incoming.on("data", (chunk: Buffer) => {
          total += chunk.byteLength;
          if (total > 4 * 1024 * 1024) {
            incoming.destroy();
            reject(new Error("PMS_NATIVE_DOCUMENTS_RESPONSE_LIMIT"));
            return;
          }
          chunks.push(chunk);
        });
        incoming.on("end", () => {
          agent.destroy();
          resolve(
            new Response(new Uint8Array(Buffer.concat(chunks)), {
              status: incoming.statusCode ?? 502,
              headers: new Headers(
                Object.entries(incoming.headers).flatMap(([key, value]) =>
                  value === undefined ? [] : [[key, Array.isArray(value) ? value.join(", ") : value]],
                ),
              ),
            }),
          );
        });
        incoming.on("error", (error) => {
          agent.destroy();
          reject(error);
        });
      },
    );
    outgoing.on("error", (error) => {
      agent.destroy();
      reject(error);
    });
    outgoing.end();
  });
}

async function payload(response: Response): Promise<unknown> {
  if (response.redirected) return unavailable();
  const contentType = response.headers.get("content-type")?.split(";")[0]?.trim();
  if (contentType !== "application/json") return unavailable();
  if (response.status !== 200) {
    const error = await response.json().catch((): unknown => null);
    if (response.status >= 400 && response.status < 500) {
      const detail = record(error) && record(error.error) ? error.error : {};
      throw new ApiError(
        response.status,
        typeof detail.code === "string" ? detail.code : "PMS_NATIVE_DOCUMENTS_REQUEST_INVALID",
        typeof detail.message === "string" ? detail.message : "The native PMS document read was rejected.",
      );
    }
    return unavailable();
  }
  return response.json().catch((): never => unavailable());
}

const documentRow = (value: unknown): PmsNativeDocument => {
  if (
    !exact(value, [
      "id",
      "property_id",
      "guest_account_id",
      "kind",
      "finance_id",
      "snapshot_json",
      "created_at",
    ]) ||
    !idOk(value.id) ||
    !idOk(value.property_id) ||
    !idOk(value.guest_account_id) ||
    !text(value.kind, 200) ||
    !(value.finance_id === null || idOk(value.finance_id)) ||
    !text(value.snapshot_json, 2 * 1024 * 1024) ||
    !integer(value.created_at)
  )
    return unavailable("The native PMS document payload is incomplete.");
  return value as unknown as PmsNativeDocument;
};

const postingRows = (value: unknown): PmsNativePosting[] => {
  if (
    !Array.isArray(value) ||
    value.length > 1_000 ||
    !value.every((row) => exact(row, ["journal_id"]) && idOk(row.journal_id))
  )
    return unavailable("The native PMS postings payload is incomplete.");
  return value as PmsNativePosting[];
};

/** Build a strict client. `transport` is injectable for bounded unit tests. */
export function createPmsDocumentsClient(
  cfg: PmsDocumentsConfig,
  transport: PmsDocumentsTransport = get,
): (actor: PmsDocumentActor) => PmsDocumentsQueries {
  return (actor) => {
    const target = select(cfg, actor);
    const headers: Record<string, string> = {
      host: target.host,
      origin: target.origin,
      accept: "application/json",
      "cache-control": "no-cache",
      "x-erpx-context-company": actor.companyId,
    };
    const call = async (path: "document" | "postings", query: ReadonlyArray<readonly [string, string]>): Promise<unknown> => {
      const url = new URL(
        `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/pms/native-documents/${path}`,
        cfg.base,
      );
      for (const [key, value] of query) url.searchParams.set(key, value);
      let response: Response;
      try {
        response = await transport(url, headers, AbortSignal.timeout(cfg.timeoutMs));
      } catch {
        return unavailable();
      }
      return payload(response);
    };
    return {
      document: async (id) => {
        if (!idOk(id)) return unavailable();
        const value = await call("document", [["id", id]]);
        if (!exact(value, ["document"])) return unavailable();
        return value.document === null ? null : documentRow(value.document);
      },
      relatedPostings: async (property, id) => {
        if (!idOk(property) || !idOk(id)) return unavailable();
        const value = await call("postings", [
          ["property", property],
          ["id", id],
        ]);
        if (!exact(value, ["postings"])) return unavailable();
        return value.postings === null ? [] : postingRows(value.postings);
      },
    };
  };
}

/** Presentation binding: every read is executed by the mounted owner routes. */
export function pmsNativeDocumentQueries(actor: PmsDocumentActor): PmsDocumentsQueries {
  return createPmsDocumentsClient(pmsDocumentsConfig())(actor);
}
