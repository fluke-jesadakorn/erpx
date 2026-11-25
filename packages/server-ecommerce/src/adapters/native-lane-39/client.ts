import "server-only";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

/**
 * Strict private transport for the lane 39 Ecommerce owner-local workspace
 * channel/cart list family. The axum owner executes both SELECTs against
 * Ecommerce-owned tables, applies the Company/Branch/Department scope and
 * projects the rows. This bridge emits no SQL and has no fallback reader;
 * missing config, an unmatched work context, a transport failure or a drifted
 * reply all fail closed.
 */
export type NativeChannelRow = Readonly<{
  id: string;
  company_id: string;
  name: string;
  channel_type: "web" | "marketplace" | "social";
  configuration_json: string;
  status: "draft" | "active" | "rejected" | "retired";
  created_by: string;
  reviewed_by: string | null;
  version: number;
  created_at: number;
  updated_at: number;
}>;

export type NativeCartRow = Readonly<{
  id: string;
  company_id: string;
  branch_id: string;
  department_id: string | null;
  channel_id: string;
  party_id: string;
  warehouse_id: string;
  currency: string;
  status: "open" | "checked_out" | "abandoned";
  created_by: string;
  version: number;
  created_at: number;
  updated_at: number;
}>;

export type NativeWorkspaceLists = Readonly<{
  channels: readonly NativeChannelRow[];
  carts: readonly NativeCartRow[];
}>;

/** Opaque owner rows projected by the axum reader; no SQL is emitted here. */
export type NativeRow = Readonly<Record<string, string | number | null>>;
export type NativeWorkspaceRows = Readonly<{
  listings: readonly NativeRow[];
  checkouts: readonly NativeRow[];
}>;

export type NativeLane39Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;

export type Lane39HoldingContext = Pick<WorkContext, "holdingId" | "holdingSlug">;

export type WorkspaceListScope = Readonly<{
  companyId: string;
  branchId?: string | null;
  departmentId?: string | null;
}>;

export type NativeLane39Transport = (url: URL, init: RequestInit) => Promise<Response>;

type Settings = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly NativeLane39Holding[];
}>;

export const LANE39_ECOM_CONFIG_ENV = "ERPX_NATIVE_ECOM_LANE39_CONFIG";
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
const safeInt = (value: unknown, min = 0): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= min;

const unavailable = (): never => {
  throw new ApiError(503, "ECOMMERCE_WORKSPACE_LIST_UNAVAILABLE", "The native Ecommerce workspace service is unavailable.");
};
const misconfigured = (): never => {
  throw new ApiError(503, "ECOMMERCE_WORKSPACE_LIST_CONFIG_INVALID", "The native Ecommerce workspace transport is misconfigured.");
};
const invalidInput = (): never => {
  throw new ApiError(422, "ECOMMERCE_WORKSPACE_LIST_REQUEST_INVALID", "Use one bounded Ecommerce workspace scope.");
};
const invalidResponse = (): never => {
  throw new ApiError(503, "ECOMMERCE_WORKSPACE_LIST_RESPONSE_INVALID", "The native Ecommerce workspace reply was invalid.");
};
const invalidRows = (): never => {
  throw new ApiError(503, "ECOMMERCE_WORKSPACE_ROW_RESPONSE_INVALID", "The native Ecommerce workspace rows were invalid.");
};
const targetUnavailable = (): never => {
  throw new ApiError(503, "ECOMMERCE_WORKSPACE_LIST_TARGET_UNAVAILABLE", "The current Work Context has no registered Ecommerce workspace target.");
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
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

function selectHolding(config: Settings, actor: Lane39HoldingContext | undefined): NativeLane39Holding {
  if (!actor || !text(actor.holdingId, MAX_ID_LEN) || !text(actor.holdingSlug, 128))
    return targetUnavailable();
  const holding = config.holdings.find(
    (item) => item.holdingId === actor.holdingId && item.holdingSlug === actor.holdingSlug,
  );
  if (!holding) return targetUnavailable();
  return holding;
}

export function nativeLane39EcomConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[LANE39_ECOM_CONFIG_ENV] !== undefined;
}

const CHANNEL_FIELDS: readonly string[] = [
  "id", "company_id", "name", "channel_type", "configuration_json", "status",
  "created_by", "reviewed_by", "version", "created_at", "updated_at",
];
const CART_FIELDS: readonly string[] = [
  "id", "company_id", "branch_id", "department_id", "channel_id", "party_id",
  "warehouse_id", "currency", "status", "created_by", "version", "created_at", "updated_at",
];

function toChannel(value: unknown): NativeChannelRow {
  if (
    !record(value) ||
    !validId(value.id) ||
    !text(value.company_id, MAX_ID_LEN) ||
    !text(value.name, 2048) ||
    !["web", "marketplace", "social"].includes(String(value.channel_type)) ||
    !text(value.configuration_json, 64 * 1024) ||
    !["draft", "active", "rejected", "retired"].includes(String(value.status)) ||
    !text(value.created_by, MAX_ID_LEN) ||
    !(value.reviewed_by === null || validId(value.reviewed_by)) ||
    !safeInt(value.version, 1) ||
    !safeInt(value.created_at) ||
    !safeInt(value.updated_at) ||
    Object.keys(value).some((key) => !CHANNEL_FIELDS.includes(key))
  )
    return invalidResponse();
  return value as unknown as NativeChannelRow;
}

function toCart(value: unknown): NativeCartRow {
  if (
    !record(value) ||
    !validId(value.id) ||
    !text(value.company_id, MAX_ID_LEN) ||
    !validId(value.branch_id) ||
    !(value.department_id === null || validId(value.department_id)) ||
    !validId(value.channel_id) ||
    !validId(value.party_id) ||
    !validId(value.warehouse_id) ||
    !text(value.currency, 16) ||
    !["open", "checked_out", "abandoned"].includes(String(value.status)) ||
    !text(value.created_by, MAX_ID_LEN) ||
    !safeInt(value.version, 1) ||
    !safeInt(value.created_at) ||
    !safeInt(value.updated_at) ||
    Object.keys(value).some((key) => !CART_FIELDS.includes(key))
  )
    return invalidResponse();
  return value as unknown as NativeCartRow;
}

function toNativeRow(value: unknown): NativeRow {
  if (
    !record(value) ||
    Object.keys(value).length > 64 ||
    typeof value.id !== "string" ||
    !validId(value.id) ||
    typeof value.company_id !== "string" ||
    value.company_id.length > MAX_ID_LEN ||
    Object.values(value).some((item) => !(item === null || typeof item === "string" || (typeof item === "number" && Number.isFinite(item)))) ||
    JSON.stringify(value).length > 64 * 1024
  )
    return invalidRows();
  return value as NativeRow;
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

/** Build the lane 39 Ecommerce client. `transport` is injectable for tests. */
export function createLane39EcomClient(
  raw: string | undefined,
  transport: NativeLane39Transport = direct,
) {
  const config = configuration(raw);
  return {
    async workspaceLists(
      scope: WorkspaceListScope,
      actor: Lane39HoldingContext | undefined,
    ): Promise<NativeWorkspaceLists> {
      if (
        !record(scope) ||
        !validId(scope.companyId) ||
        !(scope.branchId === undefined || scope.branchId === null || validId(scope.branchId)) ||
        !(scope.departmentId === undefined || scope.departmentId === null || validId(scope.departmentId))
      )
        return invalidInput();
      const holding = selectHolding(config, actor);
      const url = new URL(
        `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/ecommerce/lane39-workspace-lists`,
        config.base,
      );
      const headers = new Headers({
        host: holding.host,
        origin: holding.origin,
        accept: "application/json",
        "cache-control": "no-cache",
        "x-erpx-context-company": scope.companyId,
      });
      if (scope.branchId) headers.set("x-erpx-workspace-branch", scope.branchId);
      if (scope.departmentId) headers.set("x-erpx-workspace-department", scope.departmentId);
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), config.timeoutMs);
      try {
        let reply: Response;
        try {
          reply = await transport(url, {
            method: "POST",
            headers,
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
              typeof error?.code === "string" ? error.code : "ECOMMERCE_WORKSPACE_LIST_READ_FAILED",
              typeof error?.message === "string" ? error.message : "The Ecommerce workspace read was rejected.",
            );
          return unavailable();
        }
        const body = await boundedJson(reply, controller.signal);
        if (
          !record(body) ||
          !Array.isArray(body.channels) ||
          !Array.isArray(body.carts) ||
          body.channels.length > MAX_ROWS ||
          body.carts.length > MAX_ROWS
        )
          return invalidResponse();
        return {
          channels: body.channels.map(toChannel),
          carts: body.carts.map(toCart),
        };
      } finally {
        clearTimeout(timer);
      }
    },

    async workspaceRows(
      scope: WorkspaceListScope,
      actor: Lane39HoldingContext | undefined,
    ): Promise<NativeWorkspaceRows> {
      if (
        !record(scope) ||
        !validId(scope.companyId) ||
        !(scope.branchId === undefined || scope.branchId === null || validId(scope.branchId)) ||
        !(scope.departmentId === undefined || scope.departmentId === null || validId(scope.departmentId))
      )
        return invalidInput();
      const holding = selectHolding(config, actor);
      const url = new URL(
        `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/ecommerce/lane39-workspace-rows`,
        config.base,
      );
      const headers = new Headers({
        host: holding.host,
        origin: holding.origin,
        accept: "application/json",
        "cache-control": "no-cache",
        "x-erpx-context-company": scope.companyId,
      });
      if (scope.branchId) headers.set("x-erpx-workspace-branch", scope.branchId);
      if (scope.departmentId) headers.set("x-erpx-workspace-department", scope.departmentId);
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), config.timeoutMs);
      try {
        let reply: Response;
        try {
          reply = await transport(url, {
            method: "POST",
            headers,
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
              typeof error?.code === "string" ? error.code : "ECOMMERCE_WORKSPACE_ROW_READ_FAILED",
              typeof error?.message === "string" ? error.message : "The Ecommerce workspace row read was rejected.",
            );
          return unavailable();
        }
        const body = await boundedJson(reply, controller.signal);
        if (
          !record(body) ||
          Object.keys(body).length !== 2 ||
          !Array.isArray(body.listings) ||
          !Array.isArray(body.checkouts) ||
          body.listings.length > MAX_ROWS ||
          body.checkouts.length > MAX_ROWS
        )
          return invalidRows();
        return {
          listings: body.listings.map(toNativeRow),
          checkouts: body.checkouts.map(toNativeRow),
        };
      } finally {
        clearTimeout(timer);
      }
    },
  };
}

/** Default consumer: reads the private config and performs both owner reads. */
export async function nativeWorkspaceLists(
  scope: WorkspaceListScope,
  actor: Lane39HoldingContext | undefined,
): Promise<NativeWorkspaceLists> {
  return createLane39EcomClient(process.env[LANE39_ECOM_CONFIG_ENV]).workspaceLists(scope, actor);
}

/** Default consumer for the workspace listings/checkouts row family. */
export async function nativeWorkspaceRows(
  scope: WorkspaceListScope,
  actor: Lane39HoldingContext | undefined,
): Promise<NativeWorkspaceRows> {
  return createLane39EcomClient(process.env[LANE39_ECOM_CONFIG_ENV]).workspaceRows(scope, actor);
}
