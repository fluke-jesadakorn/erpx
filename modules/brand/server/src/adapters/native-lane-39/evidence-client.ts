import "server-only";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import type {
  Lane39HoldingContext,
  NativeLane39Holding,
  NativeLane39Transport,
} from "./client";
import { LANE39_CONFIG_ENV } from "./client";

/**
 * Strict private transport for the lane 39 Brand executive evidence search.
 * The axum owner builds the exact registered statement with owner-controlled
 * predicates, executes it on its Holding pool and returns the projected rows.
 * This bridge sends only a bounded semantic scope: no SQL, no bind array and
 * no fallback reader. An unregistered Holding, a transport failure, an invalid
 * scope or a drifted reply fails closed.
 */
export type NativeEvidenceRow = Readonly<{
  id: string;
  branch_name: string;
  document_mode: "holding" | "branch" | "both";
  updated_at: number;
  primary_attachment_id: string | null;
  inverse_attachment_id: string | null;
}>;

export type EvidenceCursorScope = Readonly<{ branchName: string; id: string }>;

export type EvidenceScope = Readonly<{
  companyId: string;
  branchId?: string | null;
  from?: number | null;
  toExclusive?: number | null;
  query?: string | null;
  cursor?: EvidenceCursorScope | null;
  /** Owner LIMIT, including the caller's one-row `hasMore` lookahead. */
  limit: number;
}>;

type Settings = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly NativeLane39Holding[];
}>;

const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const MAX_ID_LEN = 200;
const MAX_QUERY_LEN = 100;
const MAX_CURSOR_LEN = 2048;
const MAX_LIMIT = 51;
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
  /^[A-Za-z0-9._:-]+$/.test(value);
const safeInt = (value: unknown, min = 0): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= min;

const unavailable = (): never => {
  throw new ApiError(503, "BRAND_EVIDENCE_SEARCH_UNAVAILABLE", "The native Brand evidence service is unavailable.");
};
const misconfigured = (): never => {
  throw new ApiError(503, "BRAND_EVIDENCE_SEARCH_CONFIG_INVALID", "The native Brand evidence transport is misconfigured.");
};
const invalidInput = (): never => {
  throw new ApiError(422, "BRAND_EVIDENCE_SEARCH_REQUEST_INVALID", "Use one bounded Brand evidence search.");
};
const invalidResponse = (): never => {
  throw new ApiError(503, "BRAND_EVIDENCE_SEARCH_RESPONSE_INVALID", "The native Brand evidence reply was invalid.");
};
const targetUnavailable = (): never => {
  throw new ApiError(503, "BRAND_EVIDENCE_SEARCH_TARGET_UNAVAILABLE", "The current Work Context has no registered Brand evidence target.");
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

/** Exact WorkContext selection; there is deliberately no first-holding default. */
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

export function nativeLane39EvidenceConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[LANE39_CONFIG_ENV] !== undefined;
}

const ROW_FIELDS: readonly string[] = [
  "id", "branch_name", "document_mode", "updated_at", "primary_attachment_id",
  "inverse_attachment_id",
];

function toRow(value: unknown): NativeEvidenceRow {
  if (
    !record(value) ||
    !validId(value.id) ||
    !text(value.branch_name, MAX_CURSOR_LEN) ||
    !["holding", "branch", "both"].includes(String(value.document_mode)) ||
    !safeInt(value.updated_at) ||
    !(value.primary_attachment_id === null || validId(value.primary_attachment_id)) ||
    !(value.inverse_attachment_id === null || validId(value.inverse_attachment_id)) ||
    Object.keys(value).some((key) => !ROW_FIELDS.includes(key))
  )
    return invalidResponse();
  return value as unknown as NativeEvidenceRow;
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

/** Build the lane 39 Brand evidence client. `transport` is injectable for tests. */
export function createLane39EvidenceClient(
  raw: string | undefined,
  transport: NativeLane39Transport = direct,
) {
  const config = configuration(raw);
  return {
    async search(scope: EvidenceScope, actor: Lane39HoldingContext | undefined): Promise<NativeEvidenceRow[]> {
      const from = scope.from ?? null;
      const to = scope.toExclusive ?? null;
      const hasFrom = from !== null;
      const hasTo = to !== null;
      const query = scope.query ?? null;
      const cursor = scope.cursor === undefined || scope.cursor === null ? null : scope.cursor;
      if (
        !record(scope) ||
        !validId(scope.companyId) ||
        !(scope.branchId === undefined || scope.branchId === null || validId(scope.branchId)) ||
        hasFrom !== hasTo ||
        (hasFrom && (!safeInt(from) || !safeInt(to) || from >= to)) ||
        (query !== null && (typeof query !== "string" || new TextEncoder().encode(query).length > MAX_QUERY_LEN)) ||
        (cursor !== null &&
          (!record(cursor) ||
            !text(cursor.branchName, MAX_CURSOR_LEN) ||
            !validId(cursor.id))) ||
        !safeInt(scope.limit, 1) ||
        scope.limit > MAX_LIMIT
      )
        return invalidInput();
      const holding = selectHolding(config, actor);
      const url = new URL(
        `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/branding/lane39-evidence-search`,
        config.base,
      );
      const headers = new Headers({
        host: holding.host,
        origin: holding.origin,
        accept: "application/json",
        "cache-control": "no-cache",
        "x-erpx-context-company": scope.companyId,
      });
      const body = {
        companyId: scope.companyId,
        ...(scope.branchId ? { branchId: scope.branchId } : {}),
        ...(hasFrom ? { from, toExclusive: to } : {}),
        ...(query !== null ? { query } : {}),
        ...(cursor !== null ? { cursor: { branchName: cursor.branchName, id: cursor.id } } : {}),
        limit: scope.limit,
      };
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), config.timeoutMs);
      try {
        let reply: Response;
        try {
          reply = await transport(url, {
            method: "POST",
            headers,
            body: JSON.stringify(body),
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
              typeof error?.code === "string" ? error.code : "BRAND_EVIDENCE_SEARCH_READ_FAILED",
              typeof error?.message === "string" ? error.message : "The Brand evidence read was rejected.",
            );
          return unavailable();
        }
        const payload = await boundedJson(reply, controller.signal);
        if (!record(payload) || !Array.isArray(payload.rows) || payload.rows.length > MAX_LIMIT)
          return invalidResponse();
        return payload.rows.map(toRow);
      } finally {
        clearTimeout(timer);
      }
    },
  };
}

/** Default consumer: reads the private config and executes one owner search. */
export async function nativeBrandEvidence(
  scope: EvidenceScope,
  actor: Lane39HoldingContext | undefined,
): Promise<NativeEvidenceRow[]> {
  return createLane39EvidenceClient(process.env[LANE39_CONFIG_ENV]).search(scope, actor);
}
